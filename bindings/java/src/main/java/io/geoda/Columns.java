package io.geoda;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.util.ArrayList;
import java.util.List;

/** Lista de columnas que se entrega a la capi como un arreglo de handles (una matriz o una tabla). */
final class Columns implements AutoCloseable {
    private final List<Column> columns = new ArrayList<>();

    /** Las columnas de una matriz dada por filas. */
    static Columns of(double[][] rows) {
        if (rows.length == 0) {
            throw new GeodaException(-1, "la matriz no tiene filas");
        }
        int width = rows[0].length;
        for (double[] row : rows) {
            if (row.length != width) {
                throw new GeodaException(-3, "las filas miden distinto");
            }
        }
        if (width == 0) {
            throw new GeodaException(-1, "la matriz no tiene columnas");
        }
        double[][] byColumn = new double[width][rows.length];
        for (int i = 0; i < rows.length; i++) {
            for (int j = 0; j < width; j++) {
                byColumn[j][i] = rows[i][j];
            }
        }
        return ofColumns(byColumn);
    }

    /** Una tabla dada ya por columnas. */
    static Columns ofColumns(double[][] byColumn) {
        if (byColumn.length == 0) {
            throw new GeodaException(-1, "no hay variables");
        }
        Columns list = new Columns();
        try {
            for (double[] values : byColumn) {
                list.columns.add(Column.of(values));
            }
        } catch (RuntimeException e) {
            list.close();
            throw e;
        }
        return list;
    }

    /** Envuelve un handle que devolvio la capi. */
    static Column wrap(MemorySegment handle) {
        return Column.adopt(handle);
    }

    int size() {
        return columns.size();
    }

    /** {@code const GeodaColumn *const *}: un puntero por columna. */
    MemorySegment pointers(Arena arena) {
        MemorySegment array = arena.allocate(Natives.ADDRESS.byteSize() * columns.size(), 8);
        for (int i = 0; i < columns.size(); i++) {
            array.setAtIndex(Natives.ADDRESS, i, columns.get(i).handle());
        }
        return array;
    }

    @Override
    public void close() {
        columns.forEach(Column::close);
        columns.clear();
    }
}
