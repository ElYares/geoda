# Registro de cambios

Todos los cambios visibles de geoda se anotan aquí. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y las versiones siguen
[SemVer](https://semver.org/lang/es/) con las reglas de [`docs/versiones.md`](docs/versiones.md).

## [Sin publicar]

## [0.1.0] - sin fecha (aún no publicada)

Primera versión: el plan de las fases 0 a 8. La ABI de la C-API queda en **1.0** y congelada
([`docs/abi.md`](docs/abi.md)).

### Añadido

- **Capa columnar propia** (`geoda-core`): `Column`, builders y `Chunk` con invariantes validados; importación
  y exportación por la Arrow C Data Interface (el import siempre copia).
- **Algoritmos base** (`geoda-algo`): mergesort propio, selección (`quickselect`, mediana de medianas),
  percentiles tipo 7, búsqueda binaria, montículo y `top_k`, tabla hash y conteo de valores distintos.
- **Estadística** (`geoda-stats`): count, suma, media, mínimo, máximo, varianza, desviación, covarianza,
  correlación de Pearson, histograma y agregados por grupo; todo en hilos reales (`workers` explícito),
  por trozos (`Accumulator`) y sobre archivos de columna mayores que la RAM.
- **Grafos**: CSR, BFS y DFS, componentes, orden topológico, Dijkstra y árbol de expansión mínimo (Kruskal).
- **Texto y similitud**: distancia de edición, subsecuencia creciente más larga, Huffman y deduplicación
  por similitud.
- **Minería y aprendizaje**: k-means (k-means++), clustering jerárquico de enlace simple, árbol de decisión
  CART y reglas de asociación (Apriori).
- **Álgebra lineal**: producto, LU, `solve`, `det`, inversa, QR, SVD, mínimos cuadrados, regresión lineal y PCA.
- **C-API** (`libgeoda`): 75 funciones `geoda_*`, handles opacos, errores por código y accesores por
  arreglos planos. La lista exacta está en `abi/symbols.txt` y CI la vigila.
- **Bindings**: Python (wheel con PyO3), C++17 de solo cabeceras, Java (API de memoria externa, JDK 22+),
  Node (N-API) y WASM (sin hilos).
- **Spec compartida** (`spec/`): casos que ejecutan los runners de Rust, Python y JavaScript y deben coincidir.
- **Empaquetado sin publicar** (`release/build.sh`, `RELEASING.md`): los paquetes se construyen y verifican;
  publicarlos es un paso manual.
- Documentos de comunidad (`CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`) y plantillas de GitHub.

### Limitaciones conocidas

- Solo Linux está probado; el wheel de Python sale etiquetado `manylinux_2_34`.
- WASM no usa hilos (`workers > 1` da `UNSUPPORTED`) y el paquete de Node no incluye `libgeoda`.
- Clustering jerárquico: solo enlace simple y hasta 5000 filas.
- El `unsafe` lo revisó un agente (`docs/` del vault); falta lectura humana y fuzzing.

[Sin publicar]: https://github.com/ElYares/geoda/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ElYares/geoda/commits/main
