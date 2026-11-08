// Runner compartido de `spec/` para los bindings de JavaScript (Node y WASM, decision 015 O5).
//
// Corre el subconjunto de `spec/cases/` que la superficie minima soporta. Lo que no puede expresar
// JavaScript (nulos dentro de una matriz de `Float64Array`, listas de columnas vacias, texto como
// variable numerica, `workers` distinto de 1) se salta con el motivo: lo cubren los runners de Rust y
// Python, que siguen siendo los que cubren la spec entera.
//
// `api` es el modulo del binding: `Column`, `DTYPE` y las funciones `count`, `sum`, ..., `kmeans`.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const CASES = fileURLToPath(new URL('../../spec/cases', import.meta.url));

// Los enteros fuera de rango seguro (extremos de int64) se leen como BigInt.
function parse(text) {
  return JSON.parse(text, (key, value, context) =>
    typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value) ? BigInt(context.source) : value,
  );
}

function files(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    return entry.isDirectory() ? files(full) : entry.name.endsWith('.json') ? [full] : [];
  });
}

const SPECIAL = { NaN: NaN, Infinity: Infinity, '-Infinity': -Infinity, '-0.0': -0 };
const floatOf = (v) => (typeof v === 'string' ? SPECIAL[v] : Number(v));

function build(api, spec) {
  const { Column } = api;
  const nulls = spec.values.map((v) => v === null);
  const validity = nulls.some(Boolean) ? Uint8Array.from(nulls, (n) => (n ? 0 : 1)) : undefined;
  const raw = spec.values.map((v) => (v === null ? 0 : v));
  switch (spec.dtype) {
    case 'float64': return Column.fromFloat64(Float64Array.from(raw, floatOf), validity);
    case 'int32': return Column.fromInt32(Int32Array.from(raw), validity);
    case 'int64': return Column.fromBigInt64(BigInt64Array.from(raw, BigInt), validity);
    case 'bool': return Column.fromBool(Uint8Array.from(raw, Number), validity);
    case 'utf8': return Column.fromStrings(spec.values);
    default: throw new Error(`dtype ${spec.dtype}`);
  }
}

function read(api, column) {
  const { DTYPE } = api;
  const valid = column.validity();
  const values = (() => {
    switch (column.dtype) {
      case DTYPE.FLOAT64: return [...column.toFloat64Array()];
      case DTYPE.INT32: return [...column.toInt32Array()];
      case DTYPE.INT64: return [...column.toBigInt64Array()];
      case DTYPE.BOOL: return [...column.toBoolArray()].map(Boolean);
      default: return column.toStrings();
    }
  })();
  return values.map((v, i) => (column.dtype === DTYPE.UTF8 ? v : valid[i] ? v : null));
}

function same(got, want, dtype, tolerance) {
  if (want === null || got === null) return want === null && got === null;
  if (dtype === 'float64' || typeof got === 'number') {
    const w = floatOf(want);
    if (Number.isNaN(w) || Number.isNaN(got)) return Number.isNaN(w) && Number.isNaN(got);
    if (tolerance !== undefined && Number.isFinite(w) && Number.isFinite(got)) return Math.abs(got - w) <= tolerance;
    return Object.is(got, w);
  }
  if (typeof got === 'bigint') return got === BigInt(want);
  return got === want;
}

function expectColumn(got, want, tolerance) {
  assert.equal(got.length, want.values.length, 'largo');
  got.forEach((value, i) => {
    assert.ok(same(value, want.values[i], want.dtype, tolerance), `values[${i}]: salio ${value}, esperado ${want.values[i]}`);
  });
}

const rowsOf = (columns) =>
  Array.from({ length: columns[0].values.length }, (_, i) => columns.map((c) => Number(floatOf(c.values[i]))));

const SUPPORTED = new Set([
  'roundtrip', 'array_roundtrip', 'count', 'sum', 'min', 'max', 'mean', 'var', 'std', 'median', 'percentile',
  'corr', 'edit_distance', 'matmul', 'lu_solve', 'det', 'inverse', 'kmeans',
]);

