//! Factorizacion QR por reflexiones de Householder y minimos cuadrados.
//!
//! # La idea
//! `A = Q R` con `Q` de columnas ortonormales (`Q^T Q = I`) y `R` triangular superior. Se obtiene
//! anulando, columna a columna, lo que queda debajo de la diagonal con una **reflexion**
//! `H = I - 2 v v^T` (con `v` unitario) que manda la columna al eje: es la forma numericamente
//! estable, porque las reflexiones conservan las longitudes y no amplifican los errores como lo haria
//! la eliminacion gaussiana sin pivoteo.
//!
//! # Forma delgada
//! Para `A` de `m x n` con `m >= n`, `Q` es de `m x n` y `R` de `n x n`. Con `m < n` no hay factorizacion
//! de este tipo y se devuelve [`Error::InvalidArgument`]. El signo de la diagonal de `R` es el que salga
//! de la reflexion (`-signo(x0) * norma(x)`), no se fuerza positivo.
//!
//! # Minimos cuadrados
//! El `x` que minimiza `|A x - b|` cumple `R x = Q^T b`. No hace falta formar `Q`: se aplican las
//! reflexiones directamente a `b`. Es mejor que resolver las ecuaciones normales `A^T A x = A^T b`,
//! cuyo numero de condicion es el cuadrado del de `A`. El **residuo** es lo que queda de `b` debajo de
//! las primeras `n` posiciones.
//!
//! # Rango incompleto (N7)
//! Si `|R[i][i]| <= max(m, n) * eps * max(|R[j][j]|)` para algun `i`, las columnas de `A` son
//! (numericamente) dependientes y [`least_squares`] devuelve [`Error::InvalidArgument`]
//! (`rango incompleto`); no se aplica ninguna pseudoinversa. [`qr`] si factoriza esas matrices.

use geoda_core::Error;

use crate::mat::Matrix;

/// Resultado de [`qr`].
#[derive(Debug, Clone, PartialEq)]
pub struct Qr {
    /// `m x n`, columnas ortonormales.
    pub q: Matrix,
    /// `n x n`, triangular superior.
    pub r: Matrix,
}

/// Resultado de [`least_squares`].
#[derive(Debug, Clone, PartialEq)]
pub struct LeastSquares {
    /// El `x` que minimiza `|A x - b|`.
    pub coefficients: Vec<f64>,
    /// La norma euclidiana del residuo `|A x - b|`.
    pub residual_norm: f64,
}

/// Reflexiones de Householder de una matriz de `m x n` (`m >= n`) y la matriz ya triangularizada.
struct Householder {
    /// `A` reducida: `R` en el triangulo superior de las primeras `n` filas.
    reduced: Matrix,
    /// El vector unitario de cada reflexion, de largo `m - k`.
    reflectors: Vec<Vec<f64>>,
}

fn householder(matrix: &Matrix) -> Result<Householder, Error> {
    let (rows, cols) = (matrix.rows(), matrix.cols());
    if rows < cols {
        return Err(Error::InvalidArgument(format!(
            "QR pide al menos tantas filas como columnas, no {rows} x {cols}"
        )));
    }
    let mut work = matrix.clone();
    let mut reflectors = Vec::with_capacity(cols);
    for k in 0..cols {
        let mut v: Vec<f64> = (k..rows).map(|i| work.get(i, k)).collect();
        let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm == 0.0 {
            // Columna ya en cero: la reflexion es la identidad.
            reflectors.push(Vec::new());
            continue;
        }
        let alpha = if v[0] >= 0.0 { -norm } else { norm };
        v[0] -= alpha;
        let v_norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if v_norm == 0.0 {
            reflectors.push(Vec::new());
            continue;
        }
        for x in &mut v {
            *x /= v_norm;
        }
        // Aplica H a las columnas k..n: x -= 2 v (v . x).
        for j in k..cols {
            let dot: f64 = (k..rows).zip(&v).map(|(i, vi)| vi * work.get(i, j)).sum();
            for (i, vi) in (k..rows).zip(&v) {
                let value = work.get(i, j) - 2.0 * vi * dot;
                work.data_mut()[i * cols + j] = value;
            }
        }
        reflectors.push(v);
    }
    Ok(Householder {
        reduced: work,
        reflectors,
    })
}

impl Householder {
    /// Aplica `H_0, H_1, ...` (en ese orden) a un vector de largo `m`: da `Q^T x` completo.
    fn apply_transposed(&self, x: &mut [f64]) {
        for (k, v) in self.reflectors.iter().enumerate() {
            if v.is_empty() {
                continue;
            }
            let dot: f64 = v.iter().zip(&x[k..]).map(|(a, b)| a * b).sum();
            for (xi, vi) in x[k..].iter_mut().zip(v) {
                *xi -= 2.0 * vi * dot;
            }
        }
    }

