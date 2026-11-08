//! Analisis de componentes principales (PCA) con la SVD.
//!
//! # La idea
//! Encontrar las direcciones del espacio de variables en las que los datos varian mas. La primera
//! (componente principal) es la de mayor varianza; la segunda, la de mayor varianza entre las
//! ortogonales a la primera, y asi. Proyectar los datos sobre las primeras `k` reduce la dimension
//! perdiendo la menor varianza posible.
//!
//! # Como se calcula (N8)
//! 1. Se **centra**: a cada columna se le resta su media (no se escala: las variables en unidades
//!    distintas pesan distinto, como en `sklearn.decomposition.PCA`).
//! 2. Se calcula la SVD de los datos centrados `X = U S V^T`. Las filas de `V^T` son los componentes.
//! 3. La varianza explicada por el componente `i` es `s_i^2 / (n - 1)` (varianza muestral) y la razon,
//!    esa varianza entre la total (la suma de las de todas las direcciones).
//!
//! Se usa la SVD de `X` y no la de la matriz de covarianza porque esta ultima eleva al cuadrado el
//! numero de condicion. Los signos siguen la convencion de [`svd`](crate::svd::svd) (N4).
//!
//! # Casos limite
//! - `n_components` debe estar entre 1 y `min(n, d)`; hace falta `n >= 2` filas.
//! - Si todos los datos son iguales la varianza total es 0 y las razones son 0 (no `NaN`).

use geoda_core::{Column, Error};

use crate::mat::Matrix;
use crate::matrix::Dataset;
use crate::svd::svd;

/// Modelo ajustado por [`fit_pca`].
#[derive(Debug, Clone, PartialEq)]
pub struct Pca {
    /// Los componentes, uno por fila (`k x d`), ortonormales.
    pub components: Matrix,
    /// Varianza explicada por cada componente (`s^2 / (n - 1)`), de mayor a menor.
    pub explained_variance: Vec<f64>,
    /// Fraccion de la varianza total que explica cada componente.
    pub explained_variance_ratio: Vec<f64>,
    /// Media de cada variable (con la que se centra).
    pub mean: Vec<f64>,
}

/// Ajusta un PCA con una lista de columnas (una por variable).
///
/// # Errors
/// Los de [`Dataset::from_columns`] y [`fit_pca`].
pub fn fit_pca_columns(columns: &[&Column], n_components: usize) -> Result<Pca, Error> {
    fit_pca(&Dataset::from_columns(columns)?, n_components)
}

/// Ajusta un PCA de `n_components` componentes.
///
/// # Errors
/// [`Error::InvalidArgument`] si hay menos de 2 filas o `n_components` no esta entre 1 y `min(n, d)`.
pub fn fit_pca(data: &Dataset, n_components: usize) -> Result<Pca, Error> {
    let (rows, vars) = (data.rows(), data.cols());
    if rows < 2 {
        return Err(Error::InvalidArgument(format!(
            "el PCA necesita al menos 2 filas, hay {rows}"
        )));
    }
    let limit = rows.min(vars);
    if n_components == 0 || n_components > limit {
        return Err(Error::InvalidArgument(format!(
            "n_components = {n_components} debe estar entre 1 y min(filas, variables) = {limit}"
        )));
    }
    #[allow(clippy::cast_precision_loss)] // las filas caben de sobra en 2^53
    let count = rows as f64;
    let mean: Vec<f64> = (0..vars)
        .map(|j| (0..rows).map(|i| data.get(i, j)).sum::<f64>() / count)
        .collect();
    let mut centered = Matrix::zeros(rows, vars)?;
    for i in 0..rows {
        for (j, average) in mean.iter().enumerate() {
            centered.data_mut()[i * vars + j] = data.get(i, j) - average;
        }
    }
    let decomposition = svd(&centered)?;
    let variance: Vec<f64> = decomposition
        .s
        .iter()
        .map(|s| s * s / (count - 1.0))
        .collect();
    let total: f64 = variance.iter().sum();
    let mut components = Matrix::zeros(n_components, vars)?;
    for i in 0..n_components {
        for j in 0..vars {
            components.data_mut()[i * vars + j] = decomposition.vt.get(i, j);
        }
    }
    let explained_variance: Vec<f64> = variance[..n_components].to_vec();
    let explained_variance_ratio = explained_variance
        .iter()
        .map(|v| if total > 0.0 { v / total } else { 0.0 })
        .collect();
    Ok(Pca {
        components,
        explained_variance,
        explained_variance_ratio,
        mean,
    })
}

impl Pca {
    /// Proyecta los datos sobre los componentes: `(X - media) * componentes^T`, de `n x k`.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `data` no tiene tantas variables como el ajuste.
    pub fn transform(&self, data: &Dataset) -> Result<Matrix, Error> {
        let vars = self.mean.len();
        if data.cols() != vars {
            return Err(Error::LengthMismatch(format!(
                "el PCA se ajusto con {vars} variables y llegan {}",
                data.cols()
            )));
        }
        let k = self.components.rows();
        let mut projected = Matrix::zeros(data.rows(), k)?;
        for i in 0..data.rows() {
            for c in 0..k {
                let value: f64 = (0..vars)
                    .map(|j| (data.get(i, j) - self.mean[j]) * self.components.get(c, j))
                    .sum();
                projected.data_mut()[i * k + c] = value;
            }
        }
        Ok(projected)
    }

    /// Como [`Pca::transform`] sobre una lista de columnas.
    ///
    /// # Errors
    /// Los de [`Dataset::from_columns`] y [`Pca::transform`].
    pub fn transform_columns(&self, columns: &[&Column]) -> Result<Matrix, Error> {
        self.transform(&Dataset::from_columns(columns)?)
    }
}

