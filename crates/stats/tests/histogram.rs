//! Pruebas del histograma de ancho fijo (HU-023).

use geoda_core::{BoolBuilder, Column, Error, PrimitiveBuilder};
use geoda_stats::{
    BLOCK, Histogram, HistogramData, aggregate, aggregate_with_workers, histogram_column,
};

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

fn total(h: &HistogramData) -> usize {
    h.counts.iter().sum::<usize>() + h.below + h.above + h.nulls + h.nans
}

#[test]
fn spec_example_with_four_bins() {
    let column = ints(&[1, 2, 2, 3, 4, 4, 4, 5]);
    let h = histogram_column(&column, 4, None).unwrap();
    assert_eq!(h.edges, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(h.counts, vec![1, 2, 1, 4]);
    assert_eq!(total(&h), 8);
}

#[test]
fn the_maximum_falls_in_the_last_bin() {
    let h = histogram_column(&floats(&[0.0, 10.0]), 5, Some((0.0, 10.0))).unwrap();
    assert_eq!(h.counts, vec![1, 0, 0, 0, 1]);
    assert_eq!((h.below, h.above), (0, 0));
}

#[test]
fn values_outside_an_explicit_range_are_counted_apart() {
    let column = ints(&[1, 2, 3, 4, 5, 6]);
    let h = histogram_column(&column, 2, Some((2.0, 4.0))).unwrap();
    assert_eq!(h.counts, vec![1, 2]);
    assert_eq!((h.below, h.above), (1, 2));
    assert_eq!(total(&h), 6);
}

#[test]
fn nulls_and_nans_are_counted_apart() {
    let mut b = PrimitiveBuilder::<f64>::new();
    b.append(1.0);
    b.append_null();
    b.append(f64::NAN);
    b.append(3.0);
    let h = histogram_column(&b.finish().unwrap(), 2, None).unwrap();
    assert_eq!(h.edges, vec![1.0, 2.0, 3.0]);
    assert_eq!(h.counts, vec![1, 1]);
    assert_eq!((h.nulls, h.nans), (1, 1));
    assert_eq!(total(&h), 4);
}

#[test]
fn invalid_arguments_are_errors() {
    let column = ints(&[1, 2, 3]);
    let bad = |r: Result<HistogramData, Error>| matches!(r, Err(Error::InvalidArgument(_)));
    assert!(bad(histogram_column(&column, 0, None)));
    assert!(bad(histogram_column(&column, 2, Some((5.0, 1.0)))));
    assert!(bad(histogram_column(
        &column,
        2,
        Some((0.0, f64::INFINITY))
    )));
    assert!(bad(histogram_column(&column, 2, Some((f64::NAN, 1.0)))));
    assert!(bad(histogram_column(&column, usize::MAX, None)));
    assert!(bad(histogram_column(&ints(&[]), 2, None)));
    assert!(bad(histogram_column(&floats(&[f64::NAN]), 2, None)));
    assert!(bad(histogram_column(
        &floats(&[1.0, f64::INFINITY]),
        2,
        None
    )));
}

#[test]
fn non_numeric_columns_are_unsupported() {
    let mut b = BoolBuilder::new();
    b.append(true);
    let result = histogram_column(&b.finish().unwrap(), 2, None);
    assert!(matches!(result, Err(Error::Unsupported(_))));
}

#[test]
fn equal_values_do_not_divide_by_zero() {
    let h = histogram_column(&ints(&[7, 7, 7]), 3, None).unwrap();
    assert_eq!(h.edges.len(), 4);
    assert_eq!(h.counts.iter().sum::<usize>(), 3);
    assert_eq!(h.counts.iter().filter(|&&c| c > 0).count(), 1);
    assert_eq!((h.below, h.above), (0, 0));
}

#[test]
fn merging_blocks_equals_counting_at_once() {
    #[allow(clippy::cast_precision_loss)]
    let values: Vec<f64> = (0..BLOCK * 3 + 17)
        .map(|i| (i % 101) as f64 * 0.37)
        .collect();
    let column = floats(&values);
    let whole = histogram_column(&column, 10, Some((0.0, 40.0))).unwrap();
    let agg = Histogram::new(column.dtype(), 10, 0.0, 40.0).unwrap();
    for workers in [1, 2, 5] {
        assert_eq!(
            aggregate_with_workers(&agg, &column, workers).unwrap(),
            whole
        );
    }
    assert_eq!(aggregate(&agg, &column).unwrap(), whole);
    assert_eq!(total(&whole), values.len());
}

/// Generador congruencial simple: la semilla fija hace la prueba reproducible.
fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 11
}

#[test]
fn matches_a_naive_count_over_the_edges() {
    let mut seed = 42_u64;
    for case in 0..1000 {
        let n = (next(&mut seed) % 60) as usize;
        let n_bins = 1 + (next(&mut seed) % 12) as usize;
        let mut b = PrimitiveBuilder::<f64>::new();
        let mut kept = Vec::new();
        let (mut nulls, mut nans) = (0, 0);
        for _ in 0..n {
            match next(&mut seed) % 10 {
                0 => {
                    b.append_null();
                    nulls += 1;
                }
                1 => {
                    b.append(f64::NAN);
                    nans += 1;
                }
                _ => {
                    #[allow(clippy::cast_precision_loss)]
                    let v = (next(&mut seed) % 2001) as f64 / 100.0 - 10.0;
                    b.append(v);
                    kept.push(v);
                }
            }
        }
        let column = b.finish().unwrap();
        let (lo, hi) = (-5.0, 7.5);
        let h = histogram_column(&column, n_bins, Some((lo, hi))).unwrap();

        let mut counts = vec![0_usize; n_bins];
        let (mut below, mut above) = (0, 0);
        for &v in &kept {
            if v < h.edges[0] {
                below += 1;
            } else if v > h.edges[n_bins] {
                above += 1;
            } else {
                let i = (0..n_bins)
                    .find(|&i| v < h.edges[i + 1])
                    .unwrap_or(n_bins - 1);
                counts[i] += 1;
            }
        }
        assert_eq!(h.counts, counts, "caso {case}");
        assert_eq!((h.below, h.above), (below, above), "caso {case}");
        assert_eq!((h.nulls, h.nans), (nulls, nans), "caso {case}");
        assert_eq!(total(&h), n, "caso {case}");
    }
}
