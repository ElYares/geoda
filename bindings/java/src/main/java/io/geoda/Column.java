package io.geoda;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.nio.charset.StandardCharsets;

/**
 * Columna de geoda (memoria nativa). Usar con {@code try-with-resources}: {@link #close()} la libera una
 * sola vez y despues cualquier uso lanza {@link IllegalStateException}.
 *
 * <p>Los nulos se dan y se leen con una mascara {@code boolean[]} (true = valido) o {@code null} para
 * "todos validos".
 */
public final class Column implements AutoCloseable {
    private MemorySegment handle;

    private Column(MemorySegment handle) {
        this.handle = handle;
    }

    /** Envuelve un handle que devolvio la capi (el paquete toma posesion). */
    static Column adopt(MemorySegment handle) {
        return new Column(handle);
    }

    // --- Crear.

    public static Column of(double[] values) {
        return of(values, null);
    }

    public static Column of(double[] values, boolean[] valid) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.FROM_F64.invoke(
                    arena.allocateFrom(Natives.DOUBLE, values), (long) values.length, validity(arena, valid, values.length), out));
            return new Column(out.get(Natives.ADDRESS, 0));
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    public static Column of(int[] values) {
        return of(values, null);
    }

    public static Column of(int[] values, boolean[] valid) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.FROM_I32.invoke(
                    arena.allocateFrom(Natives.INT, values), (long) values.length, validity(arena, valid, values.length), out));
            return new Column(out.get(Natives.ADDRESS, 0));
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    public static Column of(long[] values) {
        return of(values, null);
    }

    public static Column of(long[] values, boolean[] valid) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.FROM_I64.invoke(
                    arena.allocateFrom(Natives.LONG, values), (long) values.length, validity(arena, valid, values.length), out));
            return new Column(out.get(Natives.ADDRESS, 0));
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    public static Column ofBooleans(boolean[] values, boolean[] valid) {
        byte[] bytes = new byte[values.length];
        for (int i = 0; i < bytes.length; i++) {
            bytes[i] = (byte) (values[i] ? 1 : 0);
        }
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.FROM_BOOL.invoke(
                    arena.allocateFrom(Natives.BYTE, bytes), (long) bytes.length, validity(arena, valid, bytes.length), out));
            return new Column(out.get(Natives.ADDRESS, 0));
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Texto UTF-8; un elemento {@code null} es un nulo. */
    public static Column ofStrings(String[] values) {
        java.io.ByteArrayOutputStream data = new java.io.ByteArrayOutputStream();
        int[] offsets = new int[values.length + 1];
        boolean[] valid = new boolean[values.length];
        for (int i = 0; i < values.length; i++) {
            valid[i] = values[i] != null;
            if (values[i] != null) {
                byte[] bytes = values[i].getBytes(StandardCharsets.UTF_8);
                data.write(bytes, 0, bytes.length);
            }
            offsets[i + 1] = data.size();
        }
        byte[] all = data.toByteArray();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(Natives.ADDRESS);
            Natives.check((int) Natives.FROM_UTF8.invoke(
                    arena.allocateFrom(Natives.BYTE, all), (long) all.length,
                    arena.allocateFrom(Natives.INT, offsets), (long) values.length,
                    validity(arena, valid, values.length), out));
            return new Column(out.get(Natives.ADDRESS, 0));
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    private static MemorySegment validity(Arena arena, boolean[] valid, int length) {
        if (valid == null) {
            return MemorySegment.NULL;
        }
        if (valid.length != length) {
            throw new IllegalArgumentException("valid mide " + valid.length + " y values " + length);
        }
        byte[] bytes = new byte[length];
        for (int i = 0; i < length; i++) {
            bytes[i] = (byte) (valid[i] ? 1 : 0);
        }
        return arena.allocateFrom(Natives.BYTE, bytes);
    }

    // --- Leer.

    /** El handle nativo (para otros metodos del paquete). */
    MemorySegment handle() {
        if (handle == null) {
            throw new IllegalStateException("la columna ya se libero");
        }
        return handle;
    }

    public int size() {
        try {
            long n = (long) Natives.LEN.invoke(handle());
            if (n < 0) {
                Natives.check((int) n);
            }
            return Math.toIntExact(n);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    public int nullCount() {
        try {
            long n = (long) Natives.NULL_COUNT.invoke(handle());
            if (n < 0) {
                Natives.check((int) n);
            }
            return Math.toIntExact(n);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Una de las constantes {@code GEODA_DTYPE_*}: 0 bool, 1 int32, 2 int64, 3 float64, 4 utf8. */
    public int dtype() {
        try {
            int d = (int) Natives.DTYPE.invoke(handle());
            Natives.check(d < 0 ? d : 0);
            return d;
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** true = valida, false = nula. */
    public boolean[] validity() {
        int n = size();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment buffer = arena.allocate(Math.max(n, 1));
            Natives.check((int) Natives.COPY_VALIDITY.invoke(handle(), buffer, (long) n));
            boolean[] out = new boolean[n];
            for (int i = 0; i < n; i++) {
                out[i] = buffer.get(Natives.BYTE, i) != 0;
            }
            return out;
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Los valores de una columna Float64 (un nulo vale 0.0). */
    public double[] toDoubleArray() {
        int n = size();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment buffer = arena.allocate(8L * Math.max(n, 1), 8);
            Natives.check((int) Natives.COPY_F64.invoke(handle(), buffer, (long) n));
            return buffer.asSlice(0, 8L * n).toArray(Natives.DOUBLE);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Los valores de una columna Int32 (un nulo vale 0). */
    public int[] toIntArray() {
        int n = size();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment buffer = arena.allocate(4L * Math.max(n, 1), 4);
            Natives.check((int) Natives.COPY_I32.invoke(handle(), buffer, (long) n));
            return buffer.asSlice(0, 4L * n).toArray(Natives.INT);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Los valores de una columna Int64 (un nulo vale 0). */
    public long[] toLongArray() {
        int n = size();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment buffer = arena.allocate(8L * Math.max(n, 1), 8);
            Natives.check((int) Natives.COPY_I64.invoke(handle(), buffer, (long) n));
            return buffer.asSlice(0, 8L * n).toArray(Natives.LONG);
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Los valores de una columna Bool (un nulo es false). */
    public boolean[] toBooleanArray() {
        int n = size();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment buffer = arena.allocate(Math.max(n, 1));
            Natives.check((int) Natives.COPY_BOOL.invoke(handle(), buffer, (long) n));
            boolean[] out = new boolean[n];
            for (int i = 0; i < n; i++) {
                out[i] = buffer.get(Natives.BYTE, i) != 0;
            }
            return out;
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** El texto de una columna Utf8; {@code null} en los nulos. */
    public String[] toStringArray() {
        int n = size();
        boolean[] valid = validity();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment total = arena.allocate(Natives.LONG);
            Natives.check((int) Natives.UTF8_DATA_LEN.invoke(handle(), total));
            long bytes = total.get(Natives.LONG, 0);
            MemorySegment data = arena.allocate(Math.max(bytes, 1));
            MemorySegment offsets = arena.allocate(4L * (n + 1), 4);
            Natives.check((int) Natives.COPY_UTF8.invoke(handle(), data, bytes, offsets, (long) (n + 1)));
            String[] out = new String[n];
            for (int i = 0; i < n; i++) {
                if (valid[i]) {
                    int from = offsets.get(Natives.INT, 4L * i);
                    int to = offsets.get(Natives.INT, 4L * (i + 1));
                    out[i] = new String(data.asSlice(from, to - from).toArray(Natives.BYTE), StandardCharsets.UTF_8);
                }
            }
            return out;
        } catch (Throwable t) {
            throw Natives.rethrow(t);
        }
    }

    /** Libera la memoria nativa. Es idempotente. */
    @Override
    public void close() {
        if (handle != null) {
            MemorySegment h = handle;
            handle = null;
            try {
                Natives.FREE.invoke(h);
            } catch (Throwable t) {
                throw Natives.rethrow(t);
            }
        }
    }
}