    /// Aplica `H_{n-1}, ..., H_0` (en ese orden) a un vector de largo `m`: da `Q x`.
    fn apply(&self, x: &mut [f64]) {
        for (k, v) in self.reflectors.iter().enumerate().rev() {
            if v.is_empty() {
                continue;
            }
            let dot: f64 = v.iter().zip(&x[k..]).map(|(a, b)| a * b).sum();
            for (xi, vi) in x[k..].iter_mut().zip(v) {
                *xi -= 2.0 * vi * dot;
            }
        }
    }
}

/// Factoriza `A = Q R` (forma delgada).
///
/// # Errors
/// [`Error::InvalidArgument`] si `a` tiene menos filas que columnas.
pub fn qr(matrix: &Matrix) -> Result<Qr, Error> {
    let (rows, cols) = (matrix.rows(), matrix.cols());
    let reflections = householder(matrix)?;
    let mut r = Matrix::zeros(cols, cols)?;
    for i in 0..cols {
        for j in i..cols {
            r.data_mut()[i * cols + j] = reflections.reduced.get(i, j);
        }
    }
    // Q = H_0 ... H_{n-1} aplicado a las primeras n columnas de la identidad.
    let mut q = Matrix::zeros(rows, cols)?;
    let mut column = vec![0.0; rows];
    for j in 0..cols {
        column.fill(0.0);
        column[j] = 1.0;
        reflections.apply(&mut column);
        for (i, &value) in column.iter().enumerate() {
            q.data_mut()[i * cols + j] = value;
        }
    }
    Ok(Qr { q, r })
}

/// Minimos cuadrados: el `x` que minimiza `|A x - b|`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `matrix` tiene menos filas que columnas o sus columnas son
/// dependientes (`rango incompleto`); [`Error::LengthMismatch`] si `rhs.len() != matrix.rows()`.
pub fn least_squares(matrix: &Matrix, rhs: &[f64]) -> Result<LeastSquares, Error> {
    let (rows, cols) = (matrix.rows(), matrix.cols());
    if rhs.len() != rows {
        return Err(Error::LengthMismatch(format!(
            "A tiene {rows} filas y b {} valores",
            rhs.len()
        )));
    }
    let reflections = householder(matrix)?;
    let diagonal = |i: usize| reflections.reduced.get(i, i).abs();
    let largest = (0..cols).map(diagonal).fold(0.0_f64, f64::max);
    #[allow(clippy::cast_precision_loss)] // las dimensiones son pequenas frente a 2^53
    let tolerance = rows.max(cols) as f64 * f64::EPSILON * largest;
    if (0..cols).any(|i| diagonal(i) <= tolerance) {
        return Err(Error::InvalidArgument(
            "rango incompleto: las columnas son dependientes".to_string(),
        ));
    }
    let mut rotated = rhs.to_vec();
    reflections.apply_transposed(&mut rotated);
    // R x = (Q^T b)[..n], hacia atras.
    let mut solution = rotated[..cols].to_vec();
    for i in (0..cols).rev() {
        for j in i + 1..cols {
            solution[i] -= reflections.reduced.get(i, j) * solution[j];
        }
        solution[i] /= reflections.reduced.get(i, i);
    }
    let residual_norm = rotated[cols..].iter().map(|v| v * v).sum::<f64>().sqrt();
    Ok(LeastSquares {
        coefficients: solution,
        residual_norm,
    })
}

#[cfg(test)]
mod tests {
    use super::{least_squares, qr};
    use crate::lu::solve;
    use crate::mat::Matrix;
    use geoda_core::Error;

    fn m(rows: &[&[f64]]) -> Matrix {
        Matrix::from_rows(&rows.iter().map(|r| r.to_vec()).collect::<Vec<_>>()).unwrap()
    }

    fn assert_close(a: &Matrix, b: &Matrix, tol: f64) {
        assert_eq!((a.rows(), a.cols()), (b.rows(), b.cols()));
        for (x, y) in a.data().iter().zip(b.data()) {
            assert!((x - y).abs() <= tol, "{x} contra {y}");
        }
    }

    #[test]
    fn q_por_r_es_a() {
        let a = m(&[&[1.0, 2.0], &[3.0, 4.0], &[5.0, 6.0]]);
        let f = qr(&a).unwrap();
        assert_eq!(
            (f.q.rows(), f.q.cols(), f.r.rows(), f.r.cols()),
            (3, 2, 2, 2)
        );
        assert_close(&f.q.matmul(&f.r).unwrap(), &a, 1e-12);
        assert_eq!(f.r.get(1, 0), 0.0);
    }

