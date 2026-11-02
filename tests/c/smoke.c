/* Prueba de humo de la C-API: la usa un programa en C, sin nada de Rust. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "geoda.h"

static int fallos = 0;

#define CHECK(cond)                                                          \
    do {                                                                     \
        if (!(cond)) {                                                       \
            fprintf(stderr, "%s:%d: fallo: %s\n", __FILE__, __LINE__, #cond); \
            fallos++;                                                        \
        }                                                                    \
    } while (0)

/* Productor de Arrow minimo: una columna Int64 con memoria propia (malloc). */
static int liberados = 0;

static void release_schema(struct ArrowSchema *s) {
    s->release = NULL;
}

static void release_array(struct ArrowArray *a) {
    free((void *)a->buffers[0]);
    free((void *)a->buffers[1]);
    free((void *)a->buffers);
    a->release = NULL;
    liberados++;
}

/* [10, NULL, 30]: bits de validez 0b101. */
static void int64_con_un_nulo(struct ArrowSchema *s, struct ArrowArray *a) {
    unsigned char *validez = malloc(1);
    long long *datos = malloc(3 * sizeof(long long));
    const void **buffers = malloc(2 * sizeof(void *));
    validez[0] = 0x05;
    datos[0] = 10;
    datos[1] = 0;
    datos[2] = 30;
    buffers[0] = validez;
    buffers[1] = datos;

    memset(s, 0, sizeof *s);
    s->format = "l";
    s->name = "x";
    s->flags = 2; /* ARROW_FLAG_NULLABLE */
    s->release = release_schema;

    memset(a, 0, sizeof *a);
    a->length = 3;
    a->null_count = 1;
    a->n_buffers = 2;
    a->buffers = buffers;
    a->release = release_array;
}