#[cfg(test)]
mod tests {
    use super::{fit_pca, fit_pca_columns};
    use crate::matrix::Dataset;
    use geoda_core::{Column, Error, PrimitiveBuilder};

    fn floats(values: &[f64]) -> Column {
        let mut b = PrimitiveBuilder::<f64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    #[test]
    fn dos_variables_perfectamente_correlacionadas() {
        let x = floats(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let doubled = floats(&[2.0, 4.0, 6.0, 8.0, 10.0]);
        let pca = fit_pca_columns(&[&x, &doubled], 2).unwrap();
        assert!((pca.explained_variance_ratio[0] - 1.0).abs() < 1e-12);
        assert!(pca.explained_variance_ratio[1].abs() < 1e-12);
        // El primer componente apunta a (1, 2)/sqrt(5), con la mayor entrada positiva.
        let norm = 5.0_f64.sqrt();
        assert!((pca.components.get(0, 0) - 1.0 / norm).abs() < 1e-12);
        assert!((pca.components.get(0, 1) - 2.0 / norm).abs() < 1e-12);
        assert_eq!(pca.mean, [3.0, 6.0]);
    }

    fn sample() -> (Column, Column, Column) {
        (
            floats(&[2.5, 0.5, 2.2, 1.9, 3.1, 2.3, 2.0, 1.0, 1.5, 1.1]),
            floats(&[2.4, 0.7, 2.9, 2.2, 3.0, 2.7, 1.6, 1.1, 1.6, 0.9]),
            floats(&[1.0, 0.2, 0.9, 0.5, 1.4, 1.1, 0.3, 0.2, 0.6, 0.1]),
        )
    }

    #[test]
    fn varianza_explicada_y_razones() {
        let (a, b, c) = sample();
        let pca = fit_pca_columns(&[&a, &b, &c], 3).unwrap();
        assert!(pca.explained_variance.windows(2).all(|w| w[0] >= w[1]));
        let ratio_sum: f64 = pca.explained_variance_ratio.iter().sum();
        assert!((ratio_sum - 1.0).abs() < 1e-12, "{ratio_sum}");
        // La varianza total es la suma de las varianzas muestrales de cada variable.
        let total: f64 = [&a, &b, &c]
            .iter()
            .map(|col| {
                let v = col.values::<f64>().unwrap();
                let mean = v.iter().sum::<f64>() / 10.0;
                v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 9.0
            })
            .sum();
        assert!((pca.explained_variance.iter().sum::<f64>() - total).abs() < 1e-12);
    }

    #[test]
    fn la_proyeccion_tiene_media_cero_y_varianzas_explicadas() {
        let (a, b, c) = sample();
        let pca = fit_pca_columns(&[&a, &b, &c], 3).unwrap();
        let projected = pca.transform_columns(&[&a, &b, &c]).unwrap();
        for k in 0..3 {
            let column = projected.column(k);
            let mean = column.iter().sum::<f64>() / 10.0;
            let variance = column.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 9.0;
            assert!(mean.abs() < 1e-9, "media {mean}");
            assert!((variance - pca.explained_variance[k]).abs() < 1e-9);
        }
    }

    #[test]
    fn menos_componentes() {
        let (a, b, c) = sample();
        let pca = fit_pca_columns(&[&a, &b, &c], 2).unwrap();
        assert_eq!((pca.components.rows(), pca.components.cols()), (2, 3));
        assert_eq!(pca.explained_variance.len(), 2);
        assert!(pca.explained_variance_ratio.iter().sum::<f64>() < 1.0);
        assert_eq!(pca.transform_columns(&[&a, &b, &c]).unwrap().cols(), 2);
    }

    #[test]
    fn es_determinista() {
        let (a, b, c) = sample();
        let first = fit_pca_columns(&[&a, &b, &c], 2).unwrap();
        for _ in 0..3 {
            assert_eq!(fit_pca_columns(&[&a, &b, &c], 2).unwrap(), first);
        }
    }

    #[test]
    fn argumentos_invalidos() {
        let (a, b, _) = sample();
        for k in [0, 3] {
            assert!(matches!(
                fit_pca_columns(&[&a, &b], k),
                Err(Error::InvalidArgument(_))
            ));
        }
        assert!(matches!(
            fit_pca_columns(&[&floats(&[1.0])], 1),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            fit_pca_columns(&[&floats(&[1.0, f64::NAN, 3.0])], 1),
            Err(Error::InvalidArgument(_))
        ));
        let pca = fit_pca_columns(&[&a, &b], 2).unwrap();
        assert!(matches!(
            pca.transform_columns(&[&a]),
            Err(Error::LengthMismatch(_))
        ));
    }

    #[test]
    fn datos_constantes_dan_razones_cero() {
        let same = floats(&[4.0, 4.0, 4.0]);
        let pca = fit_pca_columns(&[&same], 1).unwrap();
        assert_eq!(pca.explained_variance, [0.0]);
        assert_eq!(pca.explained_variance_ratio, [0.0]);
    }

    #[test]
    fn mas_variables_que_filas() {
        // 3 filas, 5 variables: como mucho 3 componentes.
        let cols: Vec<Column> = (0..5)
            .map(|j| {
                floats(&[
                    f64::from(j),
                    f64::from(j * j) + 1.0,
                    (f64::from(j) * 0.7).sin(),
                ])
            })
            .collect();
        let refs: Vec<&Column> = cols.iter().collect();
        let data = Dataset::from_columns(&refs).unwrap();
        assert!(fit_pca(&data, 3).is_ok());
        assert!(matches!(fit_pca(&data, 4), Err(Error::InvalidArgument(_))));
    }
}
