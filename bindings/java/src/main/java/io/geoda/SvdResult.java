package io.geoda;

/** Resultado de {@link Geoda#svd}: {@code u} (m x k), {@code s} (k valores, de mayor a menor) y {@code vt} (k x n). */
public record SvdResult(double[][] u, double[] s, double[][] vt) {}
