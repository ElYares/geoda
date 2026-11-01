//! Pruebas de covarianza y correlacion de Pearson (HU-022).

use geoda_core::{BoolBuilder, Column, Error, PrimitiveBuilder};
use geoda_stats::{BLOCK, Correlation, Covariance, correlation_columns, covariance_columns};

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

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * b.abs().max(f64::MIN_POSITIVE)
}

/// Referencia en dos pasadas: (covarianza, correlacion, escala `sd_x * sd_y` sin dividir).
#[allow(clippy::cast_precision_loss)]
fn two_pass(xs: &[f64], ys: &[f64], ddof: usize) -> (f64, f64, f64) {
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let c2: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let m2x: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let m2y: f64 = ys.iter().map(|y| (y - my) * (y - my)).sum();
    let scale = (m2x * m2y).sqrt();
    (c2 / (xs.len() - ddof) as f64, c2 / scale, scale)
}

#[test]
fn perfect_positive_and_negative_correlation() {
    let x = ints(&[1, 2, 3, 4]);
    assert!(close(
        correlation_columns(&x, &ints(&[2, 4, 6, 8])).unwrap(),
        1.0,
        1e-15
    ));
    assert!(close(
        correlation_columns(&x, &ints(&[8, 6, 4, 2])).unwrap(),
        -1.0,
        1e-15
    ));
}

#[test]
fn textbook_example() {
    let (x, y) = (ints(&[1, 2, 3, 4]), ints(&[1, 3, 2, 4]));
    assert!(close(correlation_columns(&x, &y).unwrap(), 0.8, 1e-15));
    assert!(close(
        covariance_columns(&x, &y, 1).unwrap(),
        4.0 / 3.0,
        1e-15
    ));
    assert!(close(covariance_columns(&x, &y, 0).unwrap(), 1.0, 1e-15));
}

#[test]
fn mixed_integer_and_float_columns() {
    let (x, y) = (ints(&[1, 2, 3, 4]), floats(&[1.0, 3.0, 2.0, 4.0]));
    assert!(close(correlation_columns(&x, &y).unwrap(), 0.8, 1e-15));
}

#[test]
fn different_lengths_are_an_error() {
    let err = correlation_columns(&ints(&[1, 2, 3]), &ints(&[1, 2])).unwrap_err();
    assert!(matches!(err, Error::LengthMismatch(_)), "{err:?}");
    let err = covariance_columns(&ints(&[1, 2, 3]), &ints(&[1, 2]), 1).unwrap_err();
    assert!(matches!(err, Error::LengthMismatch(_)), "{err:?}");
}

#[test]
fn a_null_in_either_column_drops_the_row_from_both() {
    let mut bx = PrimitiveBuilder::<f64>::new();
    let mut by = PrimitiveBuilder::<f64>::new();
    // Filas 2 y 4 se descartan; las otras son (1,1) (2,3) (3,2) (4,4) con un valor
    // espurio en las filas descartadas que cambiaria el resultado si se contara.
    for (x, y) in [
        (Some(1.0), Some(1.0)),
        (Some(2.0), Some(3.0)),
        (None, Some(900.0)),
        (Some(3.0), Some(2.0)),
        (Some(-900.0), None),
        (Some(4.0), Some(4.0)),
    ] {
        match x {
            Some(v) => bx.append(v),
            None => bx.append_null(),
        }
        match y {
            Some(v) => by.append(v),
            None => by.append_null(),
        }
    }
    let (x, y) = (bx.finish().unwrap(), by.finish().unwrap());
    assert!(close(correlation_columns(&x, &y).unwrap(), 0.8, 1e-15));
    assert!(close(
        covariance_columns(&x, &y, 1).unwrap(),
        4.0 / 3.0,
        1e-15
    ));
}

#[test]
fn a_constant_column_is_an_error() {
    let err = correlation_columns(&ints(&[5, 5, 5]), &ints(&[1, 2, 3])).unwrap_err();
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");
    let err = correlation_columns(&ints(&[1, 2, 3]), &floats(&[2.5, 2.5, 2.5])).unwrap_err();
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");
    // La covarianza si esta definida: vale cero.
    let cov = covariance_columns(&ints(&[5, 5, 5]), &ints(&[1, 2, 3]), 1).unwrap();
    assert!(cov.abs() < 1e-300, "{cov}");
}

#[test]
fn too_few_pairs_are_an_error() {
    let empty = floats(&[]);
    assert!(matches!(
        correlation_columns(&empty, &empty).unwrap_err(),
        Error::InvalidArgument(_)
    ));
    let one = floats(&[1.0]);
    assert!(matches!(
        correlation_columns(&one, &one).unwrap_err(),
        Error::InvalidArgument(_)
    ));
    assert!(matches!(
        covariance_columns(&one, &one, 1).unwrap_err(),
        Error::InvalidArgument(_)
    ));
}

