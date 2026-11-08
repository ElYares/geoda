/** geoda compilado a WebAssembly (ABI 1.x). */
export declare class GeodaError extends Error {
  code: number;
  category: string;
}

export declare class Column {
  static fromFloat64(values: Float64Array, validity?: Uint8Array): Column;
  static fromInt32(values: Int32Array, validity?: Uint8Array): Column;
  static fromBigInt64(values: BigInt64Array, validity?: Uint8Array): Column;
  static fromBool(values: Uint8Array, validity?: Uint8Array): Column;
  static fromStrings(strings: (string | null)[]): Column;
  readonly length: number;
  readonly nullCount: number;
  readonly dtype: number;
  validity(): Uint8Array;
  toFloat64Array(): Float64Array;
  toInt32Array(): Int32Array;
  toBigInt64Array(): BigInt64Array;
  toBoolArray(): Uint8Array;
  toStrings(): (string | null)[];
  free(): void;
}

export type Matrix = ArrayLike<number>[];

export interface KMeansResult {
  labels: number[];
  centroids: number[][];
  inertia: number;
  iterations: number;
}

export declare class Geoda {
  /** `source`: los bytes del `.wasm`, o una `Response` (o promesa) para instanciar en streaming. */
  static load(source: BufferSource | Response | Promise<BufferSource | Response>): Promise<Geoda>;
  readonly Column: typeof Column;
  readonly DTYPE: Readonly<{ BOOL: 0; INT32: 1; INT64: 2; FLOAT64: 3; UTF8: 4 }>;
  readonly exports: WebAssembly.Exports;
  abiVersion(): number;
  count(column: Column): number;
  sum(column: Column): number | bigint;
  min(column: Column): number | bigint;
  max(column: Column): number | bigint;
  mean(column: Column): number;
  variance(column: Column, ddof?: number): number;
  std(column: Column, ddof?: number): number;
  median(column: Column): number;
  percentile(column: Column, p: number): number;
  corr(x: Column, y: Column): number;
  editDistance(a: string, b: string): number;
  editDistance(a: Column, b: Column): Column;
  matmul(a: Matrix, b: Matrix): number[][];
  solve(a: Matrix, b: ArrayLike<number>): number[];
  det(a: Matrix): number;
  inverse(a: Matrix): number[][];
  kmeans(variables: ArrayLike<number>[], k: number, options?: { seed?: bigint | number; maxIter?: number; tol?: number }): KMeansResult;
}
