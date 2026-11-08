//! Benchmarks reproducibles de geoda.
//!
//! Los datos son sinteticos y salen de una semilla fija, asi que dos corridas ven
//! exactamente la misma entrada. El medidor es propio (`std::time::Instant`, decision
//! 004): sin dependencias, suficiente para comparar ordenes de magnitud.
//!
//! Uso (siempre en `--release`, en debug los tiempos no significan nada):
//!
//! ```text
//! cargo run --release -p geoda-bench              # n = 1 000 000
//! cargo run --release -p geoda-bench -- 200000    # otro tamano de la mediana
//! cargo run --release -p geoda-bench -- 1000000 5000000   # y otro de la varianza
//! cargo run --release -p geoda-bench -- file 250000000    # archivo de 2 GB leido por trozos
//! cargo run --release -p geoda-bench -- graph              # BFS y DFS de 1M nodos y 10M aristas
//! cargo run --release -p geoda-bench -- graph 1000000 10000000   # incluye Dijkstra
//! cargo run --release -p geoda-bench -- text              # edicion de 20 000 caracteres, LIS y Huffman de 1M
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use geoda_algo::{
    Graph, Huffman, WeightedGraph, bfs, dfs, dijkstra, edit_distance, lis, median_of_medians,
    merge_sort, quickselect,
};
use geoda_core::file::{ColumnFileReader, ColumnFileWriter};
use geoda_core::{Column, DataType, PrimitiveBuilder};
use geoda_stats::{Accumulator, Histogram, Mean, Variance, aggregate_with_workers};

/// Una forma de calcular la mediana de una copia de los datos.
type MedianFn = fn(&mut [i64]) -> i64;

/// Repeticiones por medicion; se reporta el minimo y la mediana.
const RUNS: usize = 5;
const SEED: u64 = 0x5EED_2026;
/// Filas de la columna de la varianza si no se pide otro tamano.
const STATS_ROWS: usize = 10_000_000;
/// Numeros de hilos que se comparan en la varianza.
const WORKER_COUNTS: [usize; 4] = [1, 2, 4, 8];
/// Filas por trozo al generar y al leer el archivo grande.
const FILE_CHUNK_ROWS: usize = 64 * 1024;

/// Generador pseudoaleatorio con semilla (xorshift64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Las formas de entrada que se miden: la aleatoria y las que suelen dañar a un
/// algoritmo ingenuo.
const PATTERNS: [&str; 5] = [
    "aleatoria",
    "ya ordenada",
    "orden inverso",
    "pocos distintos",
    "todo igual",
];

/// `n` enteros con la forma `pattern`, siempre los mismos para la misma semilla.
fn generate(pattern: &str, n: usize) -> Vec<i64> {
    let mut rng = Rng(SEED);
    let n_i64 = i64::try_from(n).expect("n cabe en i64");
    match pattern {
        "aleatoria" => (0..n)
            .map(|_| i64::try_from(rng.next() % 1_000_000_000).expect("cabe"))
            .collect(),
        "ya ordenada" => (0..n_i64).collect(),
        "orden inverso" => (0..n_i64).rev().collect(),
        "pocos distintos" => (0..n)
            .map(|_| i64::try_from(rng.next() % 4).expect("cabe"))
            .collect(),
        "todo igual" => vec![42; n],
        other => panic!("patron desconocido: {other}"),
    }
}

/// Tiempos de `RUNS` ejecuciones de `work` sobre copias nuevas de `data` (la copia
/// no se mide). Devuelve `(minimo, mediana)`.
fn measure(data: &[i64], work: impl Fn(&mut [i64]) -> i64) -> (Duration, Duration) {
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let mut copy = data.to_vec();
            let start = Instant::now();
            black_box(work(black_box(&mut copy)));
            start.elapsed()
        })
        .collect();
    times.sort_unstable();
    (times[0], times[RUNS / 2])
}

/// Mediana por ordenacion completa con el mergesort propio.
fn median_by_merge_sort(data: &mut [i64]) -> i64 {
    merge_sort(data, i64::cmp);
    data[data.len() / 2]
}

/// Mediana por ordenacion completa con la de la biblioteca estandar (referencia).
fn median_by_std_sort(data: &mut [i64]) -> i64 {
    data.sort_unstable();
    data[data.len() / 2]
}

fn median_by_quickselect(data: &mut [i64]) -> i64 {
    let k = data.len() / 2;
    quickselect(data, k, i64::cmp).expect("k valido")
}

