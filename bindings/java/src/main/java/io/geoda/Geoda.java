package io.geoda;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;

/**
 * Funciones de geoda (superficie minima, decision 015 O4). Una <b>matriz matematica</b> es un
 * {@code double[][]} por <b>filas</b>; una tabla de datos ({@link #kmeans}) es un arreglo de
 * <b>columnas</b>, una por variable. {@code workers} siempre es 1.
 */
public final class Geoda {
    private Geoda() {}

    /** Version de la ABI como {@code major << 16 | minor}. */
    public static int abiVersion() {
        try {
            return (int) Natives.ABI_VERSION.invoke();
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    // --- Estadisticos escalares.

    public static long count(Column column) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.LONG);
            Natives.check((int) Natives.COUNT.invoke(column.handle(), out));
            return out.get(Natives.LONG, 0);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Suma: {@code Long} para columnas enteras, {@code Double} si no. */
    public static Number sum(Column column) {
        return integer(column) ? (Number) longScalar(Natives.SUM_I64, column) : doubleScalar(Natives.SUM_F64, column);
    }

    /** Minimo: {@code Long} para columnas enteras, {@code Double} si no. */
    public static Number min(Column column) {
        return integer(column) ? (Number) longScalar(Natives.MIN_I64, column) : doubleScalar(Natives.MIN_F64, column);
    }

    /** Maximo: {@code Long} para columnas enteras, {@code Double} si no. */
    public static Number max(Column column) {
        return integer(column) ? (Number) longScalar(Natives.MAX_I64, column) : doubleScalar(Natives.MAX_F64, column);
    }

    public static double mean(Column column) {
        return doubleScalar(Natives.MEAN, column);
    }

    /** Varianza con {@code ddof} grados de libertad restados (1 = muestral). */
    public static double var(Column column, long ddof) {
        return withOut(out -> (int) Natives.VAR.invoke(column.handle(), ddof, 1L, out));
    }

    public static double std(Column column, long ddof) {
        return withOut(out -> (int) Natives.STD.invoke(column.handle(), ddof, 1L, out));
    }

    public static double median(Column column) {
        return withOut(out -> (int) Natives.MEDIAN.invoke(column.handle(), out));
    }

    /** Percentil {@code p} en [0, 100] (interpolacion lineal, tipo 7). */
    public static double percentile(Column column, double p) {
        return withOut(out -> (int) Natives.PERCENTILE.invoke(column.handle(), p, out));
    }

    public static double corr(Column x, Column y) {
        return withOut(out -> (int) Natives.CORR.invoke(x.handle(), y.handle(), 1L, out));
    }

    // --- Texto.

    /** Distancia de edicion (por caracteres Unicode) entre dos cadenas. */
    public static long editDistance(String a, String b) {
        try (Column left = Column.ofStrings(new String[] {a});
                Column right = Column.ofStrings(new String[] {b});
                Column result = editDistance(left, right)) {
            return result.toLongArray()[0];
        }
    }

    /** Distancia fila a fila entre dos columnas Utf8 (miden lo mismo, o una mide 1). */
    public static Column editDistance(Column a, Column b) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.EDIT_DISTANCE.invoke(a.handle(), b.handle(), out));
            return Columns.wrap(out.get(Natives.ADDRESS, 0));
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    // --- Algebra lineal (matrices por filas).

