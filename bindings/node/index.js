'use strict';
// Binding de geoda para Node (HU-064): una capa fina sobre `build/Release/geoda_native.node`.
// Una matriz matematica es un arreglo de filas; una tabla de datos (kmeans) es un arreglo de columnas.

const native = require('./build/Release/geoda_native.node');

const DTYPE = Object.freeze({ BOOL: 0, INT32: 1, INT64: 2, FLOAT64: 3, UTF8: 4 });
const KIND = { float64: 0, int32: 1, int64: 2, bool: 3 };
const OP = { count: 0, sum: 1, min: 2, max: 3, mean: 4, var: 5, std: 6, median: 7, percentile: 8 };
const LIST = { matmul: 0, solve: 1, det: 2, inverse: 3 };

/** Columna de geoda. Se libera con `free()` (idempotente) o, si se olvida, al recogerla el GC. */
class Column {
  #handle;

  constructor(handle) {
    this.#handle = handle;
  }

  static fromFloat64(values, validity) {
    return new Column(native.fromTyped(KIND.float64, values, validity));
  }
  static fromInt32(values, validity) {
    return new Column(native.fromTyped(KIND.int32, values, validity));
  }
  static fromBigInt64(values, validity) {
    return new Column(native.fromTyped(KIND.int64, values, validity));
  }
  /** Booleanos como `Uint8Array` de 0 y 1. */
  static fromBool(values, validity) {
    return new Column(native.fromTyped(KIND.bool, values, validity));
  }
  /** Texto: un `null` es un nulo. */
  static fromStrings(strings) {
    const encoder = new TextEncoder();
    const parts = [];
    const offsets = new Int32Array(strings.length + 1);
    const validity = new Uint8Array(strings.length);
    let total = 0;
    strings.forEach((text, i) => {
      validity[i] = text === null || text === undefined ? 0 : 1;
      if (validity[i]) {
        const bytes = encoder.encode(text);
        parts.push(bytes);
        total += bytes.length;
      }
      offsets[i + 1] = total;
    });
    const data = new Uint8Array(total);
    let at = 0;
    for (const part of parts) {
      data.set(part, at);
      at += part.length;
    }
    return new Column(native.fromUtf8(data, offsets, validity));
  }

