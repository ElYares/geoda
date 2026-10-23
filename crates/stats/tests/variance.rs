//! Pruebas de varianza y desviacion estandar (HU-021).

use geoda_core::{BoolBuilder, Column, Error, PrimitiveBuilder, Utf8Builder};
use geoda_stats::{BLOCK, Variance, aggregate_with_workers, std_dev_column, variance_column};

fn ints(values: &[i64]) -> Column {
    let mut b = PrimitiveBuilder::<i64>::new();
    for &v in values {
        b.append(v);
    }
    b.finish().unwrap()
}

fn floats(values: &[f64]) -> Column {
    let mut b = PrimitiveBuilder::<f64>::new();
    for &v in values {
        b.append(v);
    }
    b.finish().unwrap()
}

/// Varianza en dos pasadas: la media primero y luego la suma de cuadrados. Es la
/// referencia contra la que se compara.
#[allow(clippy::cast_precision_loss)]
fn two_pass(values: &[f64], ddof: usize) -> f64 {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let m2: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
    m2 / (values.len() - ddof) as f64
}

fn close(a: f64, b: f64, relative: f64) -> bool {
    (a - b).abs() <= relative * b.abs().max(f64::MIN_POSITIVE)
}

#[test]
fn textbook_example_with_both_ddof() {
    let column = ints(&[2, 4, 4, 4, 5, 5, 7, 9]);
    assert!(close(variance_column(&column, 0).unwrap(), 4.0, 1e-15));
    assert!(close(
        variance_column(&column, 1).unwrap(),
        32.0 / 7.0,
        1e-15
    ));
    assert!(close(std_dev_column(&column, 0).unwrap(), 2.0, 1e-15));
    assert!(close(
        std_dev_column(&column, 1).unwrap(),
        (32.0_f64 / 7.0).sqrt(),
        1e-15
    ));
}

/// La formula `E[x²] - E[x]²` pierde casi todas las cifras con valores grandes y
/// cercanos; la de Welford no.
#[test]
fn large_close_values_are_computed_stably() {
    let values = [1e9 + 4.0, 1e9 + 7.0, 1e9 + 13.0, 1e9 + 16.0];
    let stable = variance_column(&floats(&values), 1).unwrap();
    assert!(close(stable, 30.0, 1e-12), "estable: {stable}");

    // Control: la formula ingenua falla en estos mismos datos.
    let n = 4.0;
    let mean_of_squares = values.iter().map(|v| v * v).sum::<f64>() / n;
    let mean = values.iter().sum::<f64>() / n;
    let naive = (mean_of_squares - mean * mean) * n / (n - 1.0);
    assert!(
        !close(naive, 30.0, 1e-6),
        "los datos no sirven de control: la formula ingenua dio {naive}"
    );
}

#[test]
fn blocks_match_a_single_pass_and_every_worker_count_is_bit_identical() {
    let values: Vec<f64> = (0..10_000_u32)
        .map(|i| 1e6 + f64::from(i % 997) * 0.37 + (f64::from(i) * 0.013).sin())
        .collect();
    let column = floats(&values);
    let variance = Variance::new(column.dtype(), 1).unwrap();

    let reference = aggregate_with_workers(&variance, &column, 1).unwrap();
    assert!(close(reference, two_pass(&values, 1), 1e-12));
    for workers in [2, 3, 7] {
        let got = aggregate_with_workers(&variance, &column, workers).unwrap();
        assert_eq!(got.to_bits(), reference.to_bits(), "{workers} trabajadores");
    }
}

#[test]
fn ddof_not_smaller_than_n_is_an_error() {
    assert!(matches!(
        variance_column(&ints(&[5]), 1),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        variance_column(&ints(&[1, 2, 3]), 3),
        Err(Error::InvalidArgument(_))
    ));
    // Un solo valor con ddof = 0 si esta definido: varianza 0.
    assert!(close(variance_column(&ints(&[5]), 0).unwrap(), 0.0, 1e-15));
}

#[test]
fn nulls_nan_and_empty_columns() {
    let mut b = PrimitiveBuilder::<f64>::new();
    for v in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
        b.append(v);
        b.append_null();
    }
    assert!(close(
        variance_column(&b.finish().unwrap(), 0).unwrap(),
        4.0,
        1e-15
    ));

    for position in [0, 1, BLOCK + 3] {
        let mut values = vec![1.0; BLOCK + 10];
        values[position] = f64::NAN;
        let column = floats(&values);
        assert!(
            variance_column(&column, 1).unwrap().is_nan(),
            "NaN en {position}"
        );
        assert!(std_dev_column(&column, 1).unwrap().is_nan());
    }

    let mut nulls = PrimitiveBuilder::<f64>::new();
    nulls.append_null();
    for column in [floats(&[]), nulls.finish().unwrap()] {
        assert!(matches!(
            variance_column(&column, 0),
            Err(Error::InvalidArgument(_))
        ));
    }
}

#[test]
fn bool_and_utf8_are_unsupported() {
    let mut bools = BoolBuilder::new();
    bools.append(true);
    let mut strings = Utf8Builder::new();
    strings.append("a").unwrap();
    for column in [bools.finish().unwrap(), strings.finish().unwrap()] {
        assert!(matches!(
            variance_column(&column, 1),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            std_dev_column(&column, 1),
            Err(Error::Unsupported(_))
        ));
    }
}

/// Generador splitmix64 con semilla fija: reproducible y sin dependencias.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn a_thousand_random_columns_match_the_two_pass_calculation() {
    let mut rng = Rng(23);
    for case in 0..1000 {
        let len = usize::try_from(rng.below(3 * BLOCK as u64)).unwrap();
        // Un desplazamiento grande exige estabilidad; los valores varian poco a su lado.
        #[allow(clippy::cast_precision_loss)]
        let offset = rng.below(1_000_000) as f64 * 1000.0;
        let mut b = PrimitiveBuilder::<f64>::new();
        let mut valid = Vec::new();
        for _ in 0..len {
            if rng.below(10) == 0 {
                b.append_null();
            } else {
                #[allow(clippy::cast_precision_loss)]
                let v = offset + (rng.below(2_000_000) as f64 - 1_000_000.0) / 7.0;
                b.append(v);
                valid.push(v);
            }
        }
        let column = b.finish().unwrap();
        for ddof in [0, 1] {
            if valid.len() <= ddof {
                continue;
            }
            let got = variance_column(&column, ddof).unwrap();
            let want = two_pass(&valid, ddof);
            assert!(
                close(got, want, 1e-12),
                "caso {case}, ddof {ddof}: {got} contra {want}"
            );
        }
    }
}
