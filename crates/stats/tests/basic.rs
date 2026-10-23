//! Pruebas de count, suma, media, minimo y maximo (HU-020).

use geoda_core::{BoolBuilder, Column, Error, PrimitiveBuilder, Utf8Builder};
use geoda_stats::{BLOCK, Number, count_column, max_column, mean_column, min_column, sum_column};

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

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
}

#[test]
fn basic_statistics_of_a_small_int64_column() {
    let column = ints(&[1, 2, 3, 4]);
    assert_eq!(count_column(&column).unwrap(), 4);
    assert_eq!(sum_column(&column).unwrap(), Number::Int(10));
    assert!(same(mean_column(&column).unwrap(), 2.5));
    assert_eq!(min_column(&column).unwrap(), Number::Int(1));
    assert_eq!(max_column(&column).unwrap(), Number::Int(4));
}

#[test]
fn int32_columns_give_int_results() {
    let mut b = PrimitiveBuilder::<i32>::new();
    for v in [5, -3, 9] {
        b.append(v);
    }
    let column = b.finish().unwrap();
    assert_eq!(sum_column(&column).unwrap(), Number::Int(11));
    assert_eq!(min_column(&column).unwrap(), Number::Int(-3));
    assert_eq!(max_column(&column).unwrap(), Number::Int(9));
}

#[test]
fn int_sum_that_does_not_fit_is_an_error() {
    let column = ints(&[i64::MAX, 1]);
    assert!(matches!(sum_column(&column), Err(Error::OutOfBounds(_))));
}

#[test]
fn int_sum_only_needs_the_final_result_to_fit() {
    assert_eq!(
        sum_column(&ints(&[i64::MAX, -i64::MAX])).unwrap(),
        Number::Int(0)
    );
    // El parcial de los primeros bloques desborda i64 y aun asi el total es 0: ejercita
    // la fusion de estados `i128`.
    let mut values = vec![i64::MAX; 2 * BLOCK];
    values.extend(vec![-i64::MAX; 2 * BLOCK]);
    assert_eq!(sum_column(&ints(&values)).unwrap(), Number::Int(0));
}

/// `[1e16, 1.0 x n, -1e16]`: sumada en orden, cada `1.0` se pierde contra `1e16` (el
/// paso entre flotantes ahi es 2) y da 0. Por pares, los `1.0` se suman entre si antes.
/// Con `n = 500` todo cabe en un bloque y solo la suma por pares *dentro* del bloque
/// puede mejorar; con `n = 5000` tambien interviene el arbol de fusion entre bloques.
#[test]
fn pairwise_float_sum_is_more_exact_than_the_naive_one() {
    for n in [500, 5_000] {
        let mut values = vec![1e16];
        values.extend(vec![1.0; n]);
        values.push(-1e16);

        let naive: f64 = values.iter().sum();
        let Number::Float(pairwise) = sum_column(&floats(&values)).unwrap() else {
            panic!("la suma de flotantes debe ser Float");
        };
        #[allow(clippy::cast_precision_loss)]
        let exact = n as f64;
        assert!(
            (pairwise - exact).abs() < (naive - exact).abs(),
            "n = {n}: por pares {pairwise}, ingenua {naive}, exacta {exact}"
        );
    }
}

#[test]
fn nulls_are_ignored_and_count_only_valid_values() {
    let mut b = PrimitiveBuilder::<i64>::new();
    b.append(10);
    b.append_null();
    b.append(30);
    b.append_null();
    let column = b.finish().unwrap();
    assert_eq!(count_column(&column).unwrap(), 2);
    assert_eq!(sum_column(&column).unwrap(), Number::Int(40));
    assert!(same(mean_column(&column).unwrap(), 20.0));
    assert_eq!(min_column(&column).unwrap(), Number::Int(10));
    assert_eq!(max_column(&column).unwrap(), Number::Int(30));
}

#[test]
fn nan_propagates_wherever_it_is() {
    for position in [0, 1, 2, BLOCK + 3] {
        let mut values = vec![1.0; BLOCK + 10];
        values[position] = f64::NAN;
        let column = floats(&values);
        let Number::Float(sum) = sum_column(&column).unwrap() else {
            panic!("Float esperado");
        };
        assert!(sum.is_nan(), "sum con NaN en {position}");
        assert!(mean_column(&column).unwrap().is_nan());
        assert!(matches!(min_column(&column).unwrap(), Number::Float(v) if v.is_nan()));
        assert!(matches!(max_column(&column).unwrap(), Number::Float(v) if v.is_nan()));
    }
}

