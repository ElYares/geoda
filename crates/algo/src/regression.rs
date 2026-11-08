//! Regresion lineal por minimos cuadrados: `y = b0 + b1 x1 + ... + bd xd`.
//!
//! # La idea
//! Se busca el vector de coeficientes que minimiza la suma de los residuos al cuadrado
//! `sum (y_i - prediccion_i)^2`. Se arma la **matriz de diseno** `[1, x1, ..., xd]` (la columna de
//! unos da el intercepto) y se resuelve con [`least_squares`] (QR de Householder, decision 014, N7):
//! es estable donde las ecuaciones normales perderian la mitad de las cifras.
//!
//! # Intercepto
//! Con `fit_intercept` (lo normal) el modelo incluye `b0`; sin el, la recta pasa por el origen.
//!
//! # R^2
//! `R^2 = 1 - SS_res / SS_tot`, con `SS_tot` la suma de cuadrados de `y` respecto a su **media**
//! (tambien sin intercepto, donde puede salir negativo: el modelo es peor que predecir la media). Si
//! `y` es constante `SS_tot` es 0: se define `R^2 = 1` cuando el residuo es 0 (dentro de redondeo) y
//! `0` si no.
//!
//! # Rango incompleto
//! Si dos variables son colineales (una combinacion lineal de las otras) los coeficientes no estan
//! unicamente determinados: [`Error::InvalidArgument`] (`rango incompleto`), sin pseudoinversa. Tambien
//! hacen falta al menos tantas filas como coeficientes.

use geoda_core::{Column, Error};

use crate::mat::Matrix;
use crate::matrix::Dataset;
use crate::qr::least_squares;

/// Modelo ajustado por [`fit_regression`].
#[derive(Debug, Clone, PartialEq)]
pub struct LinearRegression {
    /// Un coeficiente por variable, en el orden de las columnas.
    pub coefficients: Vec<f64>,
    /// El intercepto `b0` (`0` si se ajusto sin intercepto).
    pub intercept: f64,
    /// Coeficiente de determinacion del ajuste.
    pub r_squared: f64,
}

/// Ajusta un modelo con una lista de columnas (una por variable) y la columna `y`.
///
/// # Errors
/// Los de [`Dataset::from_columns`] (nulos, `NaN`, tipos) y [`fit_regression`].
pub fn fit_regression_columns(
    columns: &[&Column],
    y: &Column,
    fit_intercept: bool,
) -> Result<LinearRegression, Error> {
    let data = Dataset::from_columns(columns)?;
    let target = Dataset::from_columns(&[y])?;
    let y: Vec<f64> = (0..target.rows()).map(|i| target.get(i, 0)).collect();
    fit_regression(&data, &y, fit_intercept)
}

/// Ajusta `y = b0 + sum(bi * xi)` por minimos cuadrados.
///
/// # Errors
/// [`Error::LengthMismatch`] si `y` no mide lo mismo que las filas; [`Error::InvalidArgument`] si hay
/// menos filas que coeficientes o las variables son colineales (`rango incompleto`).
pub fn fit_regression(
    data: &Dataset,
    y: &[f64],
    fit_intercept: bool,
) -> Result<LinearRegression, Error> {
    let (rows, vars) = (data.rows(), data.cols());
    if y.len() != rows {
        return Err(Error::LengthMismatch(format!(
            "hay {rows} filas y {} valores de y",
            y.len()
        )));
    }
    if let Some(at) = y.iter().position(|v| !v.is_finite()) {
        return Err(Error::InvalidArgument(format!("y[{at}] no es finito")));
    }
    let width = vars + usize::from(fit_intercept);
    if rows < width {
        return Err(Error::InvalidArgument(format!(
            "hacen falta al menos {width} filas para {width} coeficientes, hay {rows}"
        )));
    }
    let mut design = Matrix::zeros(rows, width)?;
    for i in 0..rows {
        if fit_intercept {
            design.data_mut()[i * width] = 1.0;
        }
        for j in 0..vars {
            design.data_mut()[i * width + usize::from(fit_intercept) + j] = data.get(i, j);
        }
    }
    let fit = least_squares(&design, y)?;
    let (intercept, coefficients) = if fit_intercept {
        (fit.coefficients[0], fit.coefficients[1..].to_vec())
    } else {
        (0.0, fit.coefficients)
    };
    #[allow(clippy::cast_precision_loss)] // las filas caben de sobra en 2^53
    let mean = y.iter().sum::<f64>() / rows as f64;
    let total: f64 = y.iter().map(|v| (v - mean).powi(2)).sum();
    let residual = fit.residual_norm.powi(2);
    let r_squared = if total == 0.0 {
        if fit.residual_norm <= 1e-12 * mean.abs().max(1.0) {
            1.0
        } else {
            0.0
        }
    } else {
        1.0 - residual / total
    };
    Ok(LinearRegression {
        coefficients,
        intercept,
        r_squared,
    })
}