    public static double[][] matmul(double[][] a, double[][] b) {
        try (Arena arena = Arena.ofConfined(); Columns left = Columns.of(a); Columns right = Columns.of(b)) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.MATMUL.invoke(
                    left.pointers(arena), (long) left.size(), right.pointers(arena), (long) right.size(), out));
            try (Column flat = Columns.wrap(out.get(Natives.ADDRESS, 0))) {
                return reshape(flat.toDoubleArray(), a.length, b[0].length);
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Resuelve {@code a x = b} por LU con pivoteo parcial; una matriz singular lanza INVALID_ARGUMENT. */
    public static double[] solve(double[][] a, double[] b) {
        try (Arena arena = Arena.ofConfined(); Columns list = Columns.of(a); Column rhs = Column.of(b)) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.LU_SOLVE.invoke(list.pointers(arena), (long) list.size(), rhs.handle(), out));
            try (Column x = Columns.wrap(out.get(Natives.ADDRESS, 0))) {
                return x.toDoubleArray();
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Determinante; una matriz singular da 0.0 (no es un error). */
    public static double det(double[][] a) {
        try (Arena arena = Arena.ofConfined(); Columns list = Columns.of(a)) {
            MemorySegment out = arena.allocate(Natives.DOUBLE);
            Natives.check((int) Natives.DET.invoke(list.pointers(arena), (long) list.size(), out));
            return out.get(Natives.DOUBLE, 0);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    public static double[][] inverse(double[][] a) {
        try (Arena arena = Arena.ofConfined(); Columns list = Columns.of(a)) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.INVERSE.invoke(list.pointers(arena), (long) list.size(), out));
            try (Column flat = Columns.wrap(out.get(Natives.ADDRESS, 0))) {
                return reshape(flat.toDoubleArray(), a.length, a.length);
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    // --- Nivel 2 (decision 016): qr, svd, lstsq, lis, dedupe, histogram.

    /** Factorizacion QR (Householder); hacen falta al menos tantas filas como columnas. */
    public static QrResult qr(double[][] a) {
        try (Arena arena = Arena.ofConfined(); Columns list = Columns.of(a)) {
            MemorySegment q = arena.allocate(Natives.ADDRESS);
            MemorySegment r = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.QR.invoke(list.pointers(arena), (long) list.size(), q, r));
            try (Column qc = Columns.wrap(q.get(Natives.ADDRESS, 0)); Column rc = Columns.wrap(r.get(Natives.ADDRESS, 0))) {
                return new QrResult(reshape(qc.toDoubleArray(), a.length, a[0].length), reshape(rc.toDoubleArray(), a[0].length, a[0].length));
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** SVD delgada {@code a = u diag(s) vt} (valores de mayor a menor, signo determinista). */
    public static SvdResult svd(double[][] a) {
        try (Arena arena = Arena.ofConfined(); Columns list = Columns.of(a)) {
            MemorySegment u = arena.allocate(Natives.ADDRESS);
            MemorySegment s = arena.allocate(Natives.ADDRESS);
            MemorySegment vt = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.SVD.invoke(list.pointers(arena), (long) list.size(), u, s, vt));
            int k = Math.min(a.length, a[0].length);
            try (Column uc = Columns.wrap(u.get(Natives.ADDRESS, 0));
                    Column sc = Columns.wrap(s.get(Natives.ADDRESS, 0));
                    Column vc = Columns.wrap(vt.get(Natives.ADDRESS, 0))) {
                return new SvdResult(reshape(uc.toDoubleArray(), a.length, k), sc.toDoubleArray(), reshape(vc.toDoubleArray(), k, a[0].length));
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Minimos cuadrados {@code min |a x - b|} por QR; columnas dependientes lanzan INVALID_ARGUMENT. */
    public static LstsqResult lstsq(double[][] a, double[] b) {
        try (Arena arena = Arena.ofConfined(); Columns list = Columns.of(a); Column rhs = Column.of(b)) {
            MemorySegment x = arena.allocate(Natives.ADDRESS);
            MemorySegment residual = arena.allocate(Natives.DOUBLE);
            Natives.check((int) Natives.LSTSQ.invoke(list.pointers(arena), (long) list.size(), rhs.handle(), x, residual));
            try (Column xc = Columns.wrap(x.get(Natives.ADDRESS, 0))) {
                return new LstsqResult(xc.toDoubleArray(), residual.get(Natives.DOUBLE, 0));
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Posiciones de la subsecuencia estrictamente creciente mas larga de una columna numerica sin nulos ni NaN. */
    public static long[] lis(Column column) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.LIS.invoke(column.handle(), out));
            try (Column positions = Columns.wrap(out.get(Natives.ADDRESS, 0))) {
                return positions.toLongArray();
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Agrupa las cadenas a distancia de edicion <= {@code maxDistance} (componentes conexas de "estar cerca"). */
    public static DedupeResult dedupe(Column column, long maxDistance) {
        if (maxDistance < 0) {
            throw new GeodaException(-1, "maxDistance debe ser >= 0");
        }
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment ids = arena.allocate(Natives.ADDRESS);
            MemorySegment count = arena.allocate(Natives.LONG);
            Natives.check((int) Natives.DEDUPE.invoke(column.handle(), maxDistance, ids, count));
            try (Column groups = Columns.wrap(ids.get(Natives.ADDRESS, 0))) {
                long[] values = groups.toLongArray();
                boolean[] valid = groups.validity();
                Long[] out = new Long[values.length];
                for (int i = 0; i < out.length; i++) {
                    out[i] = valid[i] ? values[i] : null;
                }
                return new DedupeResult(out, count.get(Natives.LONG, 0));
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Histograma de ancho fijo con {@code bins} cubos; {@code range} es {@code {lo, hi}} o {@code null} para [min, max]. */
    public static HistogramResult histogram(Column column, long bins, double[] range) {
        if (bins < 0) {
            throw new GeodaException(-1, "bins debe ser >= 0");
        }
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment edges = arena.allocate(Natives.ADDRESS);
            MemorySegment counts = arena.allocate(Natives.ADDRESS);
            MemorySegment outside = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.HISTOGRAM.invoke(
                    column.handle(), bins, range != null, range != null ? range[0] : 0.0, range != null ? range[1] : 0.0,
                    1L, edges, counts, outside));
            try (Column e = Columns.wrap(edges.get(Natives.ADDRESS, 0));
                    Column c = Columns.wrap(counts.get(Natives.ADDRESS, 0));
                    Column o = Columns.wrap(outside.get(Natives.ADDRESS, 0))) {
                long[] out = o.toLongArray();
                return new HistogramResult(e.toDoubleArray(), c.toLongArray(), out[0], out[1], out[2], out[3]);
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    // --- K-means.

    /**
     * K-means con k-means++; {@code variables[j]} es la columna {@code j} (sin nulos ni NaN). La semilla
     * fija el resultado.
     */
    public static KMeansResult kmeans(double[][] variables, long k, long seed) {
        try (Arena arena = Arena.ofConfined(); Columns list = Columns.ofColumns(variables)) {
            MemorySegment labels = arena.allocate(Natives.ADDRESS);
            MemorySegment centroids = arena.allocate(Natives.ADDRESS);
            MemorySegment inertia = arena.allocate(Natives.DOUBLE);
            MemorySegment iterations = arena.allocate(Natives.LONG);
            Natives.check((int) Natives.KMEANS.invoke(
                    list.pointers(arena), (long) list.size(), k, 100L, 1e-4, seed, labels, centroids, inertia, iterations));
            try (Column l = Columns.wrap(labels.get(Natives.ADDRESS, 0));
                    Column c = Columns.wrap(centroids.get(Natives.ADDRESS, 0))) {
                return new KMeansResult(
                        l.toLongArray(),
                        reshape(c.toDoubleArray(), (int) k, variables.length),
                        inertia.get(Natives.DOUBLE, 0),
                        iterations.get(Natives.LONG, 0));
            }
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    // --- Auxiliares.

    private static boolean integer(Column column) {
        int dtype = column.dtype();
        return dtype == 1 || dtype == 2;
    }

    private interface Call {
        int run(MemorySegment out) throws Throwable;
    }

    private static double withOut(Call call) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.DOUBLE);
            Natives.check(call.run(out));
            return out.get(Natives.DOUBLE, 0);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    private static double doubleScalar(java.lang.invoke.MethodHandle handle, Column column) {
        return withOut(out -> (int) handle.invoke(column.handle(), 1L, out));
    }

    private static long longScalar(java.lang.invoke.MethodHandle handle, Column column) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.LONG);
            Natives.check((int) handle.invoke(column.handle(), 1L, out));
            return out.get(Natives.LONG, 0);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    private static double[][] reshape(double[] flat, int rows, int cols) {
        if ((long) rows * cols != flat.length) {
            throw new IllegalStateException("la forma " + rows + "x" + cols + " no coincide con " + flat.length + " valores");
        }
        double[][] out = new double[rows][cols];
        for (int i = 0; i < rows; i++) {
            System.arraycopy(flat, i * cols, out[i], 0, cols);
        }
        return out;
    }
}
