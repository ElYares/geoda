package io.geoda;

/** Error de la C-API de geoda: el codigo negativo, su categoria y el mensaje de la capi. */
public final class GeodaException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    private final int code;

    GeodaException(int code, String message) {
        super(category(code) + ": " + message);
        this.code = code;
    }

    /** Codigo negativo {@code GEODA_ERR_*}. */
    public int code() {
        return code;
    }

    /** Categoria como texto ({@code "INVALID_ARGUMENT"}, {@code "TYPE_MISMATCH"}...). */
    public String category() {
        return category(code);
    }

    static String category(int code) {
        return switch (code) {
            case -1 -> "INVALID_ARGUMENT";
            case -2 -> "TYPE_MISMATCH";
            case -3 -> "LENGTH_MISMATCH";
            case -4 -> "OUT_OF_BOUNDS";
            case -5 -> "UNSUPPORTED";
            case -6 -> "ALLOC";
            case -7 -> "FFI";
            case -8 -> "PANIC";
            default -> "INTERNAL";
        };
    }
}
