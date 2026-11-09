package io.geoda;

/** Resultado de {@link Geoda#lstsq}: los coeficientes y la norma euclidiana del residuo. */
public record LstsqResult(double[] coefficients, double residualNorm) {}
