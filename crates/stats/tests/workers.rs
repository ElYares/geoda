//! HU-030: las variantes `_with_workers` dan los mismos bits que con un hilo y rechazan 0.

use geoda_core::{Column, Error, PrimitiveBuilder};
use geoda_stats::{
    GroupOp, correlation_columns_with_workers, group_by_with_workers,
    histogram_column_with_workers, max_column_with_workers, mean_column_with_workers,
    min_column_with_workers, std_dev_column_with_workers, sum_column_with_workers,
    variance_column_with_workers,
};

fn float_column(n: usize, shift: f64) -> Column {
    let mut b = PrimitiveBuilder::<f64>::new();
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let x = i as f64;
        b.append((x * 0.1 + shift).sin() * 10f64.powi(i32::try_from(i % 13).unwrap() - 6));
    }
    b.finish().unwrap()
}

fn keys(n: usize) -> Column {
    let mut b = PrimitiveBuilder::<i64>::new();
    for i in 0..n {
        b.append(i64::try_from(i % 3).unwrap());
    }
    b.finish().unwrap()
}

/// Resultado como texto: `Debug` de un `f64` ida y vuelta distingue los bits que importan.
fn same_for_every_worker_count(compute: impl Fn(usize) -> String) {
    let reference = compute(1);
    for workers in [2, 3, 4, 8] {
        assert_eq!(compute(workers), reference, "{workers} hilos");
    }
}

#[test]
fn every_statistic_is_identical_with_any_worker_count() {
    let x = float_column(10_000, 0.0);
    let y = float_column(10_000, 1.0);
    same_for_every_worker_count(|w| format!("{:?}", sum_column_with_workers(&x, w)));
    same_for_every_worker_count(|w| format!("{:?}", min_column_with_workers(&x, w)));
    same_for_every_worker_count(|w| format!("{:?}", max_column_with_workers(&x, w)));
    same_for_every_worker_count(|w| format!("{:?}", mean_column_with_workers(&x, w)));
    same_for_every_worker_count(|w| format!("{:?}", variance_column_with_workers(&x, 1, w)));
    same_for_every_worker_count(|w| format!("{:?}", std_dev_column_with_workers(&x, 0, w)));
    same_for_every_worker_count(|w| format!("{:?}", correlation_columns_with_workers(&x, &y, w)));
    same_for_every_worker_count(|w| {
        format!(
            "{:?}",
            histogram_column_with_workers(&x, 10, Some((-1.0, 1.0)), w)
        )
    });
    same_for_every_worker_count(|w| {
        let result = group_by_with_workers(GroupOp::Mean, 1, &keys(10_000), &x, w).unwrap();
        format!(
            "{:?}",
            result
                .values_column()
                .map(|c| c.values::<f64>().unwrap().to_vec())
        )
    });
}

#[test]
fn histogram_without_range_also_uses_the_workers() {
    let x = float_column(10_000, 0.0);
    same_for_every_worker_count(|w| format!("{:?}", histogram_column_with_workers(&x, 7, None, w)));
}

#[test]
fn zero_workers_is_an_invalid_argument_everywhere() {
    let x = float_column(3000, 0.0);
    let is_invalid = |e: Result<(), Error>| matches!(e, Err(Error::InvalidArgument(_)));
    assert!(is_invalid(sum_column_with_workers(&x, 0).map(|_| ())));
    assert!(is_invalid(mean_column_with_workers(&x, 0).map(|_| ())));
    assert!(is_invalid(
        variance_column_with_workers(&x, 1, 0).map(|_| ())
    ));
    assert!(is_invalid(
        correlation_columns_with_workers(&x, &x, 0).map(|_| ())
    ));
    assert!(is_invalid(
        histogram_column_with_workers(&x, 4, None, 0).map(|_| ())
    ));
    assert!(is_invalid(
        group_by_with_workers(GroupOp::Sum, 0, &keys(3000), &x, 0).map(|_| ())
    ));
    // Sin filas no hay grupos, pero el argumento se valida igual.
    assert!(is_invalid(
        group_by_with_workers(GroupOp::Sum, 0, &keys(0), &float_column(0, 0.0), 0).map(|_| ())
    ));
}
