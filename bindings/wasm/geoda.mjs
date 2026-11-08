// Cargador de geoda para WebAssembly (HU-065): la C-API (ABI 1.x) compilada a `wasm32-unknown-unknown`
// y una capa fina de JavaScript. Sin `wasm-bindgen`, sin WASI y sin hilos (`workers` siempre es 1).
//
//   import { Geoda } from './geoda.mjs';
//   const geoda = await Geoda.load(await fs.readFile('geoda.wasm'));     // Node
//   const geoda = await Geoda.load(fetch('geoda.wasm'));                 // navegador
//   const column = geoda.Column.fromFloat64(Float64Array.of(1.5, 99, 3.5), Uint8Array.of(1, 0, 1));
//   geoda.mean(column);   // 2.5
//   column.free();
//
// Una matriz matematica es un arreglo de filas; una tabla de datos (kmeans) es un arreglo de columnas.
// La memoria de WebAssembly puede crecer en cualquier llamada y eso invalida los `ArrayBuffer`: por eso
// las vistas se crean siempre justo antes de usarlas.

const DTYPE = Object.freeze({ BOOL: 0, INT32: 1, INT64: 2, FLOAT64: 3, UTF8: 4 });

const CATEGORY = {
  [-1]: 'INVALID_ARGUMENT',
  [-2]: 'TYPE_MISMATCH',
  [-3]: 'LENGTH_MISMATCH',
  [-4]: 'OUT_OF_BOUNDS',
  [-5]: 'UNSUPPORTED',
  [-6]: 'ALLOC',
  [-7]: 'FFI',
  [-8]: 'PANIC',
};

/** Error de la C-API: `code` negativo y `category` (`INVALID_ARGUMENT`, `TYPE_MISMATCH`...). */
export class GeodaError extends Error {
  constructor(code, message) {
    const category = CATEGORY[code] ?? 'INTERNAL';
    super(`${category}: ${message}`);
    this.name = 'GeodaError';
    this.code = code;
    this.category = category;
  }
}

export class Geoda {
  #exports;
  #registry;
  /** Clase de columna ligada a esta instancia (cada instancia tiene su propia memoria). */
  Column;
  DTYPE = DTYPE;

  /**
   * Instancia el modulo. `source` es un `ArrayBuffer` o `Uint8Array` con el `.wasm`, o una `Response`
   * (o una promesa de ella, como `fetch('geoda.wasm')`) para instanciar en streaming.
   */
  static async load(source) {
    const awaited = await source;
    let instance;
    if (typeof Response !== 'undefined' && awaited instanceof Response) {
      ({ instance } = await WebAssembly.instantiateStreaming(awaited, {}));
    } else {
      ({ instance } = await WebAssembly.instantiate(awaited, {}));
    }
    return new Geoda(instance.exports);
  }

  constructor(exports) {
    this.#exports = exports;
    const geoda = this;
    this.#registry = new FinalizationRegistry((pointer) => exports.geoda_column_free(pointer));
    const registry = this.#registry;

    /** Columna de geoda. Se libera con `free()` (idempotente) o, si se olvida, al recogerla el GC. */
    this.Column = class Column {
      #pointer;
      constructor(pointer) {
        this.#pointer = pointer;
        registry.register(this, pointer, this);
      }
      /** @internal */
      get pointer() {
        if (this.#pointer === 0) throw new Error('la columna ya se libero');
        return this.#pointer;
      }

      static fromFloat64(values, validity) {
        return geoda.#fromTyped('geoda_column_from_f64', Float64Array, values, validity);
      }
      static fromInt32(values, validity) {
        return geoda.#fromTyped('geoda_column_from_i32', Int32Array, values, validity);
      }
      static fromBigInt64(values, validity) {
        return geoda.#fromTyped('geoda_column_from_i64', BigInt64Array, values, validity);
      }
      /** Booleanos como `Uint8Array` de 0 y 1. */
      static fromBool(values, validity) {
        return geoda.#fromTyped('geoda_column_from_bool', Uint8Array, values, validity);
      }
      /** Texto: un `null` es un nulo. */
      static fromStrings(strings) {
        return geoda.#fromStrings(strings);
      }

      get length() {
        return geoda.#info(this.pointer).length;
      }
      get nullCount() {
        return geoda.#info(this.pointer).nullCount;
      }
      /** Una de `DTYPE`. */
      get dtype() {
        return geoda.#info(this.pointer).dtype;
      }
      /** 1 = valida, 0 = nula. */
      validity() {
        return geoda.#copy(this.pointer, 'geoda_column_copy_validity', Uint8Array);
      }
      toFloat64Array() {
        return geoda.#copy(this.pointer, 'geoda_column_copy_f64', Float64Array);
      }
      toInt32Array() {
        return geoda.#copy(this.pointer, 'geoda_column_copy_i32', Int32Array);
      }
      toBigInt64Array() {
        return geoda.#copy(this.pointer, 'geoda_column_copy_i64', BigInt64Array);
      }
      toBoolArray() {
        return geoda.#copy(this.pointer, 'geoda_column_copy_bool', Uint8Array);
      }
      /** `null` en los nulos. */
      toStrings() {
        return geoda.#toStrings(this);
      }
      free() {
        if (this.#pointer !== 0) {
          const pointer = this.#pointer;
          this.#pointer = 0;
          registry.unregister(this);
          exports.geoda_column_free(pointer);
        }
      }
    };
  }

  // --- Memoria y errores.

  get #memory() {
    return this.#exports.memory.buffer;
  }

