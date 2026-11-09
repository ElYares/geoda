package io.geoda;

/** Resultado de {@link Geoda#dedupe}: el grupo de cada fila ({@code null} en los nulos) y cuantos grupos hay. */
public record DedupeResult(Long[] ids, long count) {}