    #[test]
    fn las_columnas_de_q_son_ortonormales() {
        let a = m(&[
            &[1.0, 2.0, 0.5],
            &[3.0, 4.0, -1.0],
            &[5.0, 6.0, 2.0],
            &[0.0, 1.0, 1.0],
        ]);
        let q = qr(&a).unwrap().q;
        let gram = q.transpose().unwrap().matmul(&q).unwrap();
        assert_close(&gram, &Matrix::identity(3).unwrap(), 1e-12);
    }

    #[test]
    fn una_recta_exacta() {
        // y = 1 + 2 x en x = 0, 1, 2.
        let a = m(&[&[1.0, 0.0], &[1.0, 1.0], &[1.0, 2.0]]);
        let fit = least_squares(&a, &[1.0, 3.0, 5.0]).unwrap();
        assert!((fit.coefficients[0] - 1.0).abs() < 1e-12);
        assert!((fit.coefficients[1] - 2.0).abs() < 1e-12);
        assert!(fit.residual_norm < 1e-12);
    }

    #[test]
    fn el_residuo_es_ortogonal_a_las_columnas() {
        let a = m(&[&[1.0, 0.0], &[1.0, 1.0], &[1.0, 2.0], &[1.0, 3.0]]);
        let b = [1.0, 2.5, 2.0, 5.0];
        let fit = least_squares(&a, &b).unwrap();
        let ax = a.matvec(&fit.coefficients).unwrap();
        let residual: Vec<f64> = b.iter().zip(&ax).map(|(p, q)| p - q).collect();
        let norm = residual.iter().map(|v| v * v).sum::<f64>().sqrt();
        assert!((norm - fit.residual_norm).abs() < 1e-12);
        let orthogonal = a.transpose().unwrap().matvec(&residual).unwrap();
        assert!(orthogonal.iter().all(|v| v.abs() < 1e-9), "{orthogonal:?}");
    }

    #[test]
    fn rango_incompleto() {
        let a = m(&[&[1.0, 2.0], &[2.0, 4.0], &[3.0, 6.0]]);
        let err = least_squares(&a, &[1.0, 2.0, 3.0]).unwrap_err();
        assert!(
            matches!(&err, Error::InvalidArgument(msg) if msg.contains("rango incompleto")),
            "{err}"
        );
        // Una columna de ceros tambien.
        let zero = m(&[&[1.0, 0.0], &[2.0, 0.0], &[3.0, 0.0]]);
        assert!(matches!(
            least_squares(&zero, &[1.0, 2.0, 3.0]),
            Err(Error::InvalidArgument(_))
        ));
        // La factorizacion en si funciona con rango incompleto.
        let f = qr(&a).unwrap();
        assert_close(&f.q.matmul(&f.r).unwrap(), &a, 1e-12);
    }

    #[test]
    fn menos_filas_que_columnas() {
        let wide = m(&[&[1.0, 2.0, 3.0]]);
        assert!(matches!(qr(&wide), Err(Error::InvalidArgument(_))));
        assert!(matches!(
            least_squares(&wide, &[1.0]),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn b_de_otro_largo() {
        let a = m(&[&[1.0], &[2.0]]);
        assert!(matches!(
            least_squares(&a, &[1.0]),
            Err(Error::LengthMismatch(_))
        ));
    }

    #[test]
    fn coincide_con_las_ecuaciones_normales() {
        let mut state: u64 = 0x91;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 40).unwrap()) / 8_388_608.0 - 1.0
        };
        for (rows, cols) in [(3, 1), (4, 2), (8, 3), (12, 5), (20, 8), (5, 5)] {
            let a =
                Matrix::from_flat(rows, cols, (0..rows * cols).map(|_| next()).collect()).unwrap();
            let b: Vec<f64> = (0..rows).map(|_| next()).collect();
            let fit = least_squares(&a, &b).unwrap();
            let at = a.transpose().unwrap();
            let normal = solve(&at.matmul(&a).unwrap(), &at.matvec(&b).unwrap()).unwrap();
            for (got, want) in fit.coefficients.iter().zip(&normal) {
                assert!(
                    (got - want).abs() < 1e-6,
                    "{rows}x{cols}: {got} contra {want}"
                );
            }
            let f = qr(&a).unwrap();
            assert_close(&f.q.matmul(&f.r).unwrap(), &a, 1e-12);
        }
    }
}