  #alloc(bytes) {
    const pointer = this.#exports.geoda_alloc(Math.max(bytes, 8));
    if (pointer === 0) throw new GeodaError(-6, `sin memoria para ${bytes} bytes`);
    return pointer;
  }

  #free(pointer, bytes) {
    this.#exports.geoda_dealloc(pointer, Math.max(bytes, 8));
  }

  /** Ejecuta `body(scope)`; todo lo que reserve `scope.put`/`scope.cell` se libera al terminar. */
  #scoped(body) {
    const owned = [];
    const scope = {
      /** Copia un typed array a la memoria de wasm y devuelve su direccion (0 si es `undefined`). */
      put: (array) => {
        if (array === undefined || array === null) return 0;
        const bytes = array.byteLength;
        const pointer = this.#alloc(bytes);
        owned.push([pointer, bytes]);
        new Uint8Array(this.#memory, pointer, bytes).set(new Uint8Array(array.buffer, array.byteOffset, bytes));
        return pointer;
      },
      /** Una celda de 8 bytes para un `out`. */
      cell: () => {
        const pointer = this.#alloc(8);
        owned.push([pointer, 8]);
        new Uint8Array(this.#memory, pointer, 8).fill(0);
        return pointer;
      },
      /** Un buffer vacio de `bytes` bytes. */
      buffer: (bytes) => {
        const pointer = this.#alloc(bytes);
        owned.push([pointer, bytes]);
        return pointer;
      },
      u32: (cell) => new DataView(this.#memory).getUint32(cell, true),
      f64: (cell) => new DataView(this.#memory).getFloat64(cell, true),
      i64: (cell) => new DataView(this.#memory).getBigInt64(cell, true),
    };
    try {
      return body(scope);
    } finally {
      for (const [pointer, bytes] of owned) this.#free(pointer, bytes);
    }
  }

  #lastError() {
    const start = this.#exports.geoda_last_error_message();
    const bytes = new Uint8Array(this.#memory);
    let end = start;
    while (bytes[end] !== 0) end++;
    return new TextDecoder().decode(bytes.subarray(start, end));
  }

  /** Lanza una `GeodaError` si `code` es un error de la capi. */
  #check(code) {
    if (code !== 0) throw new GeodaError(code, this.#lastError());
  }

  #wrap(pointer) {
    return new this.Column(pointer);
  }

  // --- Columnas.

  #fromTyped(name, Type, values, validity) {
    if (!(values instanceof Type)) throw new TypeError(`se esperaba un ${Type.name}`);
    if (validity !== undefined && !(validity instanceof Uint8Array)) throw new TypeError('validity debe ser un Uint8Array');
    // La capi lee `len` bytes de validez: aqui se comprueba el largo (en Node lo hace el addon).
    if (validity !== undefined && validity.length !== values.length) {
      throw new GeodaError(-3, 'validity mide distinto que values');
    }
    return this.#scoped((scope) => {
      const out = scope.cell();
      this.#check(this.#exports[name](scope.put(values), values.length, scope.put(validity), out));
      return this.#wrap(scope.u32(out));
    });
  }

  #fromStrings(strings) {
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
    return this.#scoped((scope) => {
      const out = scope.cell();
      this.#check(
        this.#exports.geoda_column_from_utf8(scope.put(data), total, scope.put(offsets), strings.length, scope.put(validity), out),
      );
      return this.#wrap(scope.u32(out));
    });
  }

  #info(pointer) {
    const length = this.#exports.geoda_column_len(pointer); // i64 -> BigInt
    const nullCount = this.#exports.geoda_column_null_count(pointer);
    const dtype = this.#exports.geoda_column_dtype(pointer);
    for (const value of [length, nullCount, dtype]) {
      if (value < 0) this.#check(Number(value));
    }
    return { length: Number(length), nullCount: Number(nullCount), dtype };
  }

  #copy(pointer, name, Type) {
    const { length } = this.#info(pointer);
    return this.#scoped((scope) => {
      const bytes = length * Type.BYTES_PER_ELEMENT;
      const buffer = scope.buffer(bytes);
      this.#check(this.#exports[name](pointer, buffer, length));
      return new Type(this.#memory.slice(buffer, buffer + bytes));
    });
  }

  #toStrings(column) {
    const { length } = this.#info(column.pointer);
    const valid = column.validity();
    return this.#scoped((scope) => {
      const total = scope.cell();
      this.#check(this.#exports.geoda_column_utf8_data_len(column.pointer, total));
      const bytes = scope.u32(total);
      const data = scope.buffer(bytes);
      const offsets = scope.buffer(4 * (length + 1));
      this.#check(this.#exports.geoda_column_copy_utf8(column.pointer, data, bytes, offsets, length + 1));
      const view = new Int32Array(this.#memory.slice(offsets, offsets + 4 * (length + 1)));
      const raw = new Uint8Array(this.#memory.slice(data, data + bytes));
      const decoder = new TextDecoder();
      return Array.from(valid, (ok, i) => (ok ? decoder.decode(raw.subarray(view[i], view[i + 1])) : null));
    });
  }

  // --- Estadisticos escalares.

  abiVersion() {
    return this.#exports.geoda_abi_version();
  }

  #real(call) {
    return this.#scoped((scope) => {
      const out = scope.cell();
      this.#check(call(out));
      return scope.f64(out);
    });
  }

  #whole(call) {
    return this.#scoped((scope) => {
      const out = scope.cell();
      this.#check(call(out));
      return scope.i64(out);
    });
  }

  #isInteger(column) {
    return column.dtype === DTYPE.INT32 || column.dtype === DTYPE.INT64;
  }

  count(column) {
    return Number(this.#whole((out) => this.#exports.geoda_column_count(column.pointer, out)));
  }
  /** `bigint` para columnas enteras, `number` si no (igual `min` y `max`). */
  sum(column) {
    return this.#isInteger(column)
      ? this.#whole((out) => this.#exports.geoda_column_sum_i64(column.pointer, 1, out))
      : this.#real((out) => this.#exports.geoda_column_sum_f64(column.pointer, 1, out));
  }
  min(column) {
    return this.#isInteger(column)
      ? this.#whole((out) => this.#exports.geoda_column_min_i64(column.pointer, 1, out))
      : this.#real((out) => this.#exports.geoda_column_min_f64(column.pointer, 1, out));
  }
  max(column) {
    return this.#isInteger(column)
      ? this.#whole((out) => this.#exports.geoda_column_max_i64(column.pointer, 1, out))
      : this.#real((out) => this.#exports.geoda_column_max_f64(column.pointer, 1, out));
  }
  mean(column) {
    return this.#real((out) => this.#exports.geoda_column_mean(column.pointer, 1, out));
  }
  /** Varianza con `ddof` grados de libertad restados (1 = muestral). */
  variance(column, ddof = 1) {
    return this.#real((out) => this.#exports.geoda_column_var(column.pointer, ddof, 1, out));
  }
  std(column, ddof = 1) {
    return this.#real((out) => this.#exports.geoda_column_std(column.pointer, ddof, 1, out));
  }
  median(column) {
    return this.#real((out) => this.#exports.geoda_column_median(column.pointer, out));
  }
  percentile(column, p) {
    return this.#real((out) => this.#exports.geoda_column_percentile(column.pointer, p, out));
  }
  corr(x, y) {
    return this.#real((out) => this.#exports.geoda_column_corr(x.pointer, y.pointer, 1, out));
  }

  /** Distancia de edicion entre dos cadenas, o fila a fila entre dos columnas Utf8. */
  editDistance(a, b) {
    if (typeof a === 'string' && typeof b === 'string') {
      const left = this.Column.fromStrings([a]);
      const right = this.Column.fromStrings([b]);
      try {
        const result = this.editDistance(left, right);
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
    return this.#scoped((scope) => {
      const out = scope.cell();
      this.#check(this.#exports.geoda_column_edit_distance(a.pointer, b.pointer, out));
      return this.#wrap(scope.u32(out));
    });
  }

  // --- Algebra lineal (matrices por filas) y k-means.

  #columnsOfRows(rows) {
    if (!Array.isArray(rows) || rows.length === 0) throw new TypeError('la matriz necesita al menos una fila');
    const width = rows[0].length;
    if (rows.some((row) => row.length !== width)) throw new GeodaError(-3, 'las filas miden distinto');
    return Array.from({ length: width }, (_, j) => this.Column.fromFloat64(Float64Array.from(rows, (row) => row[j])));
  }

  #withColumns(columns, body) {
    try {
      return this.#scoped((scope) => {
        const list = scope.buffer(4 * columns.length);
        new Uint32Array(this.#memory, list, columns.length).set(columns.map((c) => c.pointer));
        return body(scope, list, columns.length);
      });
    } finally {
      columns.forEach((c) => c.free());
    }
  }

  #takeFlat(pointer, rows, cols) {
    const column = this.#wrap(pointer);
    try {
      const flat = column.toFloat64Array();
      return Array.from({ length: rows }, (_, i) => Array.from(flat.subarray(i * cols, (i + 1) * cols)));
    } finally {
      column.free();
    }
  }

  matmul(a, b) {
    return this.#withColumns(this.#columnsOfRows(a), (scope, left, nLeft) =>
      this.#withColumns(this.#columnsOfRows(b), (scope2, right, nRight) => {
        const out = scope2.cell();
        this.#check(this.#exports.geoda_matmul(left, nLeft, right, nRight, out));
        return this.#takeFlat(scope2.u32(out), a.length, b[0].length);
      }),
    );
  }

  solve(a, b) {
    const rhs = this.Column.fromFloat64(Float64Array.from(b));
    try {
      return this.#withColumns(this.#columnsOfRows(a), (scope, list, n) => {
        const out = scope.cell();
        this.#check(this.#exports.geoda_lu_solve(list, n, rhs.pointer, out));
        const x = this.#wrap(scope.u32(out));
        try {
          return Array.from(x.toFloat64Array());
        } finally {
          x.free();
        }
      });
    } finally {
      rhs.free();
    }
  }

  /** Determinante; una matriz singular da 0 (no es un error). */
  det(a) {
    return this.#withColumns(this.#columnsOfRows(a), (scope, list, n) => {
      const out = scope.cell();
      this.#check(this.#exports.geoda_det(list, n, out));
      return scope.f64(out);
    });
  }

  inverse(a) {
    return this.#withColumns(this.#columnsOfRows(a), (scope, list, n) => {
      const out = scope.cell();
      this.#check(this.#exports.geoda_inverse(list, n, out));
      return this.#takeFlat(scope.u32(out), a.length, a.length);
    });
  }

  /** `variables` es un arreglo de columnas (cada una `Float64Array` o `number[]`). */
  kmeans(variables, k, { seed = 0n, maxIter = 100, tol = 1e-4 } = {}) {
    const columns = variables.map((v) => this.Column.fromFloat64(Float64Array.from(v)));
    return this.#withColumns(columns, (scope, list, n) => {
      const [labels, centroids, inertia, iterations] = [scope.cell(), scope.cell(), scope.cell(), scope.cell()];
      this.#check(this.#exports.geoda_kmeans(list, n, k, maxIter, tol, BigInt(seed), labels, centroids, inertia, iterations));
      const labelColumn = this.#wrap(scope.u32(labels));
      const centroidColumn = this.#wrap(scope.u32(centroids));
      try {
        const flat = centroidColumn.toFloat64Array();
        return {
          labels: Array.from(labelColumn.toBigInt64Array(), Number),
          centroids: Array.from({ length: k }, (_, i) => Array.from(flat.subarray(i * variables.length, (i + 1) * variables.length))),
          inertia: scope.f64(inertia),
          iterations: Number(scope.i64(iterations)),
        };
      } finally {
        labelColumn.free();
        centroidColumn.free();
      }
    });
  }

  /** Las funciones exportadas por el modulo (para quien quiera llamar la C-API directamente). */
  get exports() {
    return this.#exports;
  }
}
