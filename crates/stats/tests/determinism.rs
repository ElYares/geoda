//! Pruebas del marco de agregados con dos agregados de juguete: suma de enteros y de
//! flotantes.

use geoda_core::{BoolBuilder, Column, DataType, Error, PrimitiveBuilder, Utf8Builder};
use geoda_stats::{Aggregate, BLOCK, aggregate, aggregate_with_workers, for_each_valid};

struct SumInt;

impl Aggregate for SumInt {
    type State = i64;
    type Output = i64;

    fn init(&self) -> i64 {
        0
    }

    fn update(&self, state: &mut i64, block: &Column) -> Result<(), Error> {
        match block.dtype() {
            DataType::Int64 => for_each_valid::<i64>(block, |v| *state += v),
            dtype => Err(Error::Unsupported(format!("suma entera de {dtype:?}"))),
        }
    }

    fn merge(&self, left: i64, right: i64) -> i64 {
        left + right
    }

    fn finalize(&self, state: i64) -> Result<i64, Error> {
        Ok(state)
    }
}

struct SumFloat;

impl Aggregate for SumFloat {
    type State = f64;
    type Output = f64;

    fn init(&self) -> f64 {
        0.0
    }

    fn update(&self, state: &mut f64, block: &Column) -> Result<(), Error> {
        match block.dtype() {
            DataType::Float64 => for_each_valid::<f64>(block, |v| *state += v),
            dtype => Err(Error::Unsupported(format!("suma de {dtype:?}"))),
        }
    }

    fn merge(&self, left: f64, right: f64) -> f64 {
        left + right
    }

    fn finalize(&self, state: f64) -> Result<f64, Error> {
        Ok(state)
    }
}

fn int_column(n: i64) -> Column {
    let mut b = PrimitiveBuilder::<i64>::new();
    for i in 0..n {
        b.append(i * 3 - 7);
    }
    b.finish().unwrap()
}

/// Valores de magnitudes muy distintas: el orden de la suma cambia los ultimos bits.
fn float_column(n: usize) -> Column {
    let mut b = PrimitiveBuilder::<f64>::new();
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let x = i as f64;
        b.append((x * 0.1).sin() * 10f64.powi(i32::try_from(i % 13).unwrap() - 6));
    }
    b.finish().unwrap()
}

#[test]
fn integer_sum_by_blocks_equals_whole() {
    let n = 10_000;
    let expected: i64 = (0..n).map(|i| i * 3 - 7).sum();
    assert_eq!(aggregate(&SumInt, &int_column(n)).unwrap(), expected);
}

#[test]
fn float_sum_is_bit_identical_for_any_worker_count() {
    let column = float_column(10_000);
    let reference = aggregate(&SumFloat, &column).unwrap().to_bits();
    for workers in [1, 2, 3, 7] {
        let got = aggregate_with_workers(&SumFloat, &column, workers).unwrap();
        assert_eq!(got.to_bits(), reference, "con {workers} trabajadores");
    }
}

/// Control de la prueba anterior: los datos deben ser sensibles al orden de la suma. Si
/// la suma secuencial diera los mismos bits que el arbol, la prueba de arriba no
/// demostraria nada.
#[test]
fn float_data_is_sensitive_to_summation_order() {
    let column = float_column(10_000);
    let mut sequential = 0.0_f64;
    for_each_valid::<f64>(&column, |v| sequential += v).unwrap();
    let tree = aggregate(&SumFloat, &column).unwrap();
    assert_ne!(sequential.to_bits(), tree.to_bits());
}

#[test]
fn block_boundaries_do_not_lose_values() {
    for n in [0, 1, BLOCK - 1, BLOCK, BLOCK + 1, 3 * BLOCK, 3 * BLOCK + 5] {
        let n = i64::try_from(n).unwrap();
        let expected: i64 = (0..n).map(|i| i * 3 - 7).sum();
        for workers in [1, 2, 5] {
            let got = aggregate_with_workers(&SumInt, &int_column(n), workers).unwrap();
            assert_eq!(got, expected, "n = {n}, trabajadores = {workers}");
        }
    }
}

#[test]
fn empty_column_gives_the_initial_state() {
    assert_eq!(aggregate(&SumInt, &int_column(0)).unwrap(), 0);
}

#[test]
fn nulls_are_ignored() {
    let mut b = PrimitiveBuilder::<i64>::new();
    b.append(5);
    b.append_null();
    b.append(7);
    assert_eq!(aggregate(&SumInt, &b.finish().unwrap()).unwrap(), 12);
}

#[test]
fn unsupported_types_are_rejected() {
    let mut bools = BoolBuilder::new();
    bools.append(true);
    let mut strings = Utf8Builder::new();
    strings.append("a").unwrap();
    for column in [bools.finish().unwrap(), strings.finish().unwrap()] {
        assert!(matches!(
            aggregate(&SumInt, &column),
            Err(Error::Unsupported(_))
        ));
    }
}

#[test]
fn zero_workers_is_an_error() {
    assert!(matches!(
        aggregate_with_workers(&SumInt, &int_column(10), 0),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn quantiles_are_reexported() {
    use geoda_algo::Method;
    use geoda_stats::{median_column, percentile_column};
    let column = int_column(5);
    // Valores -7, -4, -1, 2, 5.
    let median = median_column(&column, Method::Quickselect).unwrap();
    let p0 = percentile_column(&column, 0.0, Method::Quickselect).unwrap();
    assert_eq!(median.to_bits(), (-1.0_f64).to_bits());
    assert_eq!(p0.to_bits(), (-7.0_f64).to_bits());
}
