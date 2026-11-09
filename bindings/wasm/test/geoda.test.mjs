import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { Geoda, GeodaError } from '../geoda.mjs';

const wasmPath = fileURLToPath(new URL('../geoda.wasm', import.meta.url));
const geoda = await Geoda.load(fs.readFileSync(wasmPath));
const { Column, DTYPE } = geoda;
const near = (a, b, tol = 1e-12) => Math.abs(a - b) <= tol;

test('el modulo exporta exactamente la ABI congelada y no importa nada', () => {
  const module = new WebAssembly.Module(fs.readFileSync(wasmPath));
  assert.deepEqual(WebAssembly.Module.imports(module), [], 'sin imports: ni WASI ni env');
  const exported = WebAssembly.Module.exports(module)
    .map((e) => e.name)
    .filter((name) => name.startsWith('geoda_'))
    .sort();
  const frozen = fs
    .readFileSync(fileURLToPath(new URL('../../../abi/symbols.txt', import.meta.url)), 'utf8')
    .split('\n')
    .filter(Boolean)
    .sort();
  assert.deepEqual(exported, frozen);
  assert.ok(WebAssembly.Module.exports(module).some((e) => e.name === 'memory' && e.kind === 'memory'));
});

test('la version de la ABI es 1.0', () => {
  assert.equal(geoda.abiVersion(), 1 << 16);
});

test('columnas: ida y vuelta con nulos', () => {
  const f = Column.fromFloat64(Float64Array.of(1.5, 99, 3.5), Uint8Array.of(1, 0, 1));
  assert.deepEqual([f.length, f.nullCount, f.dtype], [3, 1, DTYPE.FLOAT64]);
  assert.deepEqual([...f.validity()], [1, 0, 1]);
  assert.deepEqual([...f.toFloat64Array()], [1.5, 0, 3.5]);
  f.free();
  const i = Column.fromInt32(Int32Array.of(-2147483648, 0, 2147483647));
  assert.deepEqual([...i.toInt32Array()], [-2147483648, 0, 2147483647]);
  i.free();
  const l = Column.fromBigInt64(BigInt64Array.of(-(2n ** 63n), 0n, 2n ** 63n - 1n));
  assert.deepEqual([...l.toBigInt64Array()], [-(2n ** 63n), 0n, 2n ** 63n - 1n]);
  l.free();
  const b = Column.fromBool(Uint8Array.of(1, 0, 1), Uint8Array.of(1, 1, 0));
  assert.deepEqual([...b.toBoolArray()], [1, 0, 0]);
  b.free();
});

test('texto: vacias, nulos y no ASCII', () => {
  const texts = ['', 'ñandú', null, '日本語', 'hola'];
  const column = Column.fromStrings(texts);
  assert.deepEqual(column.toStrings(), texts);
  assert.equal(column.nullCount, 1);
  column.free();
});

test('free es idempotente; la memoria no crece sin fin al crear y liberar', () => {
  const column = Column.fromFloat64(Float64Array.of(1, 2));
  column.free();
  column.free();
  assert.throws(() => column.length, /ya se libero/);
  const before = geoda.exports.memory.buffer.byteLength;
  for (let n = 0; n < 5000; n++) Column.fromFloat64(Float64Array.of(n, n + 1, n + 2)).free();
  assert.ok(geoda.exports.memory.buffer.byteLength <= before + 65536, 'la memoria crecio: fuga');
});

test('errores de la capi traen code y category', () => {
  const empty = Column.fromFloat64(new Float64Array(0));
  assert.throws(() => geoda.median(empty), (e) => e instanceof GeodaError && e.category === 'INVALID_ARGUMENT' && e.code === -1);
  const floats = Column.fromFloat64(Float64Array.of(1));
  assert.throws(() => floats.toBigInt64Array(), (e) => e.category === 'TYPE_MISMATCH');
  assert.throws(() => Column.fromFloat64(Float64Array.of(1, 2), Uint8Array.of(1)), (e) => e.category === 'LENGTH_MISMATCH');
  assert.throws(() => geoda.solve([[1, 2], [2, 4]], [1, 2]), (e) => e.category === 'INVALID_ARGUMENT');
  assert.throws(() => geoda.kmeans([[0, 1, 2]], 0), (e) => e.category === 'INVALID_ARGUMENT');
  assert.throws(() => Column.fromFloat64([1, 2]), TypeError);
  empty.free();
  floats.free();
});

test('workers mayor que 1: UNSUPPORTED y no un panic (WebAssembly no tiene hilos)', () => {
  const column = Column.fromBigInt64(BigInt64Array.of(1n, 2n, 3n));
  const e = geoda.exports;
  const cell = e.geoda_alloc(8);
  assert.equal(e.geoda_column_sum_i64(column.pointer, 4, cell), -5); // GEODA_ERR_UNSUPPORTED
  assert.equal(e.geoda_column_sum_i64(column.pointer, 1, cell), 0);
  assert.equal(new DataView(e.memory.buffer).getBigInt64(cell, true), 6n);
  e.geoda_dealloc(cell, 8);
  column.free();
});

