package io.geoda;

import java.util.Arrays;

/**
 * Pruebas del binding de Java (HU-063), sin dependencias: {@code main} falla con codigo 1 si algo no
 * cumple. Los valores esperados salen de {@code spec/cases/}.
 */
public final class GeodaTest {
    private static int failures = 0;

    private static void check(boolean condition, String what) {
        if (!condition) {
            System.err.println("fallo: " + what);
            failures++;
        }
    }

    private static boolean near(double a, double b, double tol) {
        return Math.abs(a - b) <= tol;
    }

    private interface Body {
        void run() throws Throwable;
    }

    private static void throwsCategory(String category, Body body, String what) {
        try {
            body.run();
            check(false, what + ": no lanzo");
        } catch (GeodaException e) {
            check(e.category().equals(category) && e.code() < 0 && e.getMessage().startsWith(category),
                    what + ": categoria " + e.category());
        } catch (Throwable t) {
            check(false, what + ": lanzo " + t);
        }
    }

    public static void main(String[] args) {
        check(Geoda.abiVersion() == (1 << 16), "version de la ABI");
        columnsRoundTrip();
        lifecycle();
        errors();
        scalars();
        text();
        linearAlgebra();
        clustering();
        levelTwo();
        if (failures == 0) {
            System.out.println("GeodaTest: ok");
        } else {
            System.err.println("GeodaTest: " + failures + " fallos");
            System.exit(1);
        }
    }

    static void columnsRoundTrip() {
        try (Column f = Column.of(new double[] {1.5, 99.0, 3.5}, new boolean[] {true, false, true})) {
            check(f.size() == 3 && f.nullCount() == 1 && f.dtype() == 3, "float64: tamano y nulos");
            check(Arrays.equals(f.validity(), new boolean[] {true, false, true}), "float64: validez");
            double[] v = f.toDoubleArray();
            check(v[0] == 1.5 && v[1] == 0.0 && v[2] == 3.5, "float64: valores (nulo = 0)");
        }
        try (Column i = Column.of(new int[] {Integer.MIN_VALUE, 0, Integer.MAX_VALUE})) {
            check(Arrays.equals(i.toIntArray(), new int[] {Integer.MIN_VALUE, 0, Integer.MAX_VALUE}), "int32");
        }
        try (Column l = Column.of(new long[] {Long.MIN_VALUE, 0, Long.MAX_VALUE})) {
            check(Arrays.equals(l.toLongArray(), new long[] {Long.MIN_VALUE, 0, Long.MAX_VALUE}), "int64");
        }
        try (Column b = Column.ofBooleans(new boolean[] {true, false, true}, new boolean[] {true, true, false})) {
            check(Arrays.equals(b.toBooleanArray(), new boolean[] {true, false, false}) && b.nullCount() == 1, "bool");
        }
        String[] texts = {"", "ñandú", null, "日本語", "hola"};
        try (Column t = Column.ofStrings(texts)) {
            check(Arrays.equals(t.toStringArray(), texts) && t.nullCount() == 1, "utf8 con vacias, nulos y no ASCII");
        }
        try (Column empty = Column.of(new double[0])) {
            check(empty.size() == 0 && empty.toDoubleArray().length == 0, "columna vacia");
        }
    }

    static void lifecycle() {
        Column column = Column.of(new double[] {1.0, 2.0});
        column.close();
        column.close(); // idempotente
        try {
            column.size();
            check(false, "usar una columna cerrada debia lanzar");
        } catch (IllegalStateException expected) {
            check(true, "");
        }
        // Muchas columnas en un bucle: cada una se libera (no hay fuga que Arena o ASan vean aqui, pero
        // si hubiera un doble free la capi abortaria).
        for (int n = 0; n < 2000; n++) {
            try (Column c = Column.of(new long[] {n, n + 1})) {
                check(c.size() == 2, "bucle");
            }
        }
    }

