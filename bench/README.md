# bench/

Benchmarks reproducibles de geoda: datos sinteticos con semilla fija y medidor propio
(`std::time::Instant`, sin `criterion`; decision 004 del vault).

```bash
cargo run --release -p geoda-bench              # n = 1 000 000
cargo run --release -p geoda-bench -- 200000    # otro tamano
```

Siempre `--release`: en debug los tiempos no significan nada. Cada celda es
`minimo / mediana` de 5 repeticiones, en milisegundos, sobre copias nuevas de la misma
entrada. Hoy compara cuatro formas de hallar la mediana de enteros:

| Metodo | Que es |
|---|---|
| mergesort propio | ordena todo con `geoda_algo::merge_sort` y toma el del medio |
| sort std (ref.) | lo mismo con `sort_unstable` de la biblioteca estandar |
| quickselect | `geoda_algo::quickselect`, sin ordenar todo |
| mediana de medianas | `geoda_algo::median_of_medians`, lineal en el peor caso |

Las entradas son: aleatoria, ya ordenada, orden inverso, pocos distintos y todo igual.
Los tiempos dependen de la maquina: sirven para comparar metodos entre si, no como
cifras absolutas.