  /** @internal */
  get handle() {
    if (this.#handle === null) throw new Error('la columna ya se libero');
    return this.#handle;
  }

  get length() {
    return native.info(this.handle)[0];
  }
  get nullCount() {
    return native.info(this.handle)[1];
  }
  /** Una de `DTYPE`. */
  get dtype() {
    return native.info(this.handle)[2];
  }
  /** 1 = valida, 0 = nula. */
  validity() {
    return native.validity(this.handle);
  }
  toFloat64Array() {
    return native.copy(this.handle, KIND.float64);
  }
  toInt32Array() {
    return native.copy(this.handle, KIND.int32);
  }
  toBigInt64Array() {
    return native.copy(this.handle, KIND.int64);
  }
  toBoolArray() {
    return native.copy(this.handle, KIND.bool);
  }
  /** `null` en los nulos. */
  toStrings() {
    const { data, offsets } = native.utf8(this.handle);
    const valid = this.validity();
    const decoder = new TextDecoder();
    return Array.from(valid, (ok, i) => (ok ? decoder.decode(data.subarray(offsets[i], offsets[i + 1])) : null));
  }
  free() {
    if (this.#handle !== null) {
      native.release(this.#handle);
      this.#handle = null;
    }
  }
}

const scalar = (op, column, arg) => native.scalar(OP[op], column.handle, arg);

/** Filas -> columnas de geoda (la columna j de la matriz es la j-esima de la lista). */
function columnsOfRows(rows) {
  if (!Array.isArray(rows) || rows.length === 0) throw new TypeError('la matriz necesita al menos una fila');
  const width = rows[0].length;
  if (rows.some((row) => row.length !== width)) {
    const error = new Error('LENGTH_MISMATCH: las filas miden distinto');
    error.code = -3;
    error.category = 'LENGTH_MISMATCH';
    throw error;
  }
  return Array.from({ length: width }, (_, j) => Column.fromFloat64(Float64Array.from(rows, (row) => row[j])));
}

function withColumns(columns, body) {
  try {
    return body(columns.map((c) => c.handle));
  } finally {
    columns.forEach((c) => c.free());
  }
}

function reshape(flat, rows, cols) {
  return Array.from({ length: rows }, (_, i) => Array.from(flat.subarray(i * cols, (i + 1) * cols)));
}

function takeFlat(handle, rows, cols) {
  const column = new Column(handle);
  try {
    return reshape(column.toFloat64Array(), rows, cols);
  } finally {
    column.free();
  }
}

module.exports = {
  DTYPE,
  Column,
  abiVersion: () => native.abiVersion(),

  count: (column) => scalar('count', column),
  /** `bigint` para columnas enteras, `number` si no (igual `min` y `max`). */
  sum: (column) => scalar('sum', column),
  min: (column) => scalar('min', column),
  max: (column) => scalar('max', column),
  mean: (column) => scalar('mean', column),
  /** Varianza con `ddof` grados de libertad restados (1 = muestral). */
  variance: (column, ddof = 1) => scalar('var', column, ddof),
  std: (column, ddof = 1) => scalar('std', column, ddof),
  median: (column) => scalar('median', column),
  percentile: (column, p) => scalar('percentile', column, p),
  corr: (x, y) => native.pair(0, x.handle, y.handle),

  /** Distancia de edicion entre dos cadenas, o fila a fila entre dos columnas Utf8. */
  editDistance(a, b) {
    if (typeof a === 'string' && typeof b === 'string') {
      const left = Column.fromStrings([a]);
      const right = Column.fromStrings([b]);
      try {
        const result = new Column(native.pair(1, left.handle, right.handle));
        try {
          return Number(result.toBigInt64Array()[0]);
        } finally {
          result.free();
        }
      } finally {
        left.free();
        right.free();
      }
    }
    return new Column(native.pair(1, a.handle, b.handle));
  },

  matmul: (a, b) =>
    withColumns(columnsOfRows(a), (left) =>
      withColumns(columnsOfRows(b), (right) => takeFlat(native.list(LIST.matmul, left, right), a.length, b[0].length)),
    ),
  solve(a, b) {
    const rhs = Column.fromFloat64(Float64Array.from(b));
    try {
      return withColumns(columnsOfRows(a), (list) => {
        const x = new Column(native.list(LIST.solve, list, rhs.handle));
        try {
          return Array.from(x.toFloat64Array());
        } finally {
          x.free();
        }
      });
    } finally {
      rhs.free();
    }
  },
  /** Determinante; una matriz singular da 0 (no es un error). */
  det: (a) => withColumns(columnsOfRows(a), (list) => native.list(LIST.det, list)),
  inverse: (a) => withColumns(columnsOfRows(a), (list) => takeFlat(native.list(LIST.inverse, list), a.length, a.length)),

  /** Factorizacion QR (Householder) de una matriz por filas con al menos tantas filas como columnas. */
  qr: (a) =>
    withColumns(columnsOfRows(a), (list) => {
      const [q, r] = native.factor(0, list);
      return { q: takeFlat(q, a.length, a[0].length), r: takeFlat(r, a[0].length, a[0].length) };
    }),
  /** SVD delgada `a = u diag(s) vt` (valores de mayor a menor, signo determinista). */
  svd: (a) =>
    withColumns(columnsOfRows(a), (list) => {
      const k = Math.min(a.length, a[0].length);
      const [u, s, vt] = native.factor(1, list);
      const values = new Column(s);
      try {
        return { u: takeFlat(u, a.length, k), s: Array.from(values.toFloat64Array()), vt: takeFlat(vt, k, a[0].length) };
      } finally {
        values.free();
      }
    }),
  /** Minimos cuadrados `min |a x - b|` (QR): los coeficientes y la norma del residuo. */
  lstsq(a, b) {
    const rhs = Column.fromFloat64(Float64Array.from(b));
    try {
      return withColumns(columnsOfRows(a), (list) => {
        const { x, residual } = native.lstsq(list, rhs.handle);
        const coefficients = new Column(x);
        try {
          return { coefficients: Array.from(coefficients.toFloat64Array()), residualNorm: residual };
        } finally {
          coefficients.free();
        }
      });
    } finally {
      rhs.free();
    }
  },

  /** Posiciones de la subsecuencia estrictamente creciente mas larga de una columna numerica sin nulos. */
  lis(column) {
    const result = new Column(native.lis(column.handle));
    try {
      return Array.from(result.toBigInt64Array(), Number);
    } finally {
      result.free();
    }
  },
  /** Agrupa las cadenas a distancia de edicion <= `maxDistance` (componentes conexas); `ids[i]` es `null` en los nulos. */
  dedupe(column, maxDistance = 1) {
    const { ids, count } = native.dedupe(column.handle, maxDistance);
    const result = new Column(ids);
    try {
      const valid = result.validity();
      return { ids: Array.from(result.toBigInt64Array(), (v, i) => (valid[i] ? Number(v) : null)), count };
    } finally {
      result.free();
    }
  },
  /** Histograma de ancho fijo; `range` = `[lo, hi]` o `undefined` para `[min, max]`. */
  histogram(column, bins, range) {
    const raw = native.histogram(column.handle, bins, range !== undefined, range?.[0] ?? 0, range?.[1] ?? 0);
    const [edges, counts, outside] = [raw.edges, raw.counts, raw.outside].map((h) => new Column(h));
    try {
      const [below, above, nulls, nans] = Array.from(outside.toBigInt64Array(), Number);
      return {
        edges: Array.from(edges.toFloat64Array()),
        counts: Array.from(counts.toBigInt64Array(), Number),
        below,
        above,
        nulls,
        nans,
      };
    } finally {
      [edges, counts, outside].forEach((c) => c.free());
    }
  },

  /** `variables` es un arreglo de columnas (cada una `Float64Array` o `number[]`). */
  kmeans(variables, k, { seed = 0n, maxIter = 100, tol = 1e-4 } = {}) {
    const columns = variables.map((v) => Column.fromFloat64(Float64Array.from(v)));
    return withColumns(columns, (list) => {
      const result = native.kmeans(list, k, maxIter, tol, BigInt(seed));
      const labels = new Column(result.labels);
      const centroids = new Column(result.centroids);
      try {
        return {
          labels: Array.from(labels.toBigInt64Array(), Number),
          centroids: reshape(centroids.toFloat64Array(), k, variables.length),
          inertia: result.inertia,
          iterations: result.iterations,
        };
      } finally {
        labels.free();
        centroids.free();
      }
    });
  },
};
