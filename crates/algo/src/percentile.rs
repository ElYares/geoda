//! Mediana y percentiles de una columna numerica, sin ordenarla entera.
//!
//! # La idea
//! El percentil `p` es el valor bajo el cual queda el `p` % de los datos. Con `n` valores
//! ordenados `x[0..n]`, la definicion tipo 7 (la de `R`, `NumPy` y Excel; decision F3) lo
//! busca en la posicion fraccionaria `h = (n - 1) * p / 100`. Si `h` cae entre dos
//! posiciones, interpola en linea recta entre sus vecinos:
//!
//! ```text
//! lo = floor(h)      frac = h - lo      resultado = x[lo] + frac * (x[lo + 1] - x[lo])
//! ```
//!
//! Para la mediana (`p = 50`) y `n` par, `h` termina en `.5` y el resultado es el
//! promedio de los dos valores centrales. No hace falta ordenar: solo se necesitan los
//! valores de las posiciones `lo` y `lo + 1`, y eso lo da la seleccion de [`crate::select`].
//!
//! Al pedir varios percentiles a la vez se copia la columna una sola vez y las
//! posiciones se seleccionan de menor a mayor. Tras seleccionar la posicion `k`, todo lo
//! que quedo a su izquierda es menor o igual y a su derecha mayor o igual, asi que la
//! siguiente seleccion solo mira el tramo de la derecha.

use std::cmp::Ordering;

use geoda_core::memory::Native;
use geoda_core::{Column, DataType, Error};

use crate::select::{Method, select_with, valid_values};

/// Mediana de una columna `Int32`, `Int64` o `Float64`. Es el percentil 50.
///
/// # Errors
/// Los mismos que [`percentile_column`].
pub fn median_column(column: &Column, method: Method) -> Result<f64, Error> {
    percentile_column(column, 50.0, method)
}

/// Percentil `p` (en `[0, 100]`) de una columna `Int32`, `Int64` o `Float64`, por
/// interpolacion lineal tipo 7.
///
/// Los nulos se ignoran (decision F1). En `Float64`, si algun valor valido es `NaN` el
/// resultado es `NaN` (decision F2). El resultado es siempre `f64`; en `Int64` los
/// valores de magnitud mayor a 2^53 pierden precision al convertirse. La columna no
/// cambia.
///
/// # Errors
/// - [`Error::InvalidArgument`] si `p` no esta en `[0, 100]` (o es `NaN`), o si la
///   columna no tiene valores validos.
/// - [`Error::Unsupported`] para `Bool` y `Utf8`.
pub fn percentile_column(column: &Column, p: f64, method: Method) -> Result<f64, Error> {
    let mut result = percentiles_column(column, &[p], method)?;
    // `percentiles_column` devuelve un resultado por cada `p`, y aqui hay uno.
    result
        .pop()
        .ok_or_else(|| Error::InvalidArgument("percentil sin resultado".to_string()))
}

/// Varios percentiles de una columna, reutilizando la seleccion. El resultado `i`
/// corresponde a `ps[i]`; coincide con llamar a [`percentile_column`] uno por uno.
///
/// # Errors
/// Los mismos que [`percentile_column`].
pub fn percentiles_column(column: &Column, ps: &[f64], method: Method) -> Result<Vec<f64>, Error> {
    for &p in ps {
        if !(0.0..=100.0).contains(&p) {
            return Err(Error::InvalidArgument(format!(
                "el percentil debe estar en [0, 100], llego {p}"
            )));
        }
    }
    match column.dtype() {
        DataType::Int32 => {
            percentiles_primitive::<i32>(column, ps, method, i32::cmp, never_nan, lerp_i32)
        }
        DataType::Int64 => {
            percentiles_primitive::<i64>(column, ps, method, i64::cmp, never_nan, lerp_i64)
        }
        DataType::Float64 => percentiles_primitive::<f64>(
            column,
            ps,
            method,
            f64::total_cmp,
            |v| v.is_nan(),
            lerp_f64,
        ),
        other => Err(Error::Unsupported(format!(
            "percentiles en columnas {other:?} aun no esta soportado"
        ))),
    }
}