impl LinearRegression {
    /// Predice `b0 + sum(bi * xi)` para cada fila.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `data` no tiene tantas variables como coeficientes.
    pub fn predict(&self, data: &Dataset) -> Result<Vec<f64>, Error> {
        if data.cols() != self.coefficients.len() {
            return Err(Error::LengthMismatch(format!(
                "el modelo tiene {} variables y llegan {}",
                self.coefficients.len(),
                data.cols()
            )));
        }
        Ok((0..data.rows())
            .map(|i| {
                self.coefficients
                    .iter()
                    .zip(data.row(i))
                    .fold(self.intercept, |acc, (b, x)| acc + b * x)
            })
            .collect())
    }

    /// Como [`LinearRegression::predict`] sobre una lista de columnas.
    ///
    /// # Errors
    /// Los de [`Dataset::from_columns`] y [`LinearRegression::predict`].
    pub fn predict_columns(&self, columns: &[&Column]) -> Result<Vec<f64>, Error> {
        self.predict(&Dataset::from_columns(columns)?)
    }
}

#[cfg(test)]
mod tests {
    use super::{fit_regression, fit_regression_columns};
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
    fn una_recta_exacta() {
        let x = floats(&[0.0, 1.0, 2.0, 3.0]);
        let fit = fit_regression_columns(&[&x], &floats(&[1.0, 3.0, 5.0, 7.0]), true).unwrap();
        assert!((fit.intercept - 1.0).abs() < 1e-12);
        assert!((fit.coefficients[0] - 2.0).abs() < 1e-12);
        assert!((fit.r_squared - 1.0).abs() < 1e-12);
    }

