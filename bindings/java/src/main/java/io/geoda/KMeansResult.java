package io.geoda;

/**
 * Resultado de {@link Geoda#kmeans}.
 *
 * @param labels el grupo de cada fila
 * @param centroids {@code k} filas con un valor por variable
 * @param inertia suma de distancias al cuadrado de cada fila a su centroide
 * @param iterations iteraciones hechas
 */
public record KMeansResult(long[] labels, double[][] centroids, double inertia, long iterations) {}
