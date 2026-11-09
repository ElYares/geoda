# geoda para Java

Binding sobre la C-API (ABI 1.x, [`docs/abi.md`](../../docs/abi.md)) con la **API de memoria externa
(FFM)** de Java: JDK 22 o mayor, sin JNI y sin `jextract` (los enlaces están escritos a mano en
`Natives.java`).

```java
try (Column column = Column.of(new double[] {1.5, 99.0, 3.5}, new boolean[] {true, false, true})) {
    double mean = Geoda.mean(column);                                   // 2.5
}
double[] x = Geoda.solve(new double[][] {{2, 1}, {1, 3}}, new double[] {3, 5});   // {0.8, 1.4}
KMeansResult fit = Geoda.kmeans(new double[][] {{0, 0, 10, 10}, {0, 1, 10, 11}}, 2, 1);
try { Geoda.median(Column.of(new double[0])); }
catch (GeodaException e) { /* e.category().equals("INVALID_ARGUMENT") */ }
```

## Construir y probar

```bash
cargo build -p geoda-capi
bindings/java/test.sh        # compila, arma build/geoda.jar y corre las pruebas
bindings/java/build.sh       # solo el jar
```

La biblioteca se busca en la propiedad `-Dgeoda.lib=<ruta a libgeoda.so o su carpeta>`, en la variable
`GEODA_LIB` o, si no, en el `java.library.path`. Hay que arrancar la JVM con
`--enable-native-access=ALL-UNNAMED` (si no, el JDK avisa de cada acceso nativo).

## Superficie (decisiones 015 O4 y 016 P1)

| | |
|---|---|
| `Column` | `of(double[] / int[] / long[], valid)`, `ofBooleans`, `ofStrings`; `size`, `nullCount`, `dtype`, `validity`, `toDoubleArray`, `toIntArray`, `toLongArray`, `toBooleanArray`, `toStringArray`, `close` (idempotente) |
| Estadísticos | `Geoda.count`, `sum`, `min`, `max` (`Long` si la columna es entera, `Double` si no), `mean`, `var`, `std`, `median`, `percentile`, `corr` |
| Texto | `Geoda.editDistance` (dos cadenas o dos columnas) |
| Álgebra lineal | `matmul`, `solve`, `det`, `inverse`, `qr`, `svd`, `lstsq` sobre `double[][]` **por filas** (`QrResult`, `SvdResult`, `LstsqResult`) |
| Series, texto y estadística | `lis`, `dedupe` (`DedupeResult`), `histogram` (`HistogramResult`) |
| Minería | `kmeans(variables, k, seed)` con las variables como **columnas** |
| Errores | `GeodaException` con `code()`, `category()` y el mensaje de la capi |

Quedan solo en Python (nivel 3, decisión 016 P2): los modelos con estado (regresión, PCA, árbol), los grafos, Huffman, Apriori y `group_by`.

Una `Column` usada después de `close()` lanza `IllegalStateException`. `workers` no se expone: siempre es 1.