fn median_by_median_of_medians(data: &mut [i64]) -> i64 {
    let k = data.len() / 2;
    median_of_medians(data, k, i64::cmp).expect("k valido")
}

/// Columna `Float64` de `n` valores con la semilla fija.
fn float_column(n: usize) -> Column {
    let mut rng = Rng(SEED);
    let mut builder = PrimitiveBuilder::<f64>::new();
    for _ in 0..n {
        #[allow(clippy::cast_precision_loss)]
        builder.append((rng.next() % 1_000_000_000) as f64 / 1000.0);
    }
    builder.finish().expect("la columna se construye")
}

/// Varianza de una columna con 1, 2, 4 y 8 hilos: tiempos y aceleracion contra 1 hilo.
/// Los resultados deben coincidir bit a bit; si no, el bench aborta.
fn bench_variance(n: usize) {
    let column = float_column(n);
    let variance = Variance::new(DataType::Float64, 1).expect("Float64 es soportado");
    println!(
        "\nvarianza de n = {n} Float64, {RUNS} repeticiones, tiempos en ms (minimo / mediana)\n"
    );
    println!("{:<10}{:>22}{:>14}", "hilos", "tiempo", "aceleracion");
    let mut reference: Option<(u64, Duration)> = None;
    for workers in WORKER_COUNTS {
        let mut result = 0_u64;
        let mut times: Vec<Duration> = (0..RUNS)
            .map(|_| {
                let start = Instant::now();
                let value = aggregate_with_workers(&variance, black_box(&column), workers)
                    .expect("la varianza se calcula");
                let elapsed = start.elapsed();
                result = value.to_bits();
                elapsed
            })
            .collect();
        times.sort_unstable();
        let (min, median) = (times[0], times[RUNS / 2]);
        let (bits, base) = *reference.get_or_insert((result, min));
        assert_eq!(bits, result, "el resultado cambio con {workers} hilos");
        let speedup = base.as_secs_f64() / min.as_secs_f64();
        println!(
            "{workers:<10}{:>22}{:>13.2}x",
            format!("{} / {}", ms(min), ms(median)),
            speedup
        );
    }
}

/// Pico de memoria residente del proceso en MiB (`VmHWM`), solo en Linux.
fn peak_resident_mib() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kib: f64 = status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))?
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    Some(kib / 1024.0)
}

/// Genera un archivo de `rows` filas `Float64`, lo lee por trozos calculando media,
/// varianza e histograma en una sola pasada y muestra filas por segundo y memoria pico.
fn bench_file(rows: u64, path: &std::path::Path) {
    let mut rng = Rng(SEED);
    let started = Instant::now();
    let mut writer =
        ColumnFileWriter::create(path, DataType::Float64, rows, false).expect("se crea el archivo");
    let mut left = rows;
    while left > 0 {
        let n = usize::try_from(left.min(FILE_CHUNK_ROWS as u64)).expect("cabe");
        let mut builder = PrimitiveBuilder::<f64>::new();
        for _ in 0..n {
            #[allow(clippy::cast_precision_loss)]
            builder.append((rng.next() % 1_000_000_000) as f64 / 1000.0);
        }
        writer
            .append(&builder.finish().expect("trozo"))
            .expect("se escribe");
        left -= n as u64;
    }
    writer.finish().expect("se cierra");
    println!(
        "\narchivo de {rows} filas escrito en {:.1} s ({} MiB)",
        started.elapsed().as_secs_f64(),
        std::fs::metadata(path).expect("existe").len() / (1024 * 1024)
    );

    let dtype = DataType::Float64;
    let mut mean = Accumulator::new(Mean::new(dtype).expect("Float64"));
    let mut variance = Accumulator::new(Variance::new(dtype, 1).expect("Float64"));
    let mut histogram =
        Accumulator::new(Histogram::new(dtype, 10, 0.0, 1_000_000.0).expect("Float64"));
    let started = Instant::now();
    for chunk in ColumnFileReader::open(path, FILE_CHUNK_ROWS).expect("se abre") {
        let chunk = chunk.expect("se lee");
        mean.push(&chunk).expect("media");
        variance.push(&chunk).expect("varianza");
        histogram.push(&chunk).expect("histograma");
    }
    let (mean, variance) = (
        mean.finish().expect("media"),
        variance.finish().expect("varianza"),
    );
    let histogram = histogram.finish().expect("histograma");
    let elapsed = started.elapsed().as_secs_f64();
    #[allow(clippy::cast_precision_loss)]
    let per_second = rows as f64 / elapsed;
    println!(
        "lectura por trozos de {FILE_CHUNK_ROWS} filas: {elapsed:.1} s, {:.1} M filas/s",
        per_second / 1e6
    );
    println!(
        "media = {mean:.3}, varianza = {variance:.3}, cubos = {:?}",
        histogram.counts
    );
    match peak_resident_mib() {
        Some(mib) => println!("pico de memoria residente: {mib:.0} MiB"),
        None => println!("pico de memoria residente: no disponible (solo Linux)"),
    }
}