    static void errors() {
        throwsCategory("INVALID_ARGUMENT", () -> {
            try (Column c = Column.of(new double[0])) {
                Geoda.median(c);
            }
        }, "mediana de una columna vacia");
        throwsCategory("TYPE_MISMATCH", () -> {
            try (Column c = Column.of(new double[] {1.0})) {
                c.toLongArray();
            }
        }, "leer Float64 como Int64");
        throwsCategory("INVALID_ARGUMENT", () -> Geoda.solve(new double[][] {{1, 2}, {2, 4}}, new double[] {1, 2}),
                "matriz singular");
        throwsCategory("LENGTH_MISMATCH", () -> Geoda.matmul(new double[][] {{1, 2, 3}, {4, 5, 6}},
                new double[][] {{1, 2, 3}, {4, 5, 6}}), "producto que no encaja");
        throwsCategory("LENGTH_MISMATCH", () -> Geoda.det(new double[][] {{1, 2}, {3}}), "filas de distinto largo");
        throwsCategory("INVALID_ARGUMENT", () -> Geoda.kmeans(new double[][] {{0, 1, 2}}, 0, 0), "k = 0");
        try {
            Column.of(new double[] {1.0, 2.0}, new boolean[] {true});
            check(false, "validez de otro largo debia lanzar");
        } catch (IllegalArgumentException expected) {
            check(true, "");
        }
    }

    static void scalars() {
        try (Column ints = Column.of(new long[] {1, 2, 3, 4});
                Column y = Column.of(new double[] {2.0, 4.0, 6.0, 8.0})) {
            check(Geoda.count(ints) == 4, "count");
            check(Geoda.sum(ints) instanceof Long s && s == 10L, "sum entera");
            check(Geoda.min(ints).equals(1L) && Geoda.max(ints).equals(4L), "min y max enteros");
            check(Geoda.median(ints) == 2.5, "mediana (spec)");
            check(Geoda.mean(ints) == 2.5, "media");
            check(near(Geoda.var(ints, 1), 5.0 / 3.0, 1e-12) && near(Geoda.var(ints, 0), 1.25, 1e-12), "varianza");
            check(near(Geoda.std(ints, 0), Math.sqrt(1.25), 1e-12), "desviacion");
            check(near(Geoda.percentile(ints, 25.0), 1.75, 1e-12), "percentil");
            check(near(Geoda.corr(ints, y), 1.0, 1e-12), "correlacion");
        }
        try (Column floats = Column.of(new double[] {1.5, 0.0, 2.5}, new boolean[] {true, false, true})) {
            check(Geoda.sum(floats) instanceof Double d && d == 4.0 && Geoda.count(floats) == 2, "suma con nulos");
        }
    }

    static void text() {
        check(Geoda.editDistance("kitten", "sitting") == 3, "kitten/sitting");
        check(Geoda.editDistance("añb", "anb") == 1, "por caracteres, no bytes");
        try (Column a = Column.ofStrings(new String[] {"casa", "cosa", null});
                Column b = Column.ofStrings(new String[] {"casa"});
                Column result = Geoda.editDistance(a, b)) {
            check(Arrays.equals(result.toLongArray(), new long[] {0, 1, 0}) && result.nullCount() == 1, "una cadena contra muchas");
        }
    }

    static void linearAlgebra() {
        double[][] product = Geoda.matmul(new double[][] {{1, 2, 3}, {4, 5, 6}}, new double[][] {{7, 8}, {9, 10}, {11, 12}});
        check(Arrays.deepEquals(product, new double[][] {{58, 64}, {139, 154}}), "producto exacto (spec)");
        double[] x = Geoda.solve(new double[][] {{2, 1}, {1, 3}}, new double[] {3, 5});
        check(near(x[0], 0.8, 1e-12) && near(x[1], 1.4, 1e-12), "solve 2x2");
        check(Arrays.equals(Geoda.solve(new double[][] {{0, 1}, {1, 0}}, new double[] {2, 3}), new double[] {3, 2}), "pivoteo");
        check(near(Geoda.det(new double[][] {{4, 3}, {6, 3}}), -6.0, 1e-12), "determinante");
        check(Geoda.det(new double[][] {{1, 2}, {2, 4}}) == 0.0, "determinante singular = 0");
        double[][] inv = Geoda.inverse(new double[][] {{4, 7}, {2, 6}});
        check(near(inv[0][0], 0.6, 1e-12) && near(inv[0][1], -0.7, 1e-12) && near(inv[1][0], -0.2, 1e-12)
                && near(inv[1][1], 0.4, 1e-12), "inversa");
    }