#[test]
fn empty_and_all_null_columns_follow_s6() {
    let mut nulls = PrimitiveBuilder::<f64>::new();
    nulls.append_null();
    nulls.append_null();
    for column in [floats(&[]), nulls.finish().unwrap()] {
        assert_eq!(count_column(&column).unwrap(), 0);
        assert_eq!(sum_column(&column).unwrap(), Number::Float(0.0));
        assert!(matches!(
            mean_column(&column),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            min_column(&column),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            max_column(&column),
            Err(Error::InvalidArgument(_))
        ));
    }
    assert_eq!(sum_column(&ints(&[])).unwrap(), Number::Int(0));
}

#[test]
fn bool_and_utf8_are_unsupported() {
    let mut bools = BoolBuilder::new();
    bools.append(true);
    let mut strings = Utf8Builder::new();
    strings.append("a").unwrap();
    for column in [bools.finish().unwrap(), strings.finish().unwrap()] {
        assert!(matches!(count_column(&column), Err(Error::Unsupported(_))));
        assert!(matches!(sum_column(&column), Err(Error::Unsupported(_))));
        assert!(matches!(mean_column(&column), Err(Error::Unsupported(_))));
        assert!(matches!(min_column(&column), Err(Error::Unsupported(_))));
        assert!(matches!(max_column(&column), Err(Error::Unsupported(_))));
    }
}

// ------------------------------------------------ contra un calculo ingenuo

/// Generador de numeros pseudoaleatorios (splitmix64) con semilla fija: reproducible y
/// sin dependencias.
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
fn a_thousand_random_int_columns_match_the_naive_i128_calculation() {
    let mut rng = Rng(7);
    for case in 0..1000 {
        let len = usize::try_from(rng.below(3 * BLOCK as u64)).unwrap();
        let mut b = PrimitiveBuilder::<i64>::new();
        let mut valid: Vec<i64> = Vec::new();
        for _ in 0..len {
            if rng.below(10) == 0 {
                b.append_null();
            } else {
                let v = i64::try_from(rng.below(2_000_000_000_000)).unwrap() - 1_000_000_000_000;
                b.append(v);
                valid.push(v);
            }
        }
        let column = b.finish().unwrap();
        assert_eq!(count_column(&column).unwrap(), valid.len(), "caso {case}");
        let total: i128 = valid.iter().map(|&v| i128::from(v)).sum();
        assert_eq!(
            sum_column(&column).unwrap(),
            Number::Int(i64::try_from(total).unwrap()),
            "caso {case}"
        );
        if valid.is_empty() {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let mean = total as f64 / valid.len() as f64;
        assert!(same(mean_column(&column).unwrap(), mean), "caso {case}");
        assert_eq!(
            min_column(&column).unwrap(),
            Number::Int(*valid.iter().min().unwrap())
        );
        assert_eq!(
            max_column(&column).unwrap(),
            Number::Int(*valid.iter().max().unwrap())
        );
    }
}

#[test]
fn a_thousand_random_float_columns_match_the_naive_calculation() {
    let mut rng = Rng(11);
    for case in 0..1000 {
        let len = usize::try_from(rng.below(3 * BLOCK as u64)).unwrap();
        let mut b = PrimitiveBuilder::<f64>::new();
        let mut valid: Vec<f64> = Vec::new();
        for _ in 0..len {
            if rng.below(10) == 0 {
                b.append_null();
            } else {
                #[allow(clippy::cast_precision_loss)]
                let v = (rng.below(2_000_000) as f64 - 1_000_000.0) / 7.0;
                b.append(v);
                valid.push(v);
            }
        }
        let column = b.finish().unwrap();
        if valid.is_empty() {
            continue;
        }
        // Referencia: suma ordenada por magnitud creciente. La tolerancia es relativa a la
        // suma de los valores absolutos, porque la suma con signo puede cancelarse.
        let mut ordered = valid.clone();
        ordered.sort_by(|a, b| a.abs().total_cmp(&b.abs()));
        let reference: f64 = ordered.iter().sum();
        let scale: f64 = valid.iter().map(|v| v.abs()).sum();
        let Number::Float(sum) = sum_column(&column).unwrap() else {
            panic!("Float esperado");
        };
        assert!(
            (sum - reference).abs() <= 1e-12 * scale,
            "suma, caso {case}"
        );
        #[allow(clippy::cast_precision_loss)]
        let mean_reference = reference / valid.len() as f64;
        #[allow(clippy::cast_precision_loss)]
        let mean_scale = scale / valid.len() as f64;
        assert!(
            (mean_column(&column).unwrap() - mean_reference).abs() <= 1e-12 * mean_scale,
            "media, caso {case}"
        );
        let low = valid.iter().copied().fold(f64::INFINITY, f64::min);
        let high = valid.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(min_column(&column).unwrap(), Number::Float(low));
        assert_eq!(max_column(&column).unwrap(), Number::Float(high));
    }
}
