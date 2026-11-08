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

/* Columna Int32 ("i") sin nulos de `n` valores. */
static struct GeodaColumn *columna_i32(const int *valores, size_t n) {
    struct ArrowSchema schema;
    struct ArrowArray array;
    void *datos = malloc(n * sizeof(int));
    const void **buffers = malloc(2 * sizeof(void *));
    memcpy(datos, valores, n * sizeof(int));
    buffers[0] = NULL;
    buffers[1] = datos;
    memset(&schema, 0, sizeof schema);
    schema.format = "i";
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

/* Texto: LIS, Huffman de ida y vuelta y los errores que dejan los `out` en NULL. */
static void prueba_texto(void) {
    const long long serie[] = {10, 9, 2, 5, 3, 7, 101, 18};
    struct GeodaColumn *valores = columna_plana("l", serie, 8);
    struct GeodaColumn *indices = (struct GeodaColumn *)1;
    CHECK(geoda_column_lis(valores, &indices) == GEODA_OK);
    CHECK(geoda_column_len(indices) == 4);
    geoda_column_free(indices);
    indices = (struct GeodaColumn *)1;
    CHECK(geoda_column_edit_distance(valores, valores, &indices) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(indices == NULL);
    CHECK(geoda_column_lis(NULL, &indices) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_lis(valores, NULL) == GEODA_ERR_INVALID_ARGUMENT);

    /* Dedupe: solo acepta texto (Int64 es TYPE_MISMATCH) y deja los `out` en NULL. */
    int64_t cuantos = -1;
    indices = (struct GeodaColumn *)1;
    CHECK(geoda_column_dedupe(valores, 1, &indices, &cuantos) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(indices == NULL && cuantos == -1);
    CHECK(geoda_column_dedupe(NULL, 1, &indices, &cuantos) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_column_dedupe(valores, 1, &indices, NULL) == GEODA_ERR_INVALID_ARGUMENT);

    /* Huffman: aaabbc -> 9 bits, 256 frecuencias, y de vuelta los mismos 6 bytes. */
    const int bytes[] = {97, 97, 97, 98, 98, 99};
    struct GeodaColumn *datos = columna_i32(bytes, 6);
    struct GeodaColumn *bits = (struct GeodaColumn *)1, *frecuencias = (struct GeodaColumn *)1;
    CHECK(geoda_huffman_encode(datos, &bits, &frecuencias) == GEODA_OK);
    CHECK(geoda_column_len(bits) == 9 && geoda_column_len(frecuencias) == 256);
    struct GeodaColumn *vuelta = (struct GeodaColumn *)1;
    CHECK(geoda_huffman_decode(frecuencias, bits, &vuelta) == GEODA_OK);
    CHECK(geoda_column_len(vuelta) == 6);
    geoda_column_free(vuelta);
    geoda_column_free(bits);
    geoda_column_free(frecuencias);
    /* Bytes que viajan como Int64: TYPE_MISMATCH y los dos `out` en NULL. */
    bits = frecuencias = (struct GeodaColumn *)1;
    CHECK(geoda_huffman_encode(valores, &bits, &frecuencias) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(bits == NULL && frecuencias == NULL);
    vuelta = (struct GeodaColumn *)1;
    CHECK(geoda_huffman_decode(valores, valores, &vuelta) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(vuelta == NULL);
    CHECK(geoda_huffman_decode(NULL, NULL, &vuelta) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(datos);
    geoda_column_free(valores);
}

/* Clustering: k-means y dendrograma desde una lista de columnas; los errores dejan los `out` en NULL. */
static void prueba_cluster(void) {
    const double xs[] = {0.0, 0.0, 10.0, 10.0}, ys[] = {0.0, 1.0, 10.0, 11.0};
    struct GeodaColumn *x = columna_plana("g", xs, 4), *y = columna_plana("g", ys, 4);
    const struct GeodaColumn *matriz[] = {x, y};
    struct GeodaColumn *etiquetas = (struct GeodaColumn *)1, *centroides = (struct GeodaColumn *)1;
    double inercia = -1.0;
    int64_t iteraciones = -1;
    CHECK(geoda_kmeans(matriz, 2, 2, 100, 1e-4, 1, &etiquetas, &centroides, &inercia,
                       &iteraciones) == GEODA_OK);
    CHECK(geoda_column_len(etiquetas) == 4 && geoda_column_len(centroides) == 4);
    CHECK(inercia == 1.0 && iteraciones >= 1);
    geoda_column_free(etiquetas);
    geoda_column_free(centroides);
    /* k = 0: INVALID_ARGUMENT, los `out` en NULL y los escalares sin tocar. */
    etiquetas = centroides = (struct GeodaColumn *)1;
    inercia = -1.0;
    CHECK(geoda_kmeans(matriz, 2, 0, 100, 1e-4, 1, &etiquetas, &centroides, &inercia,
                       &iteraciones) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(etiquetas == NULL && centroides == NULL && inercia == -1.0);
    CHECK(geoda_kmeans(NULL, 2, 2, 100, 1e-4, 1, &etiquetas, &centroides, &inercia,
                       &iteraciones) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_kmeans(matriz, 0, 2, 100, 1e-4, 1, &etiquetas, &centroides, &inercia,
                       &iteraciones) == GEODA_ERR_INVALID_ARGUMENT);
    const struct GeodaColumn *con_null[] = {x, NULL};
    CHECK(geoda_kmeans(con_null, 2, 2, 100, 1e-4, 1, &etiquetas, &centroides, &inercia,
                       &iteraciones) == GEODA_ERR_INVALID_ARGUMENT);

    struct GeodaColumn *izq = (struct GeodaColumn *)1, *der = (struct GeodaColumn *)1;
    struct GeodaColumn *altura = (struct GeodaColumn *)1, *tamano = (struct GeodaColumn *)1;
    CHECK(geoda_hierarchical(matriz, 2, &izq, &der, &altura, &tamano) == GEODA_OK);
    CHECK(geoda_column_len(izq) == 3 && geoda_column_len(altura) == 3);
    struct GeodaColumn *grupos = (struct GeodaColumn *)1;
    CHECK(geoda_dendrogram_cut(izq, der, 4, 2, &grupos) == GEODA_OK);
    CHECK(geoda_column_len(grupos) == 4);
    geoda_column_free(grupos);
    grupos = (struct GeodaColumn *)1;
    CHECK(geoda_dendrogram_cut(izq, der, 4, 0, &grupos) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(grupos == NULL);
    geoda_column_free(izq);
    geoda_column_free(der);
    geoda_column_free(altura);
    geoda_column_free(tamano);
    izq = (struct GeodaColumn *)1;
    CHECK(geoda_hierarchical(NULL, 1, &izq, &der, &altura, &tamano) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(izq == NULL);
    geoda_column_free(x);
    geoda_column_free(y);
}

/* Arbol: entrenar, predecir y liberar; los errores dejan los `out` en NULL. */
static void prueba_arbol(void) {
    const double xs[] = {1.0, 2.0, 3.0, 10.0, 11.0, 12.0};
    const long long etiquetas[] = {0, 0, 0, 1, 1, 1};
    struct GeodaColumn *x = columna_plana("g", xs, 6);
    struct GeodaColumn *y = columna_plana("l", etiquetas, 6);
    const struct GeodaColumn *matriz[] = {x};
    struct GeodaTree *arbol = (struct GeodaTree *)1;
    CHECK(geoda_tree_fit(matriz, 1, y, 8, 1, &arbol) == GEODA_OK);
    CHECK(arbol != NULL && geoda_tree_node_count(arbol) == 3);
    struct GeodaColumn *prediccion = (struct GeodaColumn *)1;
    CHECK(geoda_tree_predict(arbol, matriz, 1, &prediccion) == GEODA_OK);
    CHECK(geoda_column_len(prediccion) == 6);
    geoda_column_free(prediccion);
    /* Otro numero de variables que el entrenamiento: LENGTH_MISMATCH y `out` en NULL. */
    const struct GeodaColumn *dos[] = {x, x};
    prediccion = (struct GeodaColumn *)1;
    CHECK(geoda_tree_predict(arbol, dos, 2, &prediccion) == GEODA_ERR_LENGTH_MISMATCH);
    CHECK(prediccion == NULL);
    CHECK(geoda_tree_predict(NULL, matriz, 1, &prediccion) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_tree_node_count(NULL) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_tree_free(arbol);
    geoda_tree_free(NULL); /* acepta NULL */
    /* Errores al entrenar: min_samples_leaf = 0 y etiquetas Float64. */
    arbol = (struct GeodaTree *)1;
    CHECK(geoda_tree_fit(matriz, 1, y, 8, 0, &arbol) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(arbol == NULL);
    arbol = (struct GeodaTree *)1;
    CHECK(geoda_tree_fit(matriz, 1, x, 8, 1, &arbol) == GEODA_ERR_TYPE_MISMATCH);
    CHECK(arbol == NULL);
    CHECK(geoda_tree_fit(NULL, 1, y, 8, 1, &arbol) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(x);
    geoda_column_free(y);
}

/* Apriori: conjuntos y reglas en formato largo; los errores dejan todos los `out` en NULL. */
static void prueba_apriori(void) {
    /* {1,2,3}, {1,2}, {1,3}, {2,3} */
    const long long tx[] = {0, 0, 0, 1, 1, 2, 2, 3, 3}, it[] = {1, 2, 3, 1, 2, 1, 3, 2, 3};
    struct GeodaColumn *transaccion = columna_plana("l", tx, 9);
    struct GeodaColumn *item = columna_plana("l", it, 9);
    struct GeodaColumn *conjunto = (struct GeodaColumn *)1, *items = (struct GeodaColumn *)1;
    struct GeodaColumn *soporte = (struct GeodaColumn *)1;
    CHECK(geoda_apriori_itemsets(transaccion, item, 2, &conjunto, &items, &soporte) == GEODA_OK);
    CHECK(geoda_column_len(conjunto) == 9 && geoda_column_len(items) == 9);
    CHECK(geoda_column_len(soporte) == 6);
    geoda_column_free(conjunto);
    geoda_column_free(items);
    geoda_column_free(soporte);
    /* min_support = 0: INVALID_ARGUMENT y los tres `out` en NULL. */
    conjunto = items = soporte = (struct GeodaColumn *)1;
    CHECK(geoda_apriori_itemsets(transaccion, item, 0, &conjunto, &items, &soporte) ==
          GEODA_ERR_INVALID_ARGUMENT);
    CHECK(conjunto == NULL && items == NULL && soporte == NULL);
    CHECK(geoda_apriori_itemsets(NULL, item, 2, &conjunto, &items, &soporte) ==
          GEODA_ERR_INVALID_ARGUMENT);

    struct GeodaColumn *antecedente = (struct GeodaColumn *)1, *consecuente = (struct GeodaColumn *)1;
    struct GeodaColumn *sop = (struct GeodaColumn *)1, *confianza = (struct GeodaColumn *)1;
    struct GeodaColumn *lift = (struct GeodaColumn *)1;
    CHECK(geoda_apriori_rules(transaccion, item, 2, 0.6, &antecedente, &consecuente, &sop,
                              &confianza, &lift) == GEODA_OK);
    CHECK(geoda_column_len(antecedente) == 6 && geoda_column_len(lift) == 6);
    geoda_column_free(antecedente);
    geoda_column_free(consecuente);
    geoda_column_free(sop);
    geoda_column_free(confianza);
    geoda_column_free(lift);
    /* Confianza fuera de (0, 1]. */
    antecedente = consecuente = sop = confianza = lift = (struct GeodaColumn *)1;
    CHECK(geoda_apriori_rules(transaccion, item, 2, 1.5, &antecedente, &consecuente, &sop,
                              &confianza, &lift) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(antecedente == NULL && consecuente == NULL && sop == NULL && confianza == NULL &&
          lift == NULL);
    geoda_column_free(transaccion);
    geoda_column_free(item);
}

/* Algebra lineal: producto, solve, det e inversa; los errores dejan los `out` en NULL. */
static void prueba_algebra(void) {
    /* A = [[2,1],[1,3]] como columnas (2,1) y (1,3); b = (3,5); x = (0.8, 1.4). */
    const double c0[] = {2.0, 1.0}, c1[] = {1.0, 3.0}, rhs[] = {3.0, 5.0};
    struct GeodaColumn *col0 = columna_plana("g", c0, 2), *col1 = columna_plana("g", c1, 2);
    struct GeodaColumn *b = columna_plana("g", rhs, 2);
    const struct GeodaColumn *a[] = {col0, col1};
    struct GeodaColumn *out = (struct GeodaColumn *)1;
    CHECK(geoda_lu_solve(a, 2, b, &out) == GEODA_OK && geoda_column_len(out) == 2);
    geoda_column_free(out);
    double determinante = -1.0;
    CHECK(geoda_det(a, 2, &determinante) == GEODA_OK && determinante > 4.99 && determinante < 5.01);
    out = (struct GeodaColumn *)1;
    CHECK(geoda_inverse(a, 2, &out) == GEODA_OK && geoda_column_len(out) == 4);
    geoda_column_free(out);
    out = (struct GeodaColumn *)1;
    CHECK(geoda_matmul(a, 2, a, 2, &out) == GEODA_OK && geoda_column_len(out) == 4);
    geoda_column_free(out);

    /* Singular [[1,2],[2,4]]: INVALID_ARGUMENT, `out` en NULL; el determinante es 0 sin error. */
    const double s0[] = {1.0, 2.0}, s1[] = {2.0, 4.0};
    struct GeodaColumn *sing0 = columna_plana("g", s0, 2), *sing1 = columna_plana("g", s1, 2);
    const struct GeodaColumn *singular[] = {sing0, sing1};
    out = (struct GeodaColumn *)1;
    CHECK(geoda_inverse(singular, 2, &out) == GEODA_ERR_INVALID_ARGUMENT && out == NULL);
    CHECK(geoda_lu_solve(singular, 2, b, &out) == GEODA_ERR_INVALID_ARGUMENT && out == NULL);
    CHECK(geoda_det(singular, 2, &determinante) == GEODA_OK && determinante == 0.0);
    /* No cuadrada (2 filas, 1 columna) y punteros NULL. */
    const struct GeodaColumn *una[] = {col0};
    determinante = -1.0;
    CHECK(geoda_det(una, 1, &determinante) == GEODA_ERR_LENGTH_MISMATCH && determinante == -1.0);
    CHECK(geoda_det(NULL, 1, &determinante) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_det(a, 2, NULL) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_matmul(a, 2, NULL, 0, &out) == GEODA_ERR_INVALID_ARGUMENT && out == NULL);
    geoda_column_free(col0);
    geoda_column_free(col1);
    geoda_column_free(b);
    geoda_column_free(sing0);
    geoda_column_free(sing1);
}

/* QR, SVD y minimos cuadrados; los errores dejan los `out` en NULL. */
static void prueba_factorizaciones(void) {
    /* A (3x2) = [[1,0],[0,2],[0,0]] como columnas (1,0,0) y (0,2,0); b = (1,3,5). */
    const double c0[] = {1.0, 0.0, 0.0}, c1[] = {0.0, 2.0, 0.0}, rhs[] = {1.0, 2.0, 0.0};
    struct GeodaColumn *col0 = columna_plana("g", c0, 3), *col1 = columna_plana("g", c1, 3);
    struct GeodaColumn *b = columna_plana("g", rhs, 3);
    const struct GeodaColumn *a[] = {col0, col1};
    struct GeodaColumn *q = (struct GeodaColumn *)1, *r = (struct GeodaColumn *)1;
    CHECK(geoda_qr(a, 2, &q, &r) == GEODA_OK);
    CHECK(geoda_column_len(q) == 6 && geoda_column_len(r) == 4);
    geoda_column_free(q);
    geoda_column_free(r);
    struct GeodaColumn *u = (struct GeodaColumn *)1, *s = (struct GeodaColumn *)1;
    struct GeodaColumn *vt = (struct GeodaColumn *)1;
    CHECK(geoda_svd(a, 2, &u, &s, &vt) == GEODA_OK);
    CHECK(geoda_column_len(u) == 6 && geoda_column_len(s) == 2 && geoda_column_len(vt) == 4);
    geoda_column_free(u);
    geoda_column_free(s);
    geoda_column_free(vt);
    struct GeodaColumn *x = (struct GeodaColumn *)1;
    double residuo = -1.0;
    CHECK(geoda_lstsq(a, 2, b, &x, &residuo) == GEODA_OK && geoda_column_len(x) == 2);
    CHECK(residuo >= 0.0);
    geoda_column_free(x);

    /* Una matriz ancha (1 fila, 2 columnas) no tiene QR ni minimos cuadrados. */
    const double w0[] = {1.0}, w1[] = {2.0}, wb[] = {1.0};
    struct GeodaColumn *wide0 = columna_plana("g", w0, 1), *wide1 = columna_plana("g", w1, 1);
    struct GeodaColumn *wb_col = columna_plana("g", wb, 1);
    const struct GeodaColumn *wide[] = {wide0, wide1};
    q = r = (struct GeodaColumn *)1;
    CHECK(geoda_qr(wide, 2, &q, &r) == GEODA_ERR_INVALID_ARGUMENT && q == NULL && r == NULL);
    x = (struct GeodaColumn *)1;
    residuo = -1.0;
    CHECK(geoda_lstsq(wide, 2, wb_col, &x, &residuo) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(x == NULL && residuo == -1.0);
    /* La SVD si acepta la matriz ancha (forma delgada, k = 1). */
    CHECK(geoda_svd(wide, 2, &u, &s, &vt) == GEODA_OK && geoda_column_len(s) == 1);
    geoda_column_free(u);
    geoda_column_free(s);
    geoda_column_free(vt);
    u = s = vt = (struct GeodaColumn *)1;
    CHECK(geoda_svd(NULL, 2, &u, &s, &vt) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(u == NULL && s == NULL && vt == NULL);
    CHECK(geoda_lstsq(a, 2, b, &x, NULL) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(col0);
    geoda_column_free(col1);
    geoda_column_free(b);
    geoda_column_free(wide0);
    geoda_column_free(wide1);
    geoda_column_free(wb_col);
}

/* Regresion y PCA: ajustar, predecir/proyectar, leer los parametros y liberar. */
static void prueba_modelos(void) {
    const double xs[] = {0.0, 1.0, 2.0, 3.0}, ys[] = {1.0, 3.0, 5.0, 7.0};
    struct GeodaColumn *x = columna_plana("g", xs, 4), *y = columna_plana("g", ys, 4);
    const struct GeodaColumn *matriz[] = {x};
    struct GeodaRegression *regresion = (struct GeodaRegression *)1;
    CHECK(geoda_regression_fit(matriz, 1, y, true, &regresion) == GEODA_OK && regresion != NULL);
    struct GeodaColumn *prediccion = (struct GeodaColumn *)1, *coeficientes = (struct GeodaColumn *)1;
    CHECK(geoda_regression_predict(regresion, matriz, 1, &prediccion) == GEODA_OK);
    CHECK(geoda_column_len(prediccion) == 4);
    geoda_column_free(prediccion);
    double intercepto = -1.0, r2 = -1.0;
    CHECK(geoda_regression_params(regresion, &coeficientes, &intercepto, &r2) == GEODA_OK);
    CHECK(geoda_column_len(coeficientes) == 1 && intercepto > 0.99 && intercepto < 1.01);
    CHECK(r2 > 0.99);
    geoda_column_free(coeficientes);
    /* Otro numero de variables que el ajuste: LENGTH_MISMATCH y `out` en NULL. */
    const struct GeodaColumn *dos[] = {x, x};
    prediccion = (struct GeodaColumn *)1;
    CHECK(geoda_regression_predict(regresion, dos, 2, &prediccion) == GEODA_ERR_LENGTH_MISMATCH);
    CHECK(prediccion == NULL);
    CHECK(geoda_regression_predict(NULL, matriz, 1, &prediccion) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_regression_free(regresion);
    geoda_regression_free(NULL); /* acepta NULL */
    /* Variables colineales: INVALID_ARGUMENT y el modelo en NULL. */
    const struct GeodaColumn *colineales[] = {x, x};
    regresion = (struct GeodaRegression *)1;
    CHECK(geoda_regression_fit(colineales, 2, y, true, &regresion) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(regresion == NULL);

    struct GeodaPca *pca = (struct GeodaPca *)1;
    const double zs[] = {2.0, 4.0, 6.0, 8.0};
    struct GeodaColumn *z = columna_plana("g", zs, 4);
    const struct GeodaColumn *tabla[] = {x, z};
    CHECK(geoda_pca_fit(tabla, 2, 2, &pca) == GEODA_OK && pca != NULL);
    struct GeodaColumn *proyeccion = (struct GeodaColumn *)1;
    CHECK(geoda_pca_transform(pca, tabla, 2, &proyeccion) == GEODA_OK);
    CHECK(geoda_column_len(proyeccion) == 8);
    geoda_column_free(proyeccion);
    struct GeodaColumn *componentes = (struct GeodaColumn *)1, *varianza = (struct GeodaColumn *)1;
    struct GeodaColumn *razon = (struct GeodaColumn *)1, *media = (struct GeodaColumn *)1;
    CHECK(geoda_pca_params(pca, &componentes, &varianza, &razon, &media) == GEODA_OK);
    CHECK(geoda_column_len(componentes) == 4 && geoda_column_len(varianza) == 2);
    CHECK(geoda_column_len(razon) == 2 && geoda_column_len(media) == 2);
    geoda_column_free(componentes);
    geoda_column_free(varianza);
    geoda_column_free(razon);
    geoda_column_free(media);
    proyeccion = (struct GeodaColumn *)1;
    CHECK(geoda_pca_transform(pca, matriz, 1, &proyeccion) == GEODA_ERR_LENGTH_MISMATCH);
    CHECK(proyeccion == NULL);
    geoda_pca_free(pca);
    geoda_pca_free(NULL);
    /* n_components = 0 y fuera de rango: INVALID_ARGUMENT y el modelo en NULL. */
    pca = (struct GeodaPca *)1;
    CHECK(geoda_pca_fit(tabla, 2, 0, &pca) == GEODA_ERR_INVALID_ARGUMENT && pca == NULL);
    CHECK(geoda_pca_fit(tabla, 2, 3, &pca) == GEODA_ERR_INVALID_ARGUMENT);
    CHECK(geoda_pca_fit(NULL, 2, 1, &pca) == GEODA_ERR_INVALID_ARGUMENT);
    geoda_column_free(x);
    geoda_column_free(y);
    geoda_column_free(z);
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
    prueba_texto();
    prueba_cluster();
    prueba_arbol();
    prueba_apriori();
    prueba_algebra();
    prueba_factorizaciones();
    prueba_modelos();

    if (fallos == 0) {
        puts("smoke: ok");
        return 0;
    }
    fprintf(stderr, "smoke: %d fallos\n", fallos);
    return 1;
}
