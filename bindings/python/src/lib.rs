//! Binding de Python para geoda: `version`, `column_len`, `median` y `percentile`.
//!
//! Depende de `geoda-core` y `geoda-algo` directamente (D2). El contrato de la C-API lo ejercita el
//! runner de spec, que carga `libgeoda` por `ctypes`; este modulo no pasa por ahi.

// Los macros de PyO3 generan `unsafe`: el binding es frontera, como `capi`.
#![allow(unsafe_code)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use geoda_algo::{Method, median_column, percentile_column};
use geoda_core::{Column, Error, PrimitiveBuilder};

/// Version de geoda. Todos los crates del workspace comparten version, asi que
/// coincide con la de `geoda-capi`.
#[pyfunction]
fn version() -> &'static str {
    geoda_core::version()
}

/// Longitud de una columna de enteros construida con una lista (`None` = nulo).
#[pyfunction]
fn column_len(values: Vec<Option<i64>>) -> PyResult<usize> {
    let mut builder = PrimitiveBuilder::<i64>::new();
    for value in values {
        match value {
            Some(v) => builder.append(v),
            None => builder.append_null(),
        }
    }
    let column = builder
        .finish()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(column.len())
}

fn value_error(e: &Error) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Columna `Float64` con una lista (`None` = nulo). Los enteros se convierten a `float`.
fn float_column(values: Vec<Option<f64>>) -> PyResult<Column> {
    let mut builder = PrimitiveBuilder::<f64>::new();
    for value in values {
        match value {
            Some(v) => builder.append(v),
            None => builder.append_null(),
        }
    }
    builder.finish().map_err(|e| value_error(&e))
}

/// Mediana de una lista de numeros (`None` = nulo, se ignora). Un `NaN` da `NaN`.
///
/// Los enteros de magnitud mayor a 2^53 pierden precision al convertirse a `float`.
///
/// # Errors
/// `ValueError` si la lista no tiene valores validos.
#[pyfunction]
fn median(values: Vec<Option<f64>>) -> PyResult<f64> {
    median_column(&float_column(values)?, Method::default()).map_err(|e| value_error(&e))
}

/// Percentil `p` (en `[0, 100]`, interpolacion lineal tipo 7) de una lista de numeros.
///
/// # Errors
/// `ValueError` si `p` esta fuera de `[0, 100]` o la lista no tiene valores validos.
#[pyfunction]
fn percentile(values: Vec<Option<f64>>, p: f64) -> PyResult<f64> {
    percentile_column(&float_column(values)?, p, Method::default()).map_err(|e| value_error(&e))
}

/// Modulo nativo `geoda._geoda`.
#[pymodule]
fn _geoda(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(column_len, m)?)?;
    m.add_function(wrap_pyfunction!(median, m)?)?;
    m.add_function(wrap_pyfunction!(percentile, m)?)?;
    Ok(())
}
