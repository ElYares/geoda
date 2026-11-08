/** Binding de geoda para Node (ABI 1.x). */
export declare const DTYPE: Readonly<{ BOOL: 0; INT32: 1; INT64: 2; FLOAT64: 3; UTF8: 4 }>;

/** Error de la C-API: `code` negativo y `category` (`INVALID_ARGUMENT`, `TYPE_MISMATCH`...). */
export interface GeodaError extends Error {
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

export declare function abiVersion(): number;
export declare function count(column: Column): number;
export declare function sum(column: Column): number | bigint;
export declare function min(column: Column): number | bigint;
export declare function max(column: Column): number | bigint;
export declare function mean(column: Column): number;
export declare function variance(column: Column, ddof?: number): number;
export declare function std(column: Column, ddof?: number): number;
export declare function median(column: Column): number;
export declare function percentile(column: Column, p: number): number;
export declare function corr(x: Column, y: Column): number;
export declare function editDistance(a: string, b: string): number;
export declare function editDistance(a: Column, b: Column): Column;

export type Matrix = ArrayLike<number>[];
export declare function matmul(a: Matrix, b: Matrix): number[][];
export declare function solve(a: Matrix, b: ArrayLike<number>): number[];
export declare function det(a: Matrix): number;
export declare function inverse(a: Matrix): number[][];

export interface KMeansResult {
  labels: number[];
  centroids: number[][];
  inertia: number;
  iterations: number;
}
export declare function kmeans(
  variables: ArrayLike<number>[],
  k: number,
  options?: { seed?: bigint | number; maxIter?: number; tol?: number },
): KMeansResult;
