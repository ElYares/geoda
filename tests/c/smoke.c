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

/* Columna sin nulos de `n` valores de 8 bytes: Int64 ("l") o Float64 ("g"). */
static struct GeodaColumn *columna_plana(const char *formato, const void *valores, size_t n) {
    struct ArrowSchema schema;
    struct ArrowArray array;
    void *datos = malloc(n * 8);
    const void **buffers = malloc(2 * sizeof(void *));
    memcpy(datos, valores, n * 8);
    buffers[0] = NULL;
    buffers[1] = datos;
    memset(&schema, 0, sizeof schema);
    schema.format = formato;
    schema.name = "x";
    schema.release = release_schema;
    memset(&array, 0, sizeof array);
    array.length = (long long)n;
    array.n_buffers = 2;
    array.buffers = buffers;
    array.release = release_array;
    struct GeodaColumn *col = NULL;
    CHECK(geoda_column_from_arrow(&schema, &array, &col) == GEODA_OK);
    schema.release(&schema);
    return col;
}

/* Grafos: se crea uno con pesos, se consulta y se libera; los errores dejan los `out` en NULL. */
static void prueba_grafo(void) {
    /* 0 -> 1 (4), 0 -> 2 (1), 2 -> 1 (2), 1 -> 3 (1); el nodo 4 queda aislado. */
    const long long desde[] = {0, 0, 2, 1}, hasta[] = {1, 2, 1, 3};
    const double pesos[] = {4.0, 1.0, 2.0, 1.0};
    struct GeodaColumn *from = columna_plana("l", desde, 4);
    struct GeodaColumn *to = columna_plana("l", hasta, 4);
    struct GeodaColumn *weights = columna_plana("g", pesos, 4);
    struct GeodaGraph *grafo = (struct GeodaGraph *)1;
    CHECK(geoda_graph_new(5, from, to, weights, true, &grafo) == GEODA_OK);
    CHECK(grafo != NULL && geoda_graph_node_count(grafo) == 5);

    struct GeodaColumn *distancia = NULL, *padre = NULL, *orden = NULL;
    CHECK(geoda_graph_dijkstra(grafo, 0, &distancia, &padre) == GEODA_OK);
    double suma = 0.0;
    CHECK(geoda_column_len(distancia) == 5 && geoda_column_len(padre) == 5);
    CHECK(geoda_column_null_count(padre) == 2); /* el origen y el nodo inalcanzable */
    CHECK(geoda_column_min_f64(distancia, 1, &suma) == GEODA_OK && suma == 0.0);
    geoda_column_free(distancia);
    geoda_column_free(padre);

    /* Un origen fuera de rango falla y deja los `out` en NULL. */
    distancia = padre = (struct GeodaColumn *)1;
    CHECK(geoda_graph_dijkstra(grafo, 5, &distancia, &padre) == GEODA_ERR_OUT_OF_BOUNDS);
    CHECK(distancia == NULL && padre == NULL);
    CHECK(strlen(geoda_last_error_message()) > 0);

    struct GeodaColumn *distancias_bfs = NULL;
    CHECK(geoda_graph_bfs(grafo, 0, &orden, &distancias_bfs, &padre) == GEODA_OK);
    CHECK(geoda_column_len(orden) == 4 && geoda_column_len(distancias_bfs) == 5);
    geoda_column_free(orden);
    geoda_column_free(distancias_bfs);
    geoda_column_free(padre);

    struct GeodaColumn *ids = NULL;
    int64_t cuantas = -1;
    CHECK(geoda_graph_components(grafo, &ids, &cuantas) == GEODA_OK && cuantas == 2);
    geoda_column_free(ids);

    /* Dirigido: el arbol de expansion minimo no aplica y no escribe nada. */
    struct GeodaColumn *aristas = (struct GeodaColumn *)1;
    double total = -1.0;
    cuantas = -1;
    CHECK(geoda_graph_mst(grafo, &aristas, &total, &cuantas) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(aristas == NULL && total == -1.0 && cuantas == -1);
    CHECK(geoda_graph_topological_order(grafo, &orden) == GEODA_OK && geoda_column_len(orden) == 5);
    geoda_column_free(orden);
    geoda_graph_free(grafo);

    /* Sin pesos no hay Dijkstra. */
    grafo = NULL;
    CHECK(geoda_graph_new(5, from, to, NULL, true, &grafo) == GEODA_OK);
    CHECK(geoda_graph_dijkstra(grafo, 0, &distancia, &padre) == GEODA_ERR_TYPE_MISMATCH);
    geoda_graph_free(grafo);

    /* Nodo fuera de rango, pesos de otro largo y punteros NULL. */
    grafo = (struct GeodaGraph *)1;
    CHECK(geoda_graph_new(3, from, to, NULL, true, &grafo) == GEODA_ERR_OUT_OF_BOUNDS);
    CHECK(grafo == NULL);
    CHECK(geoda_graph_new(5, from, to, from, true, &grafo) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(geoda_graph_new(5, NULL, to, NULL, true, &grafo) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_graph_node_count(NULL) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_graph_bfs(NULL, 0, &orden, &distancia, &padre) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_graph_free(NULL); /* acepta NULL */

    geoda_column_free(from);
    geoda_column_free(to);
    geoda_column_free(weights);
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
    CHECK(geoda_column_sum_i64(col, 1, &entero) == GEODA_OK);
    CHECK(entero == 40);
    CHECK(geoda_column_min_i64(col, 1, &entero) == GEODA_OK);
    CHECK(entero == 10);
    CHECK(geoda_column_max_i64(col, 1, &entero) == GEODA_OK);
    CHECK(entero == 30);
    CHECK(geoda_column_mean(col, 1, &valor) == GEODA_OK);
    CHECK(valor == 20.0);
    CHECK(geoda_column_var(col, 1, 1, &valor) == GEODA_OK);
    CHECK(valor == 200.0);
    CHECK(geoda_column_std(col, 0, 1, &valor) == GEODA_OK);
    CHECK(valor == 10.0);
    valor = -1.0;
    CHECK(geoda_column_sum_f64(col, 1, &valor) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(valor == -1.0);
    CHECK(strlen(geoda_last_error_message()) > 0);
    /* ddof >= n valores validos, sin escribir `out`. */
    CHECK(geoda_column_var(col, 2, 1, &valor) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(valor == -1.0);
    /* Correlacion de una columna consigo misma, y punteros NULL. */
    CHECK(geoda_column_corr(col, col, 1, &valor) == GEODA_OK);
    CHECK(valor > 0.999999 && valor <= 1.0);
    CHECK(geoda_column_corr(col, NULL, 1, &valor) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_count(NULL, &entero) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_mean(col, 1, NULL) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(col);
    schema.release(&schema);

    /* Histograma de [10, NULL, 30] con 2 cubos y rango automatico: devuelve TRES columnas
       nuevas (bordes, conteos, fuera), todas a liberar. Si falla, los tres `out` son NULL. */
    int64_con_un_nulo(&schema, &array);
    CHECK(geoda_column_from_arrow(&schema, &array, &col) == GEODA_OK);
    struct GeodaColumn *bordes = NULL, *conteos = NULL, *fuera = NULL;
    CHECK(geoda_column_histogram(col, 2, false, 0.0, 0.0, 1, &bordes, &conteos, &fuera) == GEODA_OK);
    CHECK(geoda_column_len(bordes) == 3 && geoda_column_len(conteos) == 2);
    CHECK(geoda_column_len(fuera) == 4);
    CHECK(geoda_column_min_f64(bordes, 1, &valor) == GEODA_OK && valor == 10.0);
    CHECK(geoda_column_max_f64(bordes, 1, &valor) == GEODA_OK && valor == 30.0);
    CHECK(geoda_column_sum_i64(conteos, 1, &entero) == GEODA_OK && entero == 2);
    CHECK(geoda_column_sum_i64(fuera, 1, &entero) == GEODA_OK && entero == 1); /* un nulo */
    geoda_column_free(bordes);
    geoda_column_free(conteos);
    geoda_column_free(fuera);
    /* Con rango explicito [0, 20]: el 30 cae fuera (`above`). */
    CHECK(geoda_column_histogram(col, 2, true, 0.0, 20.0, 1, &bordes, &conteos, &fuera) == GEODA_OK);
    CHECK(geoda_column_max_i64(fuera, 1, &entero) == GEODA_OK && entero == 1);
    geoda_column_free(bordes);
    geoda_column_free(conteos);
    geoda_column_free(fuera);
    bordes = conteos = fuera = (struct GeodaColumn *)1;
    CHECK(geoda_column_histogram(col, 0, false, 0.0, 0.0, 1, &bordes, &conteos, &fuera) ==
          GEODA_ERR_INVALID_ARGUMENT);
    CHECK(bordes == NULL && conteos == NULL && fuera == NULL);
    CHECK(strlen(geoda_last_error_message()) > 0);
    CHECK(geoda_column_histogram(col, 2, false, 0.0, 0.0, 1, NULL, &conteos, &fuera) ==
          GEODA_ERR_INVALID_ARGUMENT);

    /* Agregado por grupo: las claves son [10, NULL, 30] y los valores, los mismos. Salen dos
       columnas nuevas: claves (con el grupo nulo) y resultados. */
    struct GeodaColumn *claves = NULL, *resultados = NULL;
    CHECK(geoda_column_group_by(col, col, GEODA_AGG_SUM, 0, 1, &claves, &resultados) == GEODA_OK);
    CHECK(geoda_column_len(claves) == 3 && geoda_column_null_count(claves) == 1);
    CHECK(geoda_column_sum_i64(resultados, 1, &entero) == GEODA_OK && entero == 40);
    geoda_column_free(claves);
    geoda_column_free(resultados);
    claves = resultados = (struct GeodaColumn *)1;
    CHECK(geoda_column_group_by(col, col, 99, 0, 1, &claves, &resultados) ==
          GEODA_ERR_INVALID_ARGUMENT);
    CHECK(claves == NULL && resultados == NULL);
    CHECK(geoda_column_group_by(col, NULL, GEODA_AGG_SUM, 0, 1, &claves, &resultados) ==
          GEODA_ERR_INVALID_ARGUMENT);
    /* Hilos: con varios el resultado es el mismo; `workers = 0` es INVALID_ARGUMENT y no
       escribe `out`. */
    CHECK(geoda_column_sum_i64(col, 4, &entero) == GEODA_OK && entero == 40);
    valor = -1.0;
    CHECK(geoda_column_mean(col, 0, &valor) == GEODA_ERR_INVALID_ARGUMENT && valor == -1.0);
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

    prueba_grafo();

    if (fallos == 0) {
        puts("smoke: ok");
        return 0;
    }
    fprintf(stderr, "smoke: %d fallos\n", fallos);
    return 1;
}