// Devuelve el motivo si el caso no se puede expresar en JavaScript.
function unsupported(c) {
  if (!SUPPORTED.has(c.op)) return 'operacion fuera de la superficie minima';
  if (c.args && c.args.workers !== undefined && c.args.workers !== 1) return 'workers: el binding siempre usa 1';
  const lists = [c.inputs, c.inputs2].filter(Boolean);
  if (lists.some((l) => l.length === 0)) return 'lista de columnas vacia';
  if (lists.flat().some((col) => col.values.includes(null) || col.dtype === 'utf8')) return 'nulos o texto en una matriz';
  return null;
}

function run(api, c) {
  const { expect } = c;
  const tolerance = expect.tolerance;
  const owned = [];
  const make = (spec) => (owned.push(build(api, spec)), owned.at(-1));
  try {
    let columns = null;
    let scalar;
    switch (c.op) {
      case 'roundtrip':
      case 'array_roundtrip':
        columns = [read(api, make(c.input))];
        break;
      case 'count': scalar = api.count(make(c.input)); break;
      case 'sum': scalar = api.sum(make(c.input)); break;
      case 'min': scalar = api.min(make(c.input)); break;
      case 'max': scalar = api.max(make(c.input)); break;
      case 'mean': scalar = api.mean(make(c.input)); break;
      case 'var': scalar = api.variance(make(c.input), c.args.ddof); break;
      case 'std': scalar = api.std(make(c.input), c.args.ddof); break;
      case 'median': scalar = api.median(make(c.input)); break;
      case 'percentile': scalar = api.percentile(make(c.input), floatOf(c.args.p)); break;
      case 'corr': scalar = api.corr(make(c.input), make(c.input2)); break;
      case 'edit_distance': {
        const result = api.editDistance(make(c.input), make(c.input2));
        owned.push(result);
        columns = [read(api, result)];
        break;
      }
      case 'matmul': columns = [api.matmul(rowsOf(c.inputs), rowsOf(c.inputs2)).flat()]; break;
      case 'lu_solve': columns = [api.solve(rowsOf(c.inputs), c.input.values.map(Number))]; break;
      case 'det': scalar = api.det(rowsOf(c.inputs)); break;
      case 'inverse': columns = [api.inverse(rowsOf(c.inputs)).flat()]; break;
      case 'kmeans': {
        const variables = c.inputs.map((col) => col.values.map(Number));
        const r = api.kmeans(variables, c.args.k, { seed: BigInt(c.args.seed), maxIter: c.args.max_iter, tol: c.args.tol });
        assert.ok(same(r.inertia, expect.inertia, 'float64', tolerance), `inertia ${r.inertia}`);
        assert.equal(r.iterations, expect.iterations);
        columns = [r.labels.map(BigInt), r.centroids.flat()];
        break;
      }
      default: return false;
    }
    if (scalar !== undefined) {
      if (typeof scalar === 'bigint') assert.equal(scalar, BigInt(expect.value));
      else assert.ok(same(scalar, expect.value, 'float64', tolerance), `value: salio ${scalar}, esperado ${expect.value}`);
    } else if (expect.columns) {
      assert.equal(columns.length, expect.columns.length, 'numero de columnas');
      columns.forEach((got, i) => expectColumn(got, expect.columns[i], tolerance));
    } else if (expect.values) {
      expectColumn(columns[0], expect, tolerance);
    }
    return true;
  } finally {
    owned.forEach((col) => col.free());
  }
}

/** Registra un `test` por caso de `spec/` y uno final que exige que se hayan ejecutado `minimum`. */
export function registerSpecTests(api, { minimum }) {
  let executed = 0;
  for (const file of files(CASES)) {
    const c = parse(fs.readFileSync(file, 'utf8'));
    const reason = unsupported(c);
    test(c.id, { skip: reason ?? undefined }, () => {
      if (c.expect.error) {
        assert.throws(() => run(api, c), (e) => e.category === c.expect.error, `esperaba ${c.expect.error}`);
        executed++;
      } else if (run(api, c)) {
        executed++;
      }
    });
  }
  test('el subconjunto soportado es grande', () => {
    assert.ok(executed >= minimum, `solo se ejecutaron ${executed} casos`);
  });
}
