package io.geoda;

import java.lang.foreign.AddressLayout;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Enlaces a mano con la C-API (JDK 22+, sin jextract). Solo lo usa el paquete.
 *
 * <p>La biblioteca se busca en la propiedad {@code geoda.lib}, en la variable {@code GEODA_LIB} (ruta
 * a {@code libgeoda.so}) o, si no, en el {@code java.library.path} como {@code libgeoda}.
 */
final class Natives {
    static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT;
    static final ValueLayout.OfLong LONG = ValueLayout.JAVA_LONG;
    static final ValueLayout.OfDouble DOUBLE = ValueLayout.JAVA_DOUBLE;
    static final ValueLayout.OfByte BYTE = ValueLayout.JAVA_BYTE;
    static final AddressLayout ADDRESS = ValueLayout.ADDRESS;

    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup LOOKUP = lookup();

    static final MethodHandle ABI_VERSION = handle("geoda_abi_version", INT);
    static final MethodHandle LAST_ERROR = handle("geoda_last_error_message", ADDRESS);

    static final MethodHandle FROM_F64 = handle("geoda_column_from_f64", INT, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle FROM_I32 = handle("geoda_column_from_i32", INT, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle FROM_I64 = handle("geoda_column_from_i64", INT, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle FROM_BOOL = handle("geoda_column_from_bool", INT, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle FROM_UTF8 =
            handle("geoda_column_from_utf8", INT, ADDRESS, LONG, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle FREE = handle("geoda_column_free", null, ADDRESS);
    static final MethodHandle LEN = handle("geoda_column_len", LONG, ADDRESS);
    static final MethodHandle NULL_COUNT = handle("geoda_column_null_count", LONG, ADDRESS);
    static final MethodHandle DTYPE = handle("geoda_column_dtype", INT, ADDRESS);
    static final MethodHandle COPY_F64 = handle("geoda_column_copy_f64", INT, ADDRESS, ADDRESS, LONG);
    static final MethodHandle COPY_I32 = handle("geoda_column_copy_i32", INT, ADDRESS, ADDRESS, LONG);
    static final MethodHandle COPY_I64 = handle("geoda_column_copy_i64", INT, ADDRESS, ADDRESS, LONG);
    static final MethodHandle COPY_BOOL = handle("geoda_column_copy_bool", INT, ADDRESS, ADDRESS, LONG);
    static final MethodHandle COPY_VALIDITY = handle("geoda_column_copy_validity", INT, ADDRESS, ADDRESS, LONG);
    static final MethodHandle UTF8_DATA_LEN = handle("geoda_column_utf8_data_len", INT, ADDRESS, ADDRESS);
    static final MethodHandle COPY_UTF8 =
            handle("geoda_column_copy_utf8", INT, ADDRESS, ADDRESS, LONG, ADDRESS, LONG);

    static final MethodHandle COUNT = handle("geoda_column_count", INT, ADDRESS, ADDRESS);
    static final MethodHandle SUM_I64 = handle("geoda_column_sum_i64", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle SUM_F64 = handle("geoda_column_sum_f64", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle MIN_I64 = handle("geoda_column_min_i64", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle MIN_F64 = handle("geoda_column_min_f64", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle MAX_I64 = handle("geoda_column_max_i64", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle MAX_F64 = handle("geoda_column_max_f64", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle MEAN = handle("geoda_column_mean", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle VAR = handle("geoda_column_var", INT, ADDRESS, LONG, LONG, ADDRESS);
    static final MethodHandle STD = handle("geoda_column_std", INT, ADDRESS, LONG, LONG, ADDRESS);
    static final MethodHandle MEDIAN = handle("geoda_column_median", INT, ADDRESS, ADDRESS);
    static final MethodHandle PERCENTILE = handle("geoda_column_percentile", INT, ADDRESS, DOUBLE, ADDRESS);
    static final MethodHandle CORR = handle("geoda_column_corr", INT, ADDRESS, ADDRESS, LONG, ADDRESS);
    static final MethodHandle EDIT_DISTANCE = handle("geoda_column_edit_distance", INT, ADDRESS, ADDRESS, ADDRESS);

    static final MethodHandle MATMUL = handle("geoda_matmul", INT, ADDRESS, LONG, ADDRESS, LONG, ADDRESS);
    static final MethodHandle LU_SOLVE = handle("geoda_lu_solve", INT, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle DET = handle("geoda_det", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle INVERSE = handle("geoda_inverse", INT, ADDRESS, LONG, ADDRESS);
    static final MethodHandle KMEANS = handle(
            "geoda_kmeans", INT, ADDRESS, LONG, LONG, LONG, DOUBLE, LONG, ADDRESS, ADDRESS, ADDRESS, ADDRESS);

    static final MethodHandle QR = handle("geoda_qr", INT, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle SVD = handle("geoda_svd", INT, ADDRESS, LONG, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle LSTSQ = handle("geoda_lstsq", INT, ADDRESS, LONG, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle LIS = handle("geoda_column_lis", INT, ADDRESS, ADDRESS);
    static final MethodHandle DEDUPE = handle("geoda_column_dedupe", INT, ADDRESS, LONG, ADDRESS, ADDRESS);
    static final MethodHandle HISTOGRAM = handle(
            "geoda_column_histogram", INT, ADDRESS, LONG, ValueLayout.JAVA_BOOLEAN, DOUBLE, DOUBLE, LONG,
            ADDRESS, ADDRESS, ADDRESS);

    private Natives() {}

    private static SymbolLookup lookup() {
        String configured = System.getProperty("geoda.lib");
        if (configured == null || configured.isEmpty()) {
            configured = System.getenv("GEODA_LIB");
        }
        Arena global = Arena.global();
        if (configured != null && !configured.isEmpty()) {
            Path path = Path.of(configured);
            if (Files.isDirectory(path)) {
                path = path.resolve(System.mapLibraryName("geoda"));
            }
            return SymbolLookup.libraryLookup(path, global);
        }
        return SymbolLookup.libraryLookup(System.mapLibraryName("geoda"), global);
    }

    private static MethodHandle handle(String name, MemoryLayout result, MemoryLayout... args) {
        MemorySegment symbol = LOOKUP.find(name)
                .orElseThrow(() -> new UnsatisfiedLinkError("libgeoda no exporta " + name));
        FunctionDescriptor descriptor = result == null
                ? FunctionDescriptor.ofVoid(args)
                : FunctionDescriptor.of(result, args);
        return LINKER.downcallHandle(symbol, descriptor);
    }

    /** Lanza una {@link GeodaException} si {@code code} es un error de la capi. */
    static void check(int code) {
        if (code != 0) {
            throw new GeodaException(code, lastError());
        }
    }

    static String lastError() {
        try {
            MemorySegment message = (MemorySegment) LAST_ERROR.invokeExact();
            return message.reinterpret(Integer.MAX_VALUE).getString(0);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    /** Propaga lo que lanzo un {@code invokeExact}: nunca es una excepcion comprobada de verdad. */
    static RuntimeException rethrow(Throwable t) {
        if (t instanceof RuntimeException e) {
            return e;
        }
        if (t instanceof Error e) {
            throw e;
        }
        return new IllegalStateException(t);
    }
}
