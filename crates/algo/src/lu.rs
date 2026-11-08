//! Factorizacion LU con pivoteo parcial: resolver `A x = b`, el determinante y la inversa.
//!
//! # La idea
//! Eliminacion gaussiana guardando los multiplicadores: `P A = L U`, con `L` triangular inferior (unos
//! en la diagonal), `U` triangular superior y `P` una permutacion de filas. Con ella, resolver
//! `A x = b` son dos pasadas baratas (sustitucion hacia adelante con `L`, hacia atras con `U`), y la
//! factorizacion, que es lo caro (O(n^3)), se hace una vez.
//!
//! # Pivoteo parcial
//! En cada columna se intercambia la fila actual con la de mayor valor absoluto de ahi para abajo.
//! Sin eso, un cero (o algo diminuto) en la diagonal da division por casi cero y resultados
//! absurdos: `[[0, 1], [1, 0]]` ni siquiera se podria factorizar.
//!
//! # Matrices singulares (N6)
//! Una matriz es **singular** si algun pivote cumple `|pivote| <= n * eps * max(|A|)` con `eps` el
//! epsilon de `f64`. [`lu`], [`solve`] e [`inverse`] la rechazan con [`Error::InvalidArgument`]; el
//! determinante de una matriz singular es `0`, no un error ([`det`]).
//!
//! # Memoria
//! `L` y `U` comparten una sola matriz de `n x n`: debajo de la diagonal van los multiplicadores y
//! de la diagonal para arriba, `U`.

use geoda_core::Error;

use crate::mat::Matrix;

/// Resultado de [`lu`].
#[derive(Debug, Clone, PartialEq)]
pub struct Lu {
    /// `L` (debajo de la diagonal) y `U` (desde la diagonal) juntas.
    packed: Matrix,
    /// `permutation[i]` es la fila de `A` que quedo en la fila `i`.
    permutation: Vec<usize>,
    /// `+1` o `-1` segun la paridad de la permutacion.
    sign: f64,
}

/// Factorizacion interna: `None` en el segundo valor si la matriz es singular.
fn factor(a: &Matrix) -> Result<(Lu, bool), Error> {
    let n = a.rows();
    if n != a.cols() {
        return Err(Error::LengthMismatch(format!(
            "la factorizacion LU pide una matriz cuadrada, no {} x {}",
            n,
            a.cols()
        )));
    }
    let mut packed = a.clone();
    let mut permutation: Vec<usize> = (0..n).collect();
    let mut sign = 1.0;
    let scale = a.data().iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    #[allow(clippy::cast_precision_loss)] // n es pequeno frente a 2^53
    let tolerance = n as f64 * f64::EPSILON * scale;
    let width = n;
    let mut singular = false;
    for col in 0..n {
        // Fila con el mayor valor absoluto de la columna, de `col` hacia abajo (la primera si empatan).
        let mut pivot_row = col;
        for row in col + 1..n {
            if packed.data()[row * width + col].abs() > packed.data()[pivot_row * width + col].abs()
            {
                pivot_row = row;
            }
        }
        let pivot = packed.data()[pivot_row * width + col];
        if pivot.abs() <= tolerance {
            singular = true;
            break;
        }
        if pivot_row != col {
            for j in 0..n {
                packed
                    .data_mut()
                    .swap(col * width + j, pivot_row * width + j);
            }
            permutation.swap(col, pivot_row);
            sign = -sign;
        }
        for row in col + 1..n {
            let factor = packed.data()[row * width + col] / pivot;
            packed.data_mut()[row * width + col] = factor;
            for j in col + 1..n {
                let upper = packed.data()[col * width + j];
                packed.data_mut()[row * width + j] -= factor * upper;
            }
        }
    }
    Ok((
        Lu {
            packed,
            permutation,
            sign,
        },
        singular,
    ))
}

/// Factoriza `P A = L U`.
///
/// # Errors
/// [`Error::LengthMismatch`] si `a` no es cuadrada; [`Error::InvalidArgument`] si es singular.
pub fn lu(a: &Matrix) -> Result<Lu, Error> {
    let (factorization, singular) = factor(a)?;
    if singular {
        return Err(singular_error());
    }
    Ok(factorization)
}

fn singular_error() -> Error {
    Error::InvalidArgument("la matriz es singular".to_string())
}