/// Grafo aleatorio dirigido de `nodes` nodos y `edges` aristas con la semilla fija: se
/// construye el CSR y se miden BFS y DFS desde el nodo 0.
fn bench_graph(nodes: usize, edges: usize) {
    let mut rng = Rng(SEED);
    let list: Vec<(usize, usize)> = (0..edges)
        .map(|_| {
            let from = usize::try_from(rng.next() % nodes as u64).expect("cabe");
            let to = usize::try_from(rng.next() % nodes as u64).expect("cabe");
            (from, to)
        })
        .collect();
    let started = Instant::now();
    let graph = Graph::from_edges(nodes, &list, true).expect("el grafo se construye");
    println!(
        "\ngrafo aleatorio de {nodes} nodos y {edges} aristas: CSR construido en {} ms",
        ms(started.elapsed())
    );
    println!("{:<10}{:>22}{:>14}", "recorrido", "tiempo", "visitados");
    for (name, run) in [
        (
            "bfs",
            (|g: &Graph| bfs(g, 0).expect("origen valido").order.len()) as fn(&Graph) -> usize,
        ),
        ("dfs", |g: &Graph| {
            dfs(g, 0).expect("origen valido").entry_order.len()
        }),
    ] {
        let mut visited = 0;
        let mut times: Vec<Duration> = (0..RUNS)
            .map(|_| {
                let start = Instant::now();
                visited = black_box(run(black_box(&graph)));
                start.elapsed()
            })
            .collect();
        times.sort_unstable();
        println!(
            "{name:<10}{:>22}{visited:>14}",
            format!("{} / {}", ms(times[0]), ms(times[RUNS / 2]))
        );
    }

    // Dijkstra con pesos enteros de 1 a 100 sobre las mismas aristas.
    let weighted: Vec<(usize, usize, f64)> = list
        .iter()
        .map(|&(from, to)| {
            (
                from,
                to,
                f64::from(u32::try_from(rng.next() % 100 + 1).expect("cabe")),
            )
        })
        .collect();
    let weighted =
        WeightedGraph::from_edges(nodes, &weighted, true).expect("el grafo se construye");
    let mut reached = 0;
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            let paths = dijkstra(black_box(&weighted), 0).expect("origen y pesos validos");
            reached = black_box(paths.distance.iter().filter(|d| d.is_finite()).count());
            start.elapsed()
        })
        .collect();
    times.sort_unstable();
    println!(
        "{:<10}{:>22}{reached:>14}",
        "dijkstra",
        format!("{} / {}", ms(times[0]), ms(times[RUNS / 2]))
    );
}

/// Cadena aleatoria de `len` letras minusculas con la semilla fija.
fn random_text(rng: &mut Rng, len: usize) -> String {
    (0..len)
        .map(|_| char::from(b'a' + u8::try_from(rng.next() % 26).expect("cabe")))
        .collect()
}

/// Distancia de edicion de dos cadenas de `len` caracteres: tiempo y memoria residente
/// (dos filas, no la tabla de `len * len` celdas).
fn bench_text(len: usize) {
    let mut rng = Rng(SEED);
    let (a, b) = (random_text(&mut rng, len), random_text(&mut rng, len));
    let started = Instant::now();
    let distance = black_box(edit_distance(black_box(&a), black_box(&b)));
    println!(
        "\ndistancia de edicion de dos cadenas de {len} caracteres: {distance} en {} ms",
        ms(started.elapsed())
    );
    match peak_resident_mib() {
        Some(mib) => println!("pico de memoria residente: {mib:.1} MiB"),
        None => println!("pico de memoria residente: no disponible (solo Linux)"),
    }

    let n = 1_000_000;
    let data: Vec<i64> = (0..n)
        .map(|_| i64::try_from(rng.next() >> 2).expect("cabe"))
        .collect();
    let started = Instant::now();
    let result = lis(black_box(&data)).expect("sin NaN");
    println!(
        "LIS de {n} enteros aleatorios: longitud {} en {} ms",
        result.length,
        ms(started.elapsed())
    );

    // Huffman sobre 10 MB de letras (26 simbolos, frecuencias casi parejas).
    let text = random_text(&mut rng, 10_000_000).into_bytes();
    let started = Instant::now();
    let huffman = Huffman::from_data(black_box(&text)).expect("codigo valido");
    let encoded = huffman
        .encode(black_box(&text))
        .expect("todos tienen codigo");
    let encode_time = started.elapsed();
    let started = Instant::now();
    let decoded = huffman
        .decode(&encoded.bytes, encoded.bit_len)
        .expect("bits validos");
    assert_eq!(decoded, text);
    println!(
        "Huffman de {} bytes: {} bits ({:.1}% del original); codifica en {} ms, decodifica en {} ms",
        text.len(),
        encoded.bit_len,
        f64::from(u32::try_from(encoded.bit_len / 1000).expect("cabe"))
            / f64::from(u32::try_from(text.len() * 8 / 1000).expect("cabe"))
            * 100.0,
        ms(encode_time),
        ms(started.elapsed())
    );
}