#[test]
fn nan_propagates() {
    let x = floats(&[1.0, f64::NAN, 3.0, 4.0]);
    let y = floats(&[1.0, 3.0, 2.0, 4.0]);
    assert!(correlation_columns(&x, &y).unwrap().is_nan());
    assert!(covariance_columns(&x, &y, 1).unwrap().is_nan());
    assert!(correlation_columns(&y, &x).unwrap().is_nan());
}

#[test]
fn bool_is_unsupported() {
    let mut b = BoolBuilder::new();
    b.append(true);
    let flags = b.finish().unwrap();
    assert!(matches!(
        Correlation::new(flags.dtype(), flags.dtype()).unwrap_err(),
        Error::Unsupported(_)
    ));
    assert!(matches!(
        Covariance::new(flags.dtype(), floats(&[1.0]).dtype(), 1).unwrap_err(),
        Error::Unsupported(_)
    ));
}

#[test]
fn blocks_match_a_single_pass_and_every_worker_count_is_bit_identical() {
    let xs: Vec<f64> = (0..10_000_u32)
        .map(|i| 1e6 + f64::from(i % 997) * 0.37 + (f64::from(i) * 0.013).sin())
        .collect();
    let ys: Vec<f64> = xs
        .iter()
        .zip(0..10_000_u32)
        .map(|(x, i)| 3.0 * x - 5e5 + (f64::from(i) * 0.7).cos() * 40.0)
        .collect();
    let (x, y) = (floats(&xs), floats(&ys));
    let corr = Correlation::new(x.dtype(), y.dtype()).unwrap();
    let cov = Covariance::new(x.dtype(), y.dtype(), 1).unwrap();

    let (want_cov, want_corr, _) = two_pass(&xs, &ys, 1);
    let ref_corr = corr.compute_with_workers(&x, &y, 1).unwrap();
    let ref_cov = cov.compute_with_workers(&x, &y, 1).unwrap();
    assert!(close(ref_corr, want_corr, 1e-12));
    assert!(close(ref_cov, want_cov, 1e-12));
    for workers in [2, 3, 7] {
        let got = corr.compute_with_workers(&x, &y, workers).unwrap();
        assert_eq!(got.to_bits(), ref_corr.to_bits(), "{workers} trabajadores");
        let got = cov.compute_with_workers(&x, &y, workers).unwrap();
        assert_eq!(got.to_bits(), ref_cov.to_bits(), "{workers} trabajadores");
    }
    assert!(matches!(
        corr.compute_with_workers(&x, &y, 0).unwrap_err(),
        Error::InvalidArgument(_)
    ));
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
#[allow(clippy::cast_precision_loss)]
fn a_thousand_random_pairs_match_the_two_pass_calculation() {
    let mut rng = Rng(29);
    let mut checked = 0;
    for case in 0..1000 {
        let len = usize::try_from(rng.below(3 * BLOCK as u64)).unwrap();
        let offset_x = rng.below(1_000_000) as f64 * 1000.0;
        let offset_y = rng.below(1_000_000) as f64 * 1000.0;
        let slope = (rng.below(2001) as f64 - 1000.0) / 100.0;
        let (mut bx, mut by) = (
            PrimitiveBuilder::<f64>::new(),
            PrimitiveBuilder::<f64>::new(),
        );
        let (mut vx, mut vy) = (Vec::new(), Vec::new());
        for _ in 0..len {
            let base = (rng.below(2_000_000) as f64 - 1_000_000.0) / 7.0;
            let noise = (rng.below(2_000_000) as f64 - 1_000_000.0) / 3.0;
            let (x, y) = (offset_x + base, offset_y + slope * base + noise);
            let (null_x, null_y) = (rng.below(10) == 0, rng.below(10) == 0);
            if null_x {
                bx.append_null();
            } else {
                bx.append(x);
            }
            if null_y {
                by.append_null();
            } else {
                by.append(y);
            }
            if !null_x && !null_y {
                vx.push(x);
                vy.push(y);
            }
        }
        let (x, y) = (bx.finish().unwrap(), by.finish().unwrap());
        if vx.len() < 2 {
            continue;
        }
        let (want_cov, want_corr, scale) = two_pass(&vx, &vy, 1);
        let got_corr = correlation_columns(&x, &y).unwrap();
        let got_cov = covariance_columns(&x, &y, 1).unwrap();
        assert!((-1.0..=1.0).contains(&got_corr), "caso {case}: {got_corr}");
        // Una correlacion cercana a 0 no admite error relativo; se mide contra la escala.
        assert!(
            (got_corr - want_corr).abs() <= 1e-12,
            "caso {case}: {got_corr} contra {want_corr}"
        );
        let cov_scale = scale / (vx.len() - 1) as f64;
        assert!(
            (got_cov - want_cov).abs() <= 1e-12 * cov_scale,
            "caso {case}: {got_cov} contra {want_cov}"
        );
        checked += 1;
    }
    assert!(checked > 900, "solo {checked} casos con pares suficientes");
}
