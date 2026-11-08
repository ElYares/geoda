//! Descomposicion en valores singulares (SVD) por el metodo de Jacobi unilateral.
//!
//! # Que calcula
//! `A = U S V^T` para `A` de `m x n`, en su forma **delgada**: con `k = min(m, n)`, `U` es de `m x k`
//! y `V^T` de `k x n`, ambas de filas/columnas ortonormales, y `S` son los `k` valores singulares,
//! de mayor a menor. Sirve para el rango, las aproximaciones de bajo rango y el PCA.
//!
//! # El metodo (Hestenes)
//! Se parte de `U = A` y `V = I`. Para cada par de columnas `p, q` de `U` se calcula la rotacion que
//! las vuelve ortogonales entre si y se aplica a `U` y a `V`. Se repite por barridos hasta que todas las
//! columnas son ortogonales. Entonces las normas de las columnas de `U` son los valores singulares y
//! `U` normalizada, la `U` de la descomposicion. Es lento frente a Golub-Kahan pero corto, muy preciso y
//! facil de verificar (decision 014, N5).
//!
//! Con mas columnas que filas se trabaja con la transpuesta (`A^T = V S U^T`).
//!
//! # Convergencia
//! Se para cuando en un barrido ninguna pareja supera `|u_p . u_q| <= 1e-15 |u_p| |u_q|`. Tope de 60
//! barridos: si no converge es [`Error::InvalidArgument`], no un resultado a medias.
//!
//! # Valores singulares nulos
//! Si `s_j <= max(m, n) * eps * s_max` la direccion de `U` no esta determinada por los datos (el vector
//! normalizado seria ruido): se sustituye por un vector unitario ortogonal a los demas, elegido de
//! forma determinista (la base canonica con mayor componente restante), para que `U` siga siendo
//! ortonormal.
//!
//! # Signo (N4)
//! Cada par `(u_j, v_j)` admite el signo contrario. Se fija: en cada columna de `V` la entrada de mayor
//! valor absoluto es positiva (la primera si hay empate). Asi el resultado es reproducible y comparable.

use geoda_core::Error;

use crate::mat::Matrix;

const MAX_SWEEPS: usize = 60;
const ORTHOGONAL: f64 = 1e-15;

/// Resultado de [`svd`].
#[derive(Debug, Clone, PartialEq)]
pub struct Svd {
    /// `m x k`, columnas ortonormales.
    pub u: Matrix,
    /// Los `k` valores singulares, de mayor a menor.
    pub s: Vec<f64>,
    /// `k x n`, filas ortonormales.
    pub vt: Matrix,
}