fn ms(d: Duration) -> String {
    format!("{:.2}", d.as_secs_f64() * 1000.0)
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("text") {
        let len: usize = std::env::args().nth(2).map_or(20_000, |a| {
            a.parse().expect("la longitud debe ser un entero")
        });
        bench_text(len);
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("graph") {
        let nodes: usize = std::env::args().nth(2).map_or(1_000_000, |a| {
            a.parse().expect("los nodos deben ser un entero")
        });
        let edges: usize = std::env::args().nth(3).map_or(10_000_000, |a| {
            a.parse().expect("las aristas deben ser un entero")
        });
        bench_graph(nodes, edges);
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("file") {
        let rows: u64 = std::env::args().nth(2).map_or(250_000_000, |a| {
            a.parse().expect("las filas deben ser un entero")
        });
        let path = std::path::PathBuf::from(
            std::env::args()
                .nth(3)
                .unwrap_or_else(|| "target/bench-columna.geoda".to_string()),
        );
        bench_file(rows, &path);
        let _ = std::fs::remove_file(&path);
        return;
    }
    let n: usize = std::env::args()
        .nth(1)
        .map_or(1_000_000, |a| a.parse().expect("n debe ser un entero"));
    println!("mediana de n = {n} enteros, {RUNS} repeticiones, tiempos en ms (minimo / mediana)\n");

    let methods: [(&str, MedianFn); 4] = [
        ("mergesort propio", median_by_merge_sort),
        ("sort std (ref.)", median_by_std_sort),
        ("quickselect", median_by_quickselect),
        ("mediana de medianas", median_by_median_of_medians),
    ];

    print!("{:<16}", "entrada");
    for (name, _) in &methods {
        print!("{name:>26}");
    }
    println!();
    for pattern in PATTERNS {
        let data = generate(pattern, n);
        print!("{pattern:<16}");
        for (_, work) in &methods {
            let (min, median) = measure(&data, work);
            print!("{:>26}", format!("{} / {}", ms(min), ms(median)));
        }
        println!();
    }
    bench_variance(
        std::env::args()
            .nth(2)
            .map_or(STATS_ROWS, |a| a.parse().expect("n debe ser un entero")),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_is_deterministic() {
        for pattern in PATTERNS {
            assert_eq!(
                generate(pattern, 1000),
                generate(pattern, 1000),
                "{pattern}"
            );
        }
    }

    #[test]
    fn generator_has_the_requested_size_and_shape() {
        for pattern in PATTERNS {
            assert_eq!(generate(pattern, 777).len(), 777, "{pattern}");
        }
        let sorted = generate("ya ordenada", 100);
        assert!(sorted.windows(2).all(|w| w[0] < w[1]));
        let reversed = generate("orden inverso", 100);
        assert!(reversed.windows(2).all(|w| w[0] > w[1]));
        assert!(
            generate("pocos distintos", 1000)
                .iter()
                .all(|&v| (0..4).contains(&v))
        );
        assert!(generate("todo igual", 10).iter().all(|&v| v == 42));
    }

    #[test]
    fn all_four_methods_agree_on_the_median() {
        for pattern in PATTERNS {
            let data = generate(pattern, 5001);
            let mut expected = data.clone();
            expected.sort_unstable();
            let expected = expected[data.len() / 2];
            for (name, work) in [
                ("mergesort", median_by_merge_sort as MedianFn),
                ("std", median_by_std_sort),
                ("quickselect", median_by_quickselect),
                ("mediana de medianas", median_by_median_of_medians),
            ] {
                assert_eq!(work(&mut data.clone()), expected, "{name} sobre {pattern}");
            }
        }
    }
}
