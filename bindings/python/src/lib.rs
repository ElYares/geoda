//! Binding de Python para geoda: `version`, `column_len`, `median` y `percentile`.
//!
//! `median` y `percentile` aceptan una lista o cualquier objeto con el protocolo Arrow
//! `PyCapsule` (`__arrow_c_array__`): `pyarrow`, `polars`, `pandas` con Arrow... (S10). Los
//! datos se copian (decision 003); no hace falta tener `pyarrow` para usar listas.
//!
//! Depende de `geoda-core` y `geoda-algo` directamente (D2). El contrato de la C-API lo ejercita el
//! runner de spec, que carga `libgeoda` por `ctypes`; este modulo no pasa por ahi.

// Los macros de PyO3 generan `unsafe`: el binding es frontera, como `capi`.
#![allow(unsafe_code)]

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

use geoda_algo::{Method, median_column, percentile_column};
use geoda_core::interop::arrow_c::{ArrowArray, ArrowSchema, import_column};
use geoda_core::{Column, DataType, Error, PrimitiveBuilder};

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

/// `Unsupported` (un tipo que geoda no maneja) es `TypeError`; el resto, `ValueError`.
fn value_error(e: &Error) -> PyErr {
    match e {
        Error::Unsupported(_) => PyTypeError::new_err(e.to_string()),
        _ => PyValueError::new_err(e.to_string()),
    }
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

/// Columna a partir de un objeto con `__arrow_c_array__` (protocolo Arrow `PyCapsule`).
///
/// El productor entrega dos capsules (`arrow_schema`, `arrow_array`). `import_column` copia
/// los datos y, si todo sale bien, llama al `release` del array una vez y lo deja en `None`.
/// Si falla, no toma propiedad: el destructor del capsule llama al `release` (la
/// especificacion lo exige de quien crea el capsule) y el del esquema lo suelta siempre.
/// Asi cada `release` se llama exactamente una vez, tambien si la funcion falla.
fn arrow_column(obj: &Bound<'_, PyAny>) -> PyResult<Column> {
    let pair = obj.call_method0("__arrow_c_array__")?;
    let (schema_capsule, array_capsule): (Bound<'_, PyCapsule>, Bound<'_, PyCapsule>) =
        pair.extract()?;
    let schema_ptr = schema_capsule
        .pointer_checked(Some(c"arrow_schema"))?
        .cast::<ArrowSchema>();
    let array_ptr = array_capsule
        .pointer_checked(Some(c"arrow_array"))?
        .cast::<ArrowArray>();
    // SAFETY: los capsules tienen los nombres que fija el protocolo, asi que apuntan a un
    // `ArrowSchema` y un `ArrowArray` validos que siguen vivos mientras `schema_capsule` y
    // `array_capsule` existan (los tenemos hasta el final de la funcion). No hay otras
    // referencias a esas structs: el GIL esta tomado y no ejecutamos Python entre leer los
    // punteros y usarlos. `import_column` valida lo que el formato permite comprobar (y
    // rechaza un array ya liberado); que los buffers midan lo que dicen `offset + length`
    // es confianza en el productor, como fija la Arrow C Data Interface.
    unsafe { import_column(schema_ptr.as_ref(), &mut *array_ptr.as_ptr()) }
        .map_err(|e| value_error(&e))
}

/// Pasa `Bool` a `Float64` (0.0 y 1.0), como lo haria la lista equivalente.
fn bool_as_float(column: &Column) -> Result<Column, Error> {
    let mut builder = PrimitiveBuilder::<f64>::new();
    for i in 0..column.len() {
        if column.is_null(i)? {
            builder.append_null();
        } else {
            builder.append(f64::from(u8::from(column.bool_value(i)?)));
        }
    }
    builder.finish()
}

/// Columna de entrada: un objeto Arrow o una lista de numeros.
fn input_column(values: &Bound<'_, PyAny>) -> PyResult<Column> {
    if values.hasattr("__arrow_c_array__")? {
        let column = arrow_column(values)?;
        if column.dtype() == DataType::Bool {
            return bool_as_float(&column).map_err(|e| value_error(&e));
        }
        return Ok(column);
    }
    float_column(values.extract()?)
}

/// Mediana de una lista de numeros o de una columna Arrow (`None` = nulo, se ignora). Un
/// `NaN` da `NaN`.
///
/// Los enteros de magnitud mayor a 2^53 pierden precision al convertirse a `float`.
///
/// # Errors
/// `ValueError` si no hay valores validos; `TypeError` si la columna Arrow es de un tipo
/// no numerico (texto, `decimal128`, listas anidadas) o el objeto no es lista ni Arrow.
#[pyfunction]
fn median(values: &Bound<'_, PyAny>) -> PyResult<f64> {
    median_column(&input_column(values)?, Method::default()).map_err(|e| value_error(&e))
}

/// Percentil `p` (en `[0, 100]`, interpolacion lineal tipo 7) de una lista de numeros o
/// de una columna Arrow.
///
/// # Errors
/// `ValueError` si `p` esta fuera de `[0, 100]` o no hay valores validos; `TypeError`
/// como en `median`.
#[pyfunction]
fn percentile(values: &Bound<'_, PyAny>, p: f64) -> PyResult<f64> {
    percentile_column(&input_column(values)?, p, Method::default()).map_err(|e| value_error(&e))
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