/// Resuelve `A x = b`.
///
/// # Errors
/// Los de [`lu`]; [`Error::LengthMismatch`] si `b.len() != a.rows()`.
pub fn solve(a: &Matrix, b: &[f64]) -> Result<Vec<f64>, Error> {
    lu(a)?.solve(b)
}

/// El determinante de `a`; `0` si es singular.
///
/// # Errors
/// [`Error::LengthMismatch`] si `a` no es cuadrada.
pub fn det(a: &Matrix) -> Result<f64, Error> {
    let (factorization, singular) = factor(a)?;
    Ok(if singular { 0.0 } else { factorization.det() })
}

/// La inversa de `a`.
///
/// # Errors
/// Los de [`lu`].
pub fn inverse(a: &Matrix) -> Result<Matrix, Error> {
    lu(a)?.inverse()
}

impl Lu {
    /// Tamano de la matriz (`n x n`).
    #[must_use]
    pub fn size(&self) -> usize {
        self.packed.rows()
    }

    /// La permutacion: la fila `i` de `P A` es la fila `permutation()[i]` de `A`.
    #[must_use]
    pub fn permutation(&self) -> &[usize] {
        &self.permutation
    }

    /// `L`, triangular inferior con unos en la diagonal.
    ///
    /// # Errors
    /// [`Error::Alloc`] si no hay memoria.
    pub fn l(&self) -> Result<Matrix, Error> {
        let n = self.size();
        let mut l = Matrix::identity(n)?;
        for i in 0..n {
            for j in 0..i {
                l.data_mut()[i * n + j] = self.packed.get(i, j);
            }
        }
        Ok(l)
    }

    /// `U`, triangular superior.
    ///
    /// # Errors
    /// [`Error::Alloc`] si no hay memoria.
    pub fn u(&self) -> Result<Matrix, Error> {
        let n = self.size();
        let mut u = Matrix::zeros(n, n)?;
        for i in 0..n {
            for j in i..n {
                u.data_mut()[i * n + j] = self.packed.get(i, j);
            }
        }
        Ok(u)
    }

    /// El determinante: el producto de la diagonal de `U` por el signo de la permutacion.
    #[must_use]
    pub fn det(&self) -> f64 {
        (0..self.size()).fold(self.sign, |acc, i| acc * self.packed.get(i, i))
    }

    /// Resuelve `A x = b` con la factorizacion ya hecha.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `b.len() != n`.
    pub fn solve(&self, b: &[f64]) -> Result<Vec<f64>, Error> {
        let n = self.size();
        if b.len() != n {
            return Err(Error::LengthMismatch(format!(
                "la matriz es de {n} x {n} y b tiene {} valores",
                b.len()
            )));
        }
        // L y = P b (hacia adelante; L tiene unos en la diagonal).
        let mut x: Vec<f64> = self.permutation.iter().map(|&row| b[row]).collect();
        for i in 0..n {
            for j in 0..i {
                x[i] -= self.packed.get(i, j) * x[j];
            }
        }
        // U x = y (hacia atras).
        for i in (0..n).rev() {
            for j in i + 1..n {
                x[i] -= self.packed.get(i, j) * x[j];
            }
            x[i] /= self.packed.get(i, i);
        }
        Ok(x)
    }

    /// La inversa: resuelve contra cada columna de la identidad.
    ///
    /// # Errors
    /// [`Error::Alloc`] si no hay memoria.
    pub fn inverse(&self) -> Result<Matrix, Error> {
        let n = self.size();
        let mut inverse = Matrix::zeros(n, n)?;
        let mut unit = vec![0.0; n];
        for col in 0..n {
            unit[col] = 1.0;
            let x = self.solve(&unit)?;
            unit[col] = 0.0;
            for (row, value) in x.into_iter().enumerate() {
                inverse.data_mut()[row * n + col] = value;
            }
        }
        Ok(inverse)
    }
}

#[cfg(test)]
mod tests {
    use super::{det, inverse, lu, solve};
    use crate::mat::Matrix;
    use geoda_core::Error;