test('estadisticos escalares', () => {
  const ints = Column.fromBigInt64(BigInt64Array.of(1n, 2n, 3n, 4n));
  const y = Column.fromFloat64(Float64Array.of(2, 4, 6, 8));
  assert.equal(geoda.count(ints), 4);
  assert.equal(geoda.sum(ints), 10n);
  assert.equal(geoda.min(ints), 1n);
  assert.equal(geoda.max(ints), 4n);
  assert.equal(geoda.median(ints), 2.5);
  assert.equal(geoda.mean(ints), 2.5);
  assert.ok(near(geoda.variance(ints), 5 / 3) && near(geoda.variance(ints, 0), 1.25));
  assert.ok(near(geoda.std(ints, 0), Math.sqrt(1.25)));
  assert.ok(near(geoda.percentile(ints, 25), 1.75));
  assert.ok(near(geoda.corr(ints, y), 1));
  [ints, y].forEach((c) => c.free());
});

test('texto, algebra lineal y k-means', () => {
  assert.equal(geoda.editDistance('kitten', 'sitting'), 3);
  assert.equal(geoda.editDistance('añb', 'anb'), 1);
  assert.deepEqual(geoda.matmul([[1, 2, 3], [4, 5, 6]], [[7, 8], [9, 10], [11, 12]]), [[58, 64], [139, 154]]);
  const x = geoda.solve([[2, 1], [1, 3]], [3, 5]);
  assert.ok(near(x[0], 0.8) && near(x[1], 1.4));
  assert.ok(near(geoda.det([[4, 3], [6, 3]]), -6));
  assert.equal(geoda.det([[1, 2], [2, 4]]), 0);
  const inv = geoda.inverse([[4, 7], [2, 6]]);
  assert.ok(near(inv[0][0], 0.6) && near(inv[0][1], -0.7) && near(inv[1][0], -0.2) && near(inv[1][1], 0.4));
  const r = geoda.kmeans([[0, 0, 10, 10], [0, 1, 10, 11]], 2, { seed: 1n });
  assert.deepEqual(r.labels, [0, 0, 1, 1]);
  assert.deepEqual(r.centroids, [[0, 0.5], [10, 10.5]]);
  assert.equal(r.inertia, 1);
});

test('datos grandes: la memoria crece y las vistas se renuevan', () => {
  const n = 2_000_000;
  const values = new Float64Array(n);
  for (let i = 0; i < n; i++) values[i] = i % 1000;
  const column = Column.fromFloat64(values);
  assert.equal(column.length, n);
  assert.ok(near(geoda.mean(column), 499.5, 1e-9));
  const back = column.toFloat64Array();
  assert.equal(back.length, n);
  assert.equal(back[1234], 234);
  column.free();
});

test('nivel 2: qr, svd, lstsq', () => {
  const { q, r } = geoda.qr([[3, 0], [4, 5]]);
  assert.ok(q.flat().every((v, i) => near(v, [-0.6, 0.8, -0.8, -0.6][i])));
  assert.ok(r.flat().every((v, i) => near(v, [-5, -4, 0, -3][i])));
  const { u, s, vt } = geoda.svd([[1, 0], [0, 2], [0, 0]]);
  assert.deepEqual(s, [2, 1]);
  assert.deepEqual(u, [[0, 1], [1, 0], [0, 0]]);
  assert.deepEqual(vt, [[0, 1], [1, 0]]);
  assert.equal(geoda.svd([[1, 0, 0], [0, 3, 0]]).s.length, 2);
  const fit = geoda.lstsq([[1, 0], [1, 1], [1, 2]], [1, 3, 5]);
  assert.ok(near(fit.coefficients[0], 1) && near(fit.coefficients[1], 2) && near(fit.residualNorm, 0));
  assert.throws(() => geoda.qr([[1, 2, 3]]), (e) => e.category === 'INVALID_ARGUMENT');
  assert.throws(() => geoda.lstsq([[1, 2], [2, 4], [3, 6]], [1, 2, 3]), (e) => e.category === 'INVALID_ARGUMENT');
});

test('nivel 2: lis, dedupe, histogram', () => {
  const series = Column.fromBigInt64(BigInt64Array.of(10n, 9n, 2n, 5n, 3n, 7n, 101n, 18n));
  assert.deepEqual(geoda.lis(series), [2, 4, 5, 7]);
  assert.throws(() => geoda.lis(Column.fromFloat64(Float64Array.of(1, NaN, 3))), (e) => e.category === 'INVALID_ARGUMENT');
  const names = Column.fromStrings(['Juan Perez', 'Juan Pérez', 'Ana', null, 'Ana']);
  assert.deepEqual(geoda.dedupe(names, 1), { ids: [0, 0, 1, null, 1], count: 2 });
  assert.throws(() => geoda.dedupe(series, 1), (e) => e.category === 'TYPE_MISMATCH');
  const values = Column.fromBigInt64(BigInt64Array.of(1n, 2n, 3n, 4n, 5n, 6n));
  assert.deepEqual(geoda.histogram(values, 2, [2, 4]), { edges: [2, 3, 4], counts: [1, 2], below: 1, above: 2, nulls: 0, nans: 0 });
  assert.throws(() => geoda.histogram(values, 0), (e) => e.category === 'INVALID_ARGUMENT');
  [series, names, values].forEach((c) => c.free());
});
