package io.geoda;

/** Resultado de {@link Geoda#histogram}: {@code bins + 1} bordes, {@code bins} conteos y lo que cae fuera. */
public record HistogramResult(double[] edges, long[] counts, long below, long above, long nulls, long nans) {}