    fn m(rows: &[&[f64]]) -> Matrix {
        Matrix::from_rows(&rows.iter().map(|r| r.to_vec()).collect::<Vec<_>>()).unwrap()
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn resuelve_un_sistema_de_2x2() {
        let x = solve(&m(&[&[2.0, 1.0], &[1.0, 3.0]]), &[3.0, 5.0]).unwrap();
        assert!(close(x[0], 0.8, 1e-12) && close(x[1], 1.4, 1e-12), "{x:?}");
    }

    #[test]
    fn el_pivoteo_resuelve_el_cero_en_la_diagonal() {
        let x = solve(&m(&[&[0.0, 1.0], &[1.0, 0.0]]), &[2.0, 3.0]).unwrap();
        assert_eq!(x, [3.0, 2.0]);
    }

    #[test]
    fn determinante_con_el_signo_de_la_permutacion() {
        let d = det(&m(&[&[4.0, 3.0], &[6.0, 3.0]])).unwrap();
        assert!(close(d, -6.0, 1e-12), "{d}");
        assert!(close(det(&Matrix::identity(4).unwrap()).unwrap(), 1.0, 0.0));
        assert!(close(
            det(&m(&[&[0.0, 1.0], &[1.0, 0.0]])).unwrap(),
            -1.0,
            0.0
        ));
    }

    #[test]
    fn la_inversa_por_la_matriz_es_la_identidad() {
        let a = m(&[&[4.0, 7.0, 2.0], &[3.0, 6.0, 1.0], &[2.0, 5.0, 3.0]]);
        let product = a.matmul(&inverse(&a).unwrap()).unwrap();
        let identity = Matrix::identity(3).unwrap();
        for (got, want) in product.data().iter().zip(identity.data()) {
            assert!(close(*got, *want, 1e-9), "{product:?}");
        }
    }

    #[test]
    fn singular_es_error_salvo_en_el_determinante() {
        let a = m(&[&[1.0, 2.0], &[2.0, 4.0]]);
        for result in [
            lu(&a).map(|_| ()),
            solve(&a, &[1.0, 2.0]).map(|_| ()),
            inverse(&a).map(|_| ()),
        ] {
            assert!(
                matches!(&result, Err(Error::InvalidArgument(m)) if m.contains("singular")),
                "{result:?}"
            );
        }
        assert_eq!(det(&a).unwrap(), 0.0);
        assert_eq!(det(&Matrix::zeros(3, 3).unwrap()).unwrap(), 0.0);
    }

    #[test]
    fn formas_que_no_encajan() {
        assert!(matches!(
            lu(&m(&[&[1.0, 2.0, 3.0]])),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            det(&m(&[&[1.0, 2.0, 3.0]])),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            solve(&Matrix::identity(2).unwrap(), &[1.0]),
            Err(Error::LengthMismatch(_))
        ));
    }

    #[test]
    fn matriz_de_1x1_y_vacia() {
        assert_eq!(solve(&m(&[&[4.0]]), &[2.0]).unwrap(), [0.5]);
        assert_eq!(det(&Matrix::zeros(0, 0).unwrap()).unwrap(), 1.0);
    }

    #[test]
    fn pa_es_igual_a_lu_y_el_residuo_es_chico() {
        let mut state: u64 = 0x1F;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 40).unwrap()) / 8_388_608.0 - 1.0
        };
        for n in 1..=12 {
            let a = Matrix::from_flat(n, n, (0..n * n).map(|_| next()).collect()).unwrap();
            let b: Vec<f64> = (0..n).map(|_| next()).collect();
            let Ok(factorization) = lu(&a) else { continue };
            // P A == L U
            let product = factorization
                .l()
                .unwrap()
                .matmul(&factorization.u().unwrap())
                .unwrap();
            for (row, &source) in factorization.permutation().iter().enumerate() {
                for col in 0..n {
                    assert!(
                        close(product.get(row, col), a.get(source, col), 1e-9),
                        "n = {n}"
                    );
                }
            }
            // Residuo relativo de A x = b.
            let x = factorization.solve(&b).unwrap();
            let ax = a.matvec(&x).unwrap();
            let residual = ax
                .iter()
                .zip(&b)
                .map(|(p, q)| (p - q).abs())
                .fold(0.0, f64::max);
            let scale = a.data().iter().fold(0.0_f64, |s, v| s.max(v.abs()))
                * x.iter().fold(0.0_f64, |s, v| s.max(v.abs()))
                + b.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
            assert!(residual <= 1e-9 * scale.max(1.0), "n = {n}: {residual}");
        }
    }
}