/// SVD delgada de `matrix`.
///
/// # Errors
/// [`Error::InvalidArgument`] si no converge en 60 barridos (no deberia pasar con datos finitos).
pub fn svd(matrix: &Matrix) -> Result<Svd, Error> {
    let (rows, cols) = (matrix.rows(), matrix.cols());
    if rows < cols {
        // A^T = V S U^T  =>  A = U S V^T con U y V intercambiadas.
        let transposed = svd(&matrix.transpose()?)?;
        return Ok(Svd {
            u: transposed.vt.transpose()?,
            s: transposed.s,
            vt: transposed.u.transpose()?,
        });
    }
    let mut u = matrix.clone();
    let mut v = Matrix::identity(cols)?;
    let mut converged = false;
    for _ in 0..MAX_SWEEPS {
        let mut rotated = false;
        for p in 0..cols {
            for q in p + 1..cols {
                let (mut alpha, mut beta, mut gamma) = (0.0, 0.0, 0.0);
                for i in 0..rows {
                    let (up, uq) = (u.get(i, p), u.get(i, q));
                    alpha += up * up;
                    beta += uq * uq;
                    gamma += up * uq;
                }
                if gamma == 0.0 || gamma.abs() <= ORTHOGONAL * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let tangent = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let cosine = 1.0 / (1.0 + tangent * tangent).sqrt();
                let sine = cosine * tangent;
                rotate(&mut u, p, q, cosine, sine);
                rotate(&mut v, p, q, cosine, sine);
            }
        }
        if !rotated {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(Error::InvalidArgument(format!(
            "la SVD no convergio en {MAX_SWEEPS} barridos"
        )));
    }

    // Valores singulares = normas de las columnas; orden descendente (estable).
    let norms: Vec<f64> = (0..cols)
        .map(|j| (0..rows).map(|i| u.get(i, j).powi(2)).sum::<f64>().sqrt())
        .collect();
    let mut order: Vec<usize> = (0..cols).collect();
    order.sort_by(|&a, &b| norms[b].total_cmp(&norms[a]));
    let largest = order.first().map_or(0.0, |&j| norms[j]);
    #[allow(clippy::cast_precision_loss)] // las dimensiones son pequenas frente a 2^53
    let zero_below = rows.max(cols) as f64 * f64::EPSILON * largest;

    let mut ordered_u = Matrix::zeros(rows, cols)?;
    let mut ordered_v = Matrix::zeros(cols, cols)?;
    let mut singular = Vec::with_capacity(cols);
    let mut undetermined = Vec::new();
    for (new, &old) in order.iter().enumerate() {
        singular.push(norms[old]);
        for i in 0..cols {
            ordered_v.data_mut()[i * cols + new] = v.get(i, old);
        }
        if norms[old] > zero_below {
            for i in 0..rows {
                ordered_u.data_mut()[i * cols + new] = u.get(i, old) / norms[old];
            }
        } else {
            undetermined.push(new);
        }
    }
    for &column in &undetermined {
        complete_column(&mut ordered_u, column);
    }

    // Signo: la entrada de mayor valor absoluto de cada columna de V es positiva.
    for j in 0..cols {
        let mut biggest = 0;
        for i in 1..cols {
            if ordered_v.get(i, j).abs() > ordered_v.get(biggest, j).abs() {
                biggest = i;
            }
        }
        if ordered_v.get(biggest, j) < 0.0 {
            for i in 0..cols {
                ordered_v.data_mut()[i * cols + j] = -ordered_v.get(i, j);
            }
            for i in 0..rows {
                ordered_u.data_mut()[i * cols + j] = -ordered_u.get(i, j);
            }
        }
    }
    Ok(Svd {
        u: ordered_u,
        s: singular,
        vt: ordered_v.transpose()?,
    })
}

/// Rota las columnas `p` y `q` de `matrix`.
fn rotate(matrix: &mut Matrix, p: usize, q: usize, cosine: f64, sine: f64) {
    let cols = matrix.cols();
    for i in 0..matrix.rows() {
        let (a, b) = (matrix.get(i, p), matrix.get(i, q));
        matrix.data_mut()[i * cols + p] = cosine * a - sine * b;
        matrix.data_mut()[i * cols + q] = sine * a + cosine * b;
    }
}

/// Pone en `column` un vector unitario ortogonal a las demas columnas ya determinadas.
///
/// Se prueba cada vector de la base canonica, se le quita su proyeccion sobre las columnas de `U`
/// (dos veces, para limpiar el redondeo) y se queda el de mayor resto.
fn complete_column(matrix: &mut Matrix, column: usize) {
    let (rows, cols) = (matrix.rows(), matrix.cols());
    let mut best: Option<(f64, Vec<f64>)> = None;
    for axis in 0..rows {
        let mut candidate = vec![0.0; rows];
        candidate[axis] = 1.0;
        for _ in 0..2 {
            for other in 0..cols {
                if other == column || matrix.column(other).iter().all(|&x| x == 0.0) {
                    continue;
                }
                let dot: f64 = (0..rows).map(|i| candidate[i] * matrix.get(i, other)).sum();
                for (i, value) in candidate.iter_mut().enumerate() {
                    *value -= dot * matrix.get(i, other);
                }
            }
        }
        let norm = candidate.iter().map(|x| x * x).sum::<f64>().sqrt();
        if best.as_ref().is_none_or(|(b, _)| norm > *b) {
            best = Some((norm, candidate));
        }
    }
    if let Some((norm, candidate)) = best
        && norm > 0.0
    {
        for (i, value) in candidate.into_iter().enumerate() {
            matrix.data_mut()[i * cols + column] = value / norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::svd;
    use crate::lu::det;
    use crate::mat::Matrix;

    fn m(rows: &[&[f64]]) -> Matrix {
        Matrix::from_rows(&rows.iter().map(|r| r.to_vec()).collect::<Vec<_>>()).unwrap()
    }

    fn close(a: &Matrix, b: &Matrix, tol: f64) {
        assert_eq!((a.rows(), a.cols()), (b.rows(), b.cols()));
        for (x, y) in a.data().iter().zip(b.data()) {
            assert!((x - y).abs() <= tol, "{x} contra {y}");
        }
    }

    fn reconstruct(f: &super::Svd) -> Matrix {
        let k = f.s.len();
        let mut scaled = f.u.clone();
        for j in 0..k {
            for i in 0..scaled.rows() {
                let value = scaled.get(i, j) * f.s[j];
                scaled.data_mut()[i * k + j] = value;
            }
        }
        scaled.matmul(&f.vt).unwrap()
    }

    #[test]
    fn matriz_diagonal() {
        let a = m(&[&[1.0, 0.0, 0.0], &[0.0, 3.0, 0.0], &[0.0, 0.0, 2.0]]);
        let f = svd(&a).unwrap();
        assert_eq!(f.s, [3.0, 2.0, 1.0]);
        close(&reconstruct(&f), &a, 1e-15);
    }

    #[test]
    fn reconstruye_y_es_ortonormal() {
        let a = m(&[&[1.0, 2.0], &[3.0, 4.0], &[5.0, 6.0], &[7.0, 9.0]]);
        let f = svd(&a).unwrap();
        close(&reconstruct(&f), &a, 1e-9);
        close(
            &f.u.transpose().unwrap().matmul(&f.u).unwrap(),
            &Matrix::identity(2).unwrap(),
            1e-9,
        );
        close(
            &f.vt.matmul(&f.vt.transpose().unwrap()).unwrap(),
            &Matrix::identity(2).unwrap(),
            1e-9,
        );
        assert!(f.s[0] >= f.s[1] && f.s[1] > 0.0);
    }

    #[test]
    fn mas_columnas_que_filas() {
        let a = m(&[&[1.0, 2.0, 3.0, 4.0], &[0.5, -1.0, 2.0, 0.0]]);
        let f = svd(&a).unwrap();
        assert_eq!(
            (f.u.rows(), f.u.cols(), f.s.len(), f.vt.rows(), f.vt.cols()),
            (2, 2, 2, 2, 4)
        );
        close(&reconstruct(&f), &a, 1e-9);
    }

    #[test]
    fn rango_uno() {
        let f = svd(&m(&[&[1.0, 2.0], &[2.0, 4.0]])).unwrap();
        assert!((f.s[0] - 5.0).abs() < 1e-12, "{:?}", f.s);
        assert!(f.s[1].abs() < 1e-12, "{:?}", f.s);
        // U sigue siendo ortonormal con un valor singular nulo.
        close(
            &f.u.transpose().unwrap().matmul(&f.u).unwrap(),
            &Matrix::identity(2).unwrap(),
            1e-9,
        );
    }

    #[test]
    fn columna_de_ceros() {
        let a = m(&[&[1.0, 0.0, 2.0], &[2.0, 0.0, 1.0], &[0.0, 0.0, 3.0]]);
        let f = svd(&a).unwrap();
        close(&reconstruct(&f), &a, 1e-9);
        close(
            &f.u.transpose().unwrap().matmul(&f.u).unwrap(),
            &Matrix::identity(3).unwrap(),
            1e-9,
        );
        assert!(f.s[2].abs() < 1e-12);
    }

    #[test]
    fn es_determinista_y_el_signo_sigue_la_convencion() {
        let a = m(&[
            &[2.0, -1.0, 0.5],
            &[-3.0, 4.0, 1.0],
            &[1.0, 1.0, -2.0],
            &[0.0, 2.0, 2.0],
        ]);
        let f = svd(&a).unwrap();
        for _ in 0..3 {
            assert_eq!(svd(&a).unwrap(), f);
        }
        for j in 0..f.s.len() {
            let column: Vec<f64> = (0..f.vt.cols().min(f.vt.rows()))
                .map(|c| f.vt.get(j, c))
                .collect();
            let big = column
                .iter()
                .copied()
                .fold(0.0_f64, |b, x| if x.abs() > b.abs() { x } else { b });
            assert!(big > 0.0, "fila {j} de V^T: {column:?}");
        }
    }

    #[test]
    fn la_suma_de_cuadrados_y_el_determinante() {
        let mut state: u64 = 0x5D;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 40).unwrap()) / 8_388_608.0 - 1.0
        };
        for (rows, cols) in [(1, 1), (2, 2), (3, 5), (6, 3), (8, 8), (12, 4), (5, 9)] {
            let a =
                Matrix::from_flat(rows, cols, (0..rows * cols).map(|_| next()).collect()).unwrap();
            let f = svd(&a).unwrap();
            close(&reconstruct(&f), &a, 1e-9);
            let frobenius: f64 = a.data().iter().map(|x| x * x).sum();
            let squares: f64 = f.s.iter().map(|s| s * s).sum();
            assert!(
                (frobenius - squares).abs() < 1e-9 * frobenius.max(1.0),
                "{rows}x{cols}"
            );
            assert!(f.s.windows(2).all(|w| w[0] >= w[1]));
            if rows == cols {
                let product: f64 = f.s.iter().product();
                assert!(
                    (product - det(&a).unwrap().abs()).abs() < 1e-9,
                    "{rows}x{cols}"
                );
            }
        }
    }
}