fn never_nan<T>(_: &T) -> bool {
    false
}

fn lerp_i32(a: i32, b: i32, frac: f64) -> f64 {
    if frac == 0.0 {
        return f64::from(a);
    }
    f64::from(a) + frac * (f64::from(b) - f64::from(a))
}

#[allow(clippy::cast_precision_loss)] // documentado: pierde precision sobre 2^53
fn lerp_i64(a: i64, b: i64, frac: f64) -> f64 {
    if frac == 0.0 {
        return a as f64;
    }
    // La resta va en i128 para que no desborde con extremos opuestos.
    a as f64 + frac * ((i128::from(b) - i128::from(a)) as f64)
}

fn lerp_f64(a: f64, b: f64, frac: f64) -> f64 {
    // Con frac = 0 o valores iguales se evita `inf - inf = NaN` y `inf * 0 = NaN`.
    if frac == 0.0 || a.total_cmp(&b) == Ordering::Equal {
        return a;
    }
    a + frac * (b - a)
}

/// Posicion de un percentil: los dos vecinos a interpolar y el peso del segundo.
struct Position {
    lo: usize,
    hi: usize,
    frac: f64,
}

/// Posicion tipo 7 de `p` en `n > 0` valores ordenados.
#[allow(clippy::cast_precision_loss)] // n real cabe de sobra en los 53 bits de f64
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 <= floor(h) <= n - 1
fn position(n: usize, p: f64) -> Position {
    let h = (n - 1) as f64 * p / 100.0;
    let lo = (h.floor() as usize).min(n - 1);
    let frac = h - lo as f64;
    let hi = if frac > 0.0 { (lo + 1).min(n - 1) } else { lo };
    Position { lo, hi, frac }
}

fn percentiles_primitive<T: Native>(
    column: &Column,
    ps: &[f64],
    method: Method,
    cmp: impl Fn(&T, &T) -> Ordering,
    is_nan: impl Fn(&T) -> bool,
    lerp: impl Fn(T, T, f64) -> f64,
) -> Result<Vec<f64>, Error> {
    let mut valid = valid_values::<T>(column)?;
    if valid.is_empty() && !ps.is_empty() {
        return Err(Error::InvalidArgument(
            "la columna no tiene valores validos".to_string(),
        ));
    }
    if valid.iter().any(&is_nan) {
        return Ok(vec![f64::NAN; ps.len()]);
    }
    let positions: Vec<Position> = ps.iter().map(|&p| position(valid.len(), p)).collect();

    // Posiciones a seleccionar: sin repetir y de menor a mayor.
    let mut ranks: Vec<usize> = positions.iter().flat_map(|q| [q.lo, q.hi]).collect();
    ranks.sort_unstable();
    ranks.dedup();

    let mut values = Vec::with_capacity(ranks.len());
    let mut start = 0;
    for &rank in &ranks {
        // Lo anterior a `start` ya quedo a la izquierda de todo el tramo.
        let value = select_with(&mut valid[start..], rank - start, method, &cmp)?;
        values.push(value);
        start = rank + 1;
    }

    // `ranks` contiene cada `lo` y `hi`, asi que `partition_point` cae justo en ellos.
    let value_at = |rank: usize| values[ranks.partition_point(|&r| r < rank)];
    Ok(positions
        .iter()
        .map(|q| lerp(value_at(q.lo), value_at(q.hi), q.frac))
        .collect())
}

