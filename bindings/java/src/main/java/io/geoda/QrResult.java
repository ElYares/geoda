package io.geoda;

/** Resultado de {@link Geoda#qr}: {@code q} (m x n, columnas ortonormales) y {@code r} (n x n, triangular superior). */
public record QrResult(double[][] q, double[][] r) {}
