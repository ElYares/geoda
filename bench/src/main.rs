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
//! cargo run --release -p geoda-bench -- 200000    # otro tamano
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use geoda_algo::{median_of_medians, merge_sort, quickselect};

/// Una forma de calcular la mediana de una copia de los datos.
type MedianFn = fn(&mut [i64]) -> i64;

/// Repeticiones por medicion; se reporta el minimo y la mediana.
const RUNS: usize = 5;
const SEED: u64 = 0x5EED_2026;

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

fn ms(d: Duration) -> String {
    format!("{:.2}", d.as_secs_f64() * 1000.0)
}

fn main() {
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