    static void levelTwo() {
        QrResult f = Geoda.qr(new double[][] {{3, 0}, {4, 5}}); // spec: linalg/qr_3_4_5
        double[] q = {-0.6, 0.8, -0.8, -0.6};
        double[] r = {-5, -4, 0, -3};
        for (int i = 0; i < 4; i++) {
            check(near(f.q()[i / 2][i % 2], q[i], 1e-12) && near(f.r()[i / 2][i % 2], r[i], 1e-12), "qr entrada " + i);
        }
        SvdResult d = Geoda.svd(new double[][] {{1, 0}, {0, 2}, {0, 0}}); // spec: linalg/svd_permutation_exact
        check(Arrays.equals(d.s(), new double[] {2, 1}), "svd: valores singulares");
        check(Arrays.deepEquals(d.u(), new double[][] {{0, 1}, {1, 0}, {0, 0}}), "svd: u");
        check(Arrays.deepEquals(d.vt(), new double[][] {{0, 1}, {1, 0}}), "svd: vt");
        check(Geoda.svd(new double[][] {{1, 0, 0}, {0, 3, 0}}).s().length == 2, "svd de una matriz ancha");
        LstsqResult fit = Geoda.lstsq(new double[][] {{1, 0}, {1, 1}, {1, 2}}, new double[] {1, 3, 5});
        check(near(fit.coefficients()[0], 1, 1e-12) && near(fit.coefficients()[1], 2, 1e-12) && near(fit.residualNorm(), 0, 1e-12), "lstsq");
        throwsCategory("INVALID_ARGUMENT", () -> Geoda.qr(new double[][] {{1, 2, 3}}), "qr de una matriz ancha");
        throwsCategory("INVALID_ARGUMENT", () -> Geoda.lstsq(new double[][] {{1, 2}, {2, 4}, {3, 6}}, new double[] {1, 2, 3}),
                "lstsq con columnas dependientes");

        try (Column series = Column.of(new long[] {10, 9, 2, 5, 3, 7, 101, 18})) {
            check(Arrays.equals(Geoda.lis(series), new long[] {2, 4, 5, 7}), "lis (spec text/lis_int64)");
            throwsCategory("TYPE_MISMATCH", () -> Geoda.dedupe(series, 1), "dedupe de una columna entera");
        }
        try (Column nan = Column.of(new double[] {1.0, Double.NaN, 3.0})) {
            throwsCategory("INVALID_ARGUMENT", () -> Geoda.lis(nan), "lis con NaN");
        }
        try (Column names = Column.ofStrings(new String[] {"Juan Perez", "Juan P\u00e9rez", "Ana", null, "Ana"})) {
            DedupeResult groups = Geoda.dedupe(names, 1);
            check(groups.count() == 2 && Arrays.equals(groups.ids(), new Long[] {0L, 0L, 1L, null, 1L}), "dedupe (spec)");
        }
        try (Column values = Column.of(new long[] {1, 2, 3, 4, 5, 6})) {
            HistogramResult h = Geoda.histogram(values, 2, new double[] {2, 4});
            check(Arrays.equals(h.edges(), new double[] {2, 3, 4}) && Arrays.equals(h.counts(), new long[] {1, 2}), "histograma: bordes y conteos");
            check(h.below() == 1 && h.above() == 2 && h.nulls() == 0 && h.nans() == 0, "histograma: fuera de rango");
            throwsCategory("INVALID_ARGUMENT", () -> Geoda.histogram(values, 0, null), "histograma de 0 cubos");
        }
    }

    static void clustering() {
        KMeansResult r = Geoda.kmeans(new double[][] {{0, 0, 10, 10}, {0, 1, 10, 11}}, 2, 1);
        check(Arrays.equals(r.labels(), new long[] {0, 0, 1, 1}), "etiquetas (spec cluster/kmeans_two_clouds)");
        check(Arrays.deepEquals(r.centroids(), new double[][] {{0.0, 0.5}, {10.0, 10.5}}), "centroides");
        check(r.inertia() == 1.0 && r.iterations() >= 1, "inercia");
        check(Arrays.equals(Geoda.kmeans(new double[][] {{0, 0, 10, 10}, {0, 1, 10, 11}}, 2, 1).labels(), r.labels()),
                "determinista");
    }
}
