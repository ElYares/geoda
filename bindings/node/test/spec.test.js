'use strict';
// Corre el subconjunto de `spec/cases/` que el binding soporta (decision 015, O5). Lo que no puede
// expresar JavaScript (nulos dentro de una matriz de `Float64Array`, listas de columnas vacias,
// texto como variable numerica) se salta con el motivo: lo cubren los runners de Rust y Python.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const geoda = require('..');
const { Column, DTYPE } = geoda;

const CASES = path.resolve(__dirname, '../../../spec/cases');

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

const floatOf = (v) => (typeof v === 'string' ? { NaN: NaN, Infinity: Infinity, '-Infinity': -Infinity, '-0.0': -0 }[v] : Number(v));

function build(spec) {
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

function read(column) {
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

const rowsOf = (columns) => Array.from({ length: columns[0].values.length }, (_, i) => columns.map((c) => Number(floatOf(c.values[i]))));

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

function run(c) {
  const expect = c.expect;
  const tolerance = expect.tolerance;
  const owned = [];
  const make = (spec) => (owned.push(build(spec)), owned.at(-1));
  try {
    let columns = null;
    let scalar;
    switch (c.op) {
      case 'roundtrip':
      case 'array_roundtrip':
        columns = [read(make(c.input))];
        break;
      case 'count': scalar = geoda.count(make(c.input)); break;
      case 'sum': scalar = geoda.sum(make(c.input)); break;
      case 'min': scalar = geoda.min(make(c.input)); break;
      case 'max': scalar = geoda.max(make(c.input)); break;
      case 'mean': scalar = geoda.mean(make(c.input)); break;
      case 'var': scalar = geoda.variance(make(c.input), c.args.ddof); break;
      case 'std': scalar = geoda.std(make(c.input), c.args.ddof); break;
      case 'median': scalar = geoda.median(make(c.input)); break;
      case 'percentile': scalar = geoda.percentile(make(c.input), floatOf(c.args.p)); break;
      case 'corr': scalar = geoda.corr(make(c.input), make(c.input2)); break;
      case 'edit_distance': {
        const result = geoda.editDistance(make(c.input), make(c.input2));
        owned.push(result);
        columns = [read(result)];
        break;
      }
      case 'matmul': columns = [geoda.matmul(rowsOf(c.inputs), rowsOf(c.inputs2)).flat()]; break;
      case 'lu_solve': columns = [geoda.solve(rowsOf(c.inputs), c.input.values.map(Number))]; break;
      case 'det': scalar = geoda.det(rowsOf(c.inputs)); break;
      case 'inverse': columns = [geoda.inverse(rowsOf(c.inputs)).flat()]; break;
      case 'kmeans': {
        const variables = c.inputs.map((col) => col.values.map(Number));
        const r = geoda.kmeans(variables, c.args.k, { seed: BigInt(c.args.seed), maxIter: c.args.max_iter, tol: c.args.tol });
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

let executed = 0;
for (const file of files(CASES)) {
  const c = parse(fs.readFileSync(file, 'utf8'));
  const reason = unsupported(c);
  test(c.id, { skip: reason ?? undefined }, () => {
    if (c.expect.error) {
      assert.throws(() => run(c), (e) => e.category === c.expect.error, `esperaba ${c.expect.error}`);
      executed++;
    } else if (run(c)) {
      executed++;
    }
  });
}

test('el subconjunto soportado es grande', () => {
  assert.ok(executed >= 60, `solo se ejecutaron ${executed} casos`);
});