    #[test]
    fn con_ruido_r2_entre_0_y_1_y_los_residuos_suman_cero() {
        let x = floats(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        let y = [1.1, 2.8, 5.3, 6.9, 9.2, 10.8];
        let fit = fit_regression_columns(&[&x], &floats(&y), true).unwrap();
        assert!(
            fit.r_squared > 0.9 && fit.r_squared < 1.0,
            "{}",
            fit.r_squared
        );
        let predicted = fit.predict_columns(&[&x]).unwrap();
        let residual_sum: f64 = y.iter().zip(&predicted).map(|(a, b)| a - b).sum();
        assert!(residual_sum.abs() < 1e-9, "{residual_sum}");
    }

    #[test]
    fn sin_intercepto() {
        let x = floats(&[1.0, 2.0, 3.0]);
        let fit = fit_regression_columns(&[&x], &floats(&[2.0, 4.0, 6.0]), false).unwrap();
        assert_eq!(fit.intercept, 0.0);
        assert!((fit.coefficients[0] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn dos_variables_y_prediccion() {
        let (x1, x2) = (
            floats(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            floats(&[2.0, 1.0, 4.0, 3.0, 7.0]),
        );
        // y = 1 + 2 x1 - 3 x2 exacto.
        let y: Vec<f64> = [1.0, 2.0, 3.0, 4.0, 5.0]
            .iter()
            .zip([2.0, 1.0, 4.0, 3.0, 7.0])
            .map(|(a, b)| 1.0 + 2.0 * a - 3.0 * b)
            .collect();
        let fit = fit_regression_columns(&[&x1, &x2], &floats(&y), true).unwrap();
        assert!((fit.intercept - 1.0).abs() < 1e-9);
        assert!(
            (fit.coefficients[0] - 2.0).abs() < 1e-9 && (fit.coefficients[1] + 3.0).abs() < 1e-9
        );
        let new = fit
            .predict_columns(&[&floats(&[10.0]), &floats(&[1.0])])
            .unwrap();
        assert!((new[0] - (1.0 + 20.0 - 3.0)).abs() < 1e-9);
    }

    #[test]
    fn variables_colineales() {
        let x = floats(&[1.0, 2.0, 3.0, 4.0]);
        let doubled = floats(&[2.0, 4.0, 6.0, 8.0]);
        let err = fit_regression_columns(&[&x, &doubled], &floats(&[1.0, 2.0, 3.0, 5.0]), true)
            .unwrap_err();
        assert!(
            matches!(&err, Error::InvalidArgument(m) if m.contains("rango incompleto")),
            "{err}"
        );
    }

    #[test]
    fn menos_filas_que_coeficientes() {
        let x = floats(&[1.0, 2.0]);
        let err = fit_regression_columns(&[&x, &x, &x], &floats(&[1.0, 2.0]), true).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)), "{err}");
    }

    #[test]
    fn y_constante() {
        let x = floats(&[1.0, 2.0, 3.0, 4.0]);
        let fit = fit_regression_columns(&[&x], &floats(&[5.0, 5.0, 5.0, 5.0]), true).unwrap();
        assert!((fit.intercept - 5.0).abs() < 1e-12 && fit.coefficients[0].abs() < 1e-12);
        assert_eq!(fit.r_squared, 1.0);
    }

    #[test]
    fn entradas_invalidas() {
        let x = floats(&[1.0, 2.0, 3.0]);
        assert!(matches!(
            fit_regression_columns(&[&x], &floats(&[1.0, 2.0]), true),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            fit_regression_columns(&[&x], &floats(&[1.0, f64::NAN, 3.0]), true),
            Err(Error::InvalidArgument(_))
        ));
        let mut with_null = PrimitiveBuilder::<f64>::new();
        with_null.append(1.0);
        with_null.append_null();
        with_null.append(3.0);
        assert!(matches!(
            fit_regression_columns(&[&x], &with_null.finish().unwrap(), true),
            Err(Error::InvalidArgument(_))
        ));
        let fit = fit_regression_columns(&[&x], &floats(&[1.0, 2.0, 3.0]), true).unwrap();
        assert!(matches!(
            fit.predict_columns(&[&x, &x]),
            Err(Error::LengthMismatch(_))
        ));
    }

    #[test]
    fn coincide_con_la_formula_de_la_regresion_simple() {
        let xs = [1.0, 2.0, 4.0, 7.0, 8.0, 11.0];
        let ys = [2.0, 3.5, 5.0, 9.5, 9.0, 14.0];
        let n = 6.0;
        let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
        let slope = xs
            .iter()
            .zip(&ys)
            .map(|(a, b)| (a - mx) * (b - my))
            .sum::<f64>()
            / xs.iter().map(|a| (a - mx).powi(2)).sum::<f64>();
        let data = Dataset::from_columns(&[&floats(&xs)]).unwrap();
        let fit = fit_regression(&data, &ys, true).unwrap();
        assert!((fit.coefficients[0] - slope).abs() < 1e-12);
        assert!((fit.intercept - (my - slope * mx)).abs() < 1e-12);
    }
}