int main(void) {
    /* Version de la ABI: major 0, minor 1. */
    CHECK(geoda_abi_version() == ((0u << 16) | 1u));

    /* Sin errores previos el mensaje es una cadena vacia, no NULL. */
    CHECK(geoda_last_error_message() != NULL);
    CHECK(strlen(geoda_last_error_message()) == 0);

    /* Camino feliz: la columna llega con len y null_count correctos. */
    struct ArrowSchema schema;
    struct ArrowArray array;
    int64_con_un_nulo(&schema, &array);
    struct GeodaColumn *col = NULL;
    CHECK(geoda_column_from_arrow(&schema, &array, &col) == GEODA_OK);
    CHECK(col != NULL);
    CHECK(geoda_column_len(col) == 3);
    CHECK(geoda_column_null_count(col) == 1);
    /* El import copia y consume el array: ya se libero una vez. */
    CHECK(array.release == NULL);
    CHECK(liberados == 1);

    /* Export: structs nuevas que comparten memoria con la columna; el llamador
       las libera con su release. */
    struct ArrowSchema out_schema;
    struct ArrowArray out_array;
    CHECK(geoda_column_to_arrow(col, &out_schema, &out_array) == GEODA_OK);
    CHECK(out_array.length == 3);
    CHECK(out_array.null_count == 1);
    CHECK(out_schema.format != NULL && strcmp(out_schema.format, "l") == 0);
    /* Sigue viva aunque se libere el handle. */
    geoda_column_free(col);
    CHECK(out_array.release != NULL);
    out_array.release(&out_array);
    out_schema.release(&out_schema);
    CHECK(out_array.release == NULL);

    /* Slice: [10, NULL, 30] -> [NULL, 30]. */
    int64_con_un_nulo(&schema, &array);
    CHECK(geoda_column_from_arrow(&schema, &array, &col) == GEODA_OK);
    struct GeodaColumn *corte = NULL;
    CHECK(geoda_column_slice(col, 1, 2, &corte) == GEODA_OK);
    CHECK(geoda_column_len(corte) == 2);
    CHECK(geoda_column_null_count(corte) == 1);
    geoda_column_free(corte);
    corte = (struct GeodaColumn *)1;
    CHECK(geoda_column_slice(col, 2, 5, &corte) == GEODA_ERR_OUT_OF_BOUNDS);
    CHECK(corte == NULL);
    CHECK(strlen(geoda_last_error_message()) > 0);
    geoda_column_free(col);
    geoda_column_free(NULL); /* no-op */
    schema.release(&schema);

    /* Mediana y percentil: [10, NULL, 30] ignora el nulo. El valor sale por el
       puntero y el retorno es solo el codigo. */
    int64_con_un_nulo(&schema, &array);
    CHECK(geoda_column_from_arrow(&schema, &array, &col) == GEODA_OK);
    double valor = -1.0;
    CHECK(geoda_column_median(col, &valor) == GEODA_OK);
    CHECK(valor == 20.0);
    CHECK(geoda_column_percentile(col, 25.0, &valor) == GEODA_OK);
    CHECK(valor == 15.0);
    /* Si falla, `out` no se toca y hay mensaje. */
    valor = -1.0;
    CHECK(geoda_column_percentile(col, 101.0, &valor) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(valor == -1.0);
    CHECK(strlen(geoda_last_error_message()) > 0);
    CHECK(geoda_column_median(col, NULL) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_median(NULL, &valor) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(col);
    schema.release(&schema);

    /* Estadisticos escalares sobre [10, NULL, 30]: count, sum, min, max, mean, var, std.
       `sum`, `min` y `max` conservan el tipo (aqui Int64 -> `_i64`); la variante equivocada
       es TYPE_MISMATCH y no escribe `out`. */
    int64_con_un_nulo(&schema, &array);
    CHECK(geoda_column_from_arrow(&schema, &array, &col) == GEODA_OK);
    int64_t entero = -1;
    CHECK(geoda_column_count(col, &entero) == GEODA_OK);
    CHECK(entero == 2);
    CHECK(geoda_column_sum_i64(col, &entero) == GEODA_OK);
    CHECK(entero == 40);
    CHECK(geoda_column_min_i64(col, &entero) == GEODA_OK);
    CHECK(entero == 10);
    CHECK(geoda_column_max_i64(col, &entero) == GEODA_OK);
    CHECK(entero == 30);
    CHECK(geoda_column_mean(col, &valor) == GEODA_OK);
    CHECK(valor == 20.0);
    CHECK(geoda_column_var(col, 1, &valor) == GEODA_OK);
    CHECK(valor == 200.0);
    CHECK(geoda_column_std(col, 0, &valor) == GEODA_OK);
    CHECK(valor == 10.0);
    valor = -1.0;
    CHECK(geoda_column_sum_f64(col, &valor) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(valor == -1.0);
    CHECK(strlen(geoda_last_error_message()) > 0);
    /* ddof >= n valores validos, sin escribir `out`. */
    CHECK(geoda_column_var(col, 2, &valor) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(valor == -1.0);
    /* Correlacion de una columna consigo misma, y punteros NULL. */
    CHECK(geoda_column_corr(col, col, &valor) == GEODA_OK);
    CHECK(valor > 0.999999 && valor <= 1.0);
    CHECK(geoda_column_corr(col, NULL, &valor) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_count(NULL, &entero) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_mean(col, NULL) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(col);
    schema.release(&schema);

    /* Histograma de [10, NULL, 30] con 2 cubos y rango automatico: devuelve TRES columnas
       nuevas (bordes, conteos, fuera), todas a liberar. Si falla, los tres `out` son NULL. */
    int64_con_un_nulo(&schema, &array);
    CHECK(geoda_column_from_arrow(&schema, &array, &col) == GEODA_OK);
    struct GeodaColumn *bordes = NULL, *conteos = NULL, *fuera = NULL;
    CHECK(geoda_column_histogram(col, 2, false, 0.0, 0.0, &bordes, &conteos, &fuera) == GEODA_OK);
    CHECK(geoda_column_len(bordes) == 3 && geoda_column_len(conteos) == 2);
    CHECK(geoda_column_len(fuera) == 4);
    CHECK(geoda_column_min_f64(bordes, &valor) == GEODA_OK && valor == 10.0);
    CHECK(geoda_column_max_f64(bordes, &valor) == GEODA_OK && valor == 30.0);
    CHECK(geoda_column_sum_i64(conteos, &entero) == GEODA_OK && entero == 2);
    CHECK(geoda_column_sum_i64(fuera, &entero) == GEODA_OK && entero == 1); /* un nulo */
    geoda_column_free(bordes);
    geoda_column_free(conteos);
    geoda_column_free(fuera);
    /* Con rango explicito [0, 20]: el 30 cae fuera (`above`). */
    CHECK(geoda_column_histogram(col, 2, true, 0.0, 20.0, &bordes, &conteos, &fuera) == GEODA_OK);
    CHECK(geoda_column_max_i64(fuera, &entero) == GEODA_OK && entero == 1);
    geoda_column_free(bordes);
    geoda_column_free(conteos);
    geoda_column_free(fuera);
    bordes = conteos = fuera = (struct GeodaColumn *)1;
    CHECK(geoda_column_histogram(col, 0, false, 0.0, 0.0, &bordes, &conteos, &fuera) ==
          GEODA_ERR_INVALID_ARGUMENT);
    CHECK(bordes == NULL && conteos == NULL && fuera == NULL);
    CHECK(strlen(geoda_last_error_message()) > 0);
    CHECK(geoda_column_histogram(col, 2, false, 0.0, 0.0, NULL, &conteos, &fuera) ==
          GEODA_ERR_INVALID_ARGUMENT);

    /* Agregado por grupo: las claves son [10, NULL, 30] y los valores, los mismos. Salen dos
       columnas nuevas: claves (con el grupo nulo) y resultados. */
    struct GeodaColumn *claves = NULL, *resultados = NULL;
    CHECK(geoda_column_group_by(col, col, GEODA_AGG_SUM, 0, &claves, &resultados) == GEODA_OK);
    CHECK(geoda_column_len(claves) == 3 && geoda_column_null_count(claves) == 1);
    CHECK(geoda_column_sum_i64(resultados, &entero) == GEODA_OK && entero == 40);
    geoda_column_free(claves);
    geoda_column_free(resultados);
    claves = resultados = (struct GeodaColumn *)1;
    CHECK(geoda_column_group_by(col, col, 99, 0, &claves, &resultados) ==
          GEODA_ERR_INVALID_ARGUMENT);
    CHECK(claves == NULL && resultados == NULL);
    CHECK(geoda_column_group_by(col, NULL, GEODA_AGG_SUM, 0, &claves, &resultados) ==
          GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(col);
    schema.release(&schema);

    /* Entrada invalida: formato que geoda no soporta. */
    int64_con_un_nulo(&schema, &array);
    schema.format = "tdD"; /* decimal128 no esta soportado */
    col = (struct GeodaColumn *)1;
    int rc = geoda_column_from_arrow(&schema, &array, &col);
    CHECK(rc == GEODA_ERR_UNSUPPORTED);
    CHECK(rc < 0);
    CHECK(col == NULL);
    CHECK(strlen(geoda_last_error_message()) > 0);
    /* Al fallar no toma propiedad: el array sigue siendo nuestro. */
    CHECK(array.release != NULL);
    array.release(&array);
    schema.release(&schema);

    /* Punteros NULL. */
    CHECK(geoda_column_from_arrow(NULL, NULL, &col) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_len(NULL) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_to_arrow(NULL, &schema, &array) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_slice(NULL, 0, 0, &corte) == GEODA_ERR_INVALID_ARGUMENT);

    if (fallos == 0) {
        puts("smoke: ok");
        return 0;
    }
    fprintf(stderr, "smoke: %d fallos\n", fallos);
    return 1;
}