#[cfg(test)]
// Los tests comparan contra valores exactos conocidos y convierten enteros chicos a f64.
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use geoda_core::{BoolBuilder, PrimitiveBuilder};

    use super::*;

    const METHODS: [Method; 2] = [Method::Quickselect, Method::MedianOfMedians];

    /// Generador pseudoaleatorio con semilla fija (xorshift64), igual que en `select`.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }
    }

    fn int64_column(values: &[Option<i64>]) -> Column {
        let mut builder = PrimitiveBuilder::<i64>::new();
        for value in values {
            match value {
                Some(v) => builder.append(*v),
                None => builder.append_null(),
            }
        }
        builder.finish().unwrap()
    }

    fn float64_column(values: &[f64]) -> Column {
        let mut builder = PrimitiveBuilder::<f64>::new();
        for &value in values {
            builder.append(value);
        }
        builder.finish().unwrap()
    }

    /// Oraculo: ordena con la biblioteca estandar y aplica la formula tipo 7.
    fn oracle(sorted: &[f64], p: f64) -> f64 {
        let h = (sorted.len() - 1) as f64 * p / 100.0;
        let lo = h.floor() as usize;
        let frac = h - lo as f64;
        if frac == 0.0 {
            sorted[lo]
        } else {
            sorted[lo] + frac * (sorted[lo + 1] - sorted[lo])
        }
    }

    fn within_one_ulp(a: f64, b: f64) -> bool {
        a.to_bits().abs_diff(b.to_bits()) <= 1
    }

    #[test]
    fn documented_examples() {
        let column = int64_column(&[Some(1), Some(2), Some(3), Some(4)]);
        for method in METHODS {
            assert_eq!(median_column(&column, method).unwrap(), 2.5);
            assert_eq!(percentile_column(&column, 25.0, method).unwrap(), 1.75);
            assert_eq!(percentile_column(&column, 0.0, method).unwrap(), 1.0);
            assert_eq!(percentile_column(&column, 100.0, method).unwrap(), 4.0);
        }
    }

    #[test]
    fn odd_length_median_is_the_middle_value_and_one_value_is_every_percentile() {
        let odd = int64_column(&[Some(9), Some(1), Some(5)]);
        let single = int64_column(&[Some(7)]);
        for method in METHODS {
            assert_eq!(median_column(&odd, method).unwrap(), 5.0);
            for p in [0.0, 33.3, 50.0, 100.0] {
                assert_eq!(percentile_column(&single, p, method).unwrap(), 7.0);
            }
        }
    }

    #[test]
    fn nulls_are_ignored() {
        let column = int64_column(&[None, Some(30), Some(10), None, Some(20), Some(40)]);
        for method in METHODS {
            assert_eq!(median_column(&column, method).unwrap(), 25.0);
        }
    }

    #[test]
    fn nan_makes_the_result_nan() {
        let column = float64_column(&[1.0, f64::NAN, 3.0]);
        for method in METHODS {
            assert!(median_column(&column, method).unwrap().is_nan());
            let all = percentiles_column(&column, &[0.0, 50.0, 100.0], method).unwrap();
            assert!(all.iter().all(|v| v.is_nan()));
        }
    }

    #[test]
    fn infinities_do_not_poison_exact_positions() {
        let column = float64_column(&[f64::NEG_INFINITY, 1.0, f64::INFINITY]);
        for method in METHODS {
            assert_eq!(
                percentile_column(&column, 0.0, method).unwrap(),
                f64::NEG_INFINITY
            );
            assert_eq!(percentile_column(&column, 50.0, method).unwrap(), 1.0);
            assert_eq!(
                percentile_column(&column, 100.0, method).unwrap(),
                f64::INFINITY
            );
        }
    }

    #[test]
    fn invalid_arguments_are_errors_not_panics() {
        let column = int64_column(&[Some(1), Some(2)]);
        let empty = int64_column(&[]);
        let all_null = int64_column(&[None, None]);
        for method in METHODS {
            for p in [-0.1, 100.1, f64::NAN, f64::INFINITY] {
                assert!(matches!(
                    percentile_column(&column, p, method),
                    Err(Error::InvalidArgument(_))
                ));
            }
            assert!(matches!(
                median_column(&empty, method),
                Err(Error::InvalidArgument(_))
            ));
            assert!(matches!(
                median_column(&all_null, method),
                Err(Error::InvalidArgument(_))
            ));
        }
        let mut bools = BoolBuilder::new();
        bools.append(true);
        assert!(matches!(
            median_column(&bools.finish().unwrap(), Method::default()),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn int32_and_extreme_int64_do_not_overflow() {
        let mut ints = PrimitiveBuilder::<i32>::new();
        for value in [i32::MIN, i32::MAX] {
            ints.append(value);
        }
        let ints = ints.finish().unwrap();
        assert_eq!(median_column(&ints, Method::default()).unwrap(), -0.5);

        let wide = int64_column(&[Some(i64::MIN), Some(i64::MAX)]);
        let median = median_column(&wide, Method::default()).unwrap();
        assert!(median.abs() <= 1.0, "mediana de los extremos: {median}");
    }

    #[test]
    fn many_percentiles_match_one_by_one() {
        let column = int64_column(&[
            Some(8),
            None,
            Some(2),
            Some(2),
            Some(9),
            Some(4),
            Some(7),
            Some(1),
            None,
            Some(6),
        ]);
        // Con repetidos, fuera de orden y con posiciones que se comparten.
        let ps = [90.0, 0.0, 50.0, 25.0, 50.0, 100.0, 33.3];
        for method in METHODS {
            let together = percentiles_column(&column, &ps, method).unwrap();
            for (p, got) in ps.iter().zip(&together) {
                assert_eq!(
                    *got,
                    percentile_column(&column, *p, method).unwrap(),
                    "p = {p}"
                );
            }
        }
        let none = percentiles_column(&column, &[], Method::default()).unwrap();
        assert_eq!(none.len(), 0);
    }

    #[test]
    fn matches_the_sorted_oracle_on_1000_seeded_inputs() {
        let mut rng = Rng(0xBEEF_0016);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(60)).unwrap() + 1;
            let range = [4, 1_000_000][case % 2];
            let data: Vec<i64> = (0..len)
                .map(|_| i64::try_from(rng.below(range)).unwrap() - 2)
                .collect();
            let mut sorted: Vec<f64> = data.iter().map(|&v| v as f64).collect();
            sorted.sort_unstable_by(f64::total_cmp);
            let floats: Vec<f64> = data.iter().map(|&v| v as f64 / 7.0).collect();
            let mut sorted_floats = floats.clone();
            sorted_floats.sort_unstable_by(f64::total_cmp);
            let ints = int64_column(&data.iter().map(|&v| Some(v)).collect::<Vec<_>>());
            let floats = float64_column(&floats);
            let ps: Vec<f64> = (0..4).map(|_| rng.below(10_001) as f64 / 100.0).collect();
            for method in METHODS {
                let got_ints = percentiles_column(&ints, &ps, method).unwrap();
                let got_floats = percentiles_column(&floats, &ps, method).unwrap();
                for (i, &p) in ps.iter().enumerate() {
                    assert_eq!(
                        got_ints[i],
                        oracle(&sorted, p),
                        "{method:?}, caso {case}, p = {p}"
                    );
                    assert!(
                        within_one_ulp(got_floats[i], oracle(&sorted_floats, p)),
                        "{method:?}, caso {case}, p = {p}: {} vs {}",
                        got_floats[i],
                        oracle(&sorted_floats, p)
                    );
                }
            }
        }
    }

    #[test]
    fn sliced_column_and_the_original_is_untouched() {
        let column = int64_column(&[Some(9), Some(4), None, Some(1), Some(7)]);
        let window = column.slice(1, 3).unwrap(); // [4, NULL, 1]
        assert_eq!(median_column(&window, Method::default()).unwrap(), 2.5);
        assert_eq!(column.values::<i64>().unwrap()[0], 9);
    }
}
