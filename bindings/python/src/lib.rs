//! Binding de Python para geoda: `version`, `column_len`, `median`, `percentile` y los
//! estadisticos escalares (`count`, `sum`, `min`, `max`, `mean`, `var`, `std`, `corr`), `histogram`
//! y `group_aggregate` (la base de `geoda.group_by`).
//!
//! Todas las funciones numericas aceptan una lista o cualquier objeto con el protocolo Arrow
//! `PyCapsule` (`__arrow_c_array__`): `pyarrow`, `polars`, `pandas` con Arrow... (S10). Los
//! datos se copian (decision 003); no hace falta tener `pyarrow` para usar listas.
//!
//! Depende de `geoda-core`, `geoda-algo` y `geoda-stats` directamente (D2). El contrato de la C-API lo ejercita el
//! runner de spec, que carga `libgeoda` por `ctypes`; este modulo no pasa por ahi.

// Los macros de PyO3 generan `unsafe`: el binding es frontera, como `capi`.
#![allow(unsafe_code)]

use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict, PyList};

use geoda_algo::{Method, Value, median_column, percentile_column};
use geoda_core::interop::arrow_c::{ArrowArray, ArrowSchema, import_column};
use geoda_core::{Column, DataType, Error, PrimitiveBuilder};
use geoda_stats::{
    GroupOp, GroupValues, Number, correlation_columns, count_column, group_by, histogram_column,
    max_column, mean_column, min_column, std_dev_column, sum_column, variance_column,
};

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

/// Un `Number` de `geoda-stats` como `int` (columna entera) o `float` (columna `Float64`).
fn number_to_py(py: Python<'_>, number: Number) -> PyResult<Bound<'_, PyAny>> {
    Ok(match number {
        Number::Int(v) => v.into_pyobject(py)?.into_any(),
        Number::Float(v) => v.into_pyobject(py)?.into_any(),
    })
}

/// Cantidad de valores no nulos.
///
/// # Errors
/// `TypeError` si la columna Arrow no es numerica.
#[pyfunction]
fn count(values: &Bound<'_, PyAny>) -> PyResult<usize> {
    count_column(&input_column(values)?).map_err(|e| value_error(&e))
}

/// Suma de los valores no nulos; sin valores validos es 0. Una columna Arrow entera da un
/// `int`; una lista o una columna `Float64`, un `float`. Un `NaN` da `NaN`.
///
/// # Errors
/// `ValueError` si una suma entera no cabe en `int64`; `TypeError` como en `median`.
#[pyfunction]
fn sum<'py>(py: Python<'py>, values: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    number_to_py(
        py,
        sum_column(&input_column(values)?).map_err(|e| value_error(&e))?,
    )
}

/// Minimo de los valores no nulos, con el tipo de la columna (ver `sum`). Un `NaN` da `NaN`.
///
/// # Errors
/// `ValueError` si no hay valores validos; `TypeError` como en `median`.
#[pyfunction]
fn min<'py>(py: Python<'py>, values: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    number_to_py(
        py,
        min_column(&input_column(values)?).map_err(|e| value_error(&e))?,
    )
}

/// Maximo de los valores no nulos, con el tipo de la columna (ver `sum`). Un `NaN` da `NaN`.
///
/// # Errors
/// `ValueError` si no hay valores validos; `TypeError` como en `median`.
#[pyfunction]
fn max<'py>(py: Python<'py>, values: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    number_to_py(
        py,
        max_column(&input_column(values)?).map_err(|e| value_error(&e))?,
    )
}

/// Media de los valores no nulos, siempre `float`.
///
/// # Errors
/// `ValueError` si no hay valores validos; `TypeError` como en `median`.
#[pyfunction]
fn mean(values: &Bound<'_, PyAny>) -> PyResult<f64> {
    mean_column(&input_column(values)?).map_err(|e| value_error(&e))
}

/// Varianza con `ddof` grados de libertad restados. Por defecto `ddof=1` (muestral, como R y
/// pandas); `NumPy` usa `0` (S2).
///
/// # Errors
/// `ValueError` si no hay valores validos o `ddof >= n`; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (values, ddof=1))]
fn var(values: &Bound<'_, PyAny>, ddof: usize) -> PyResult<f64> {
    variance_column(&input_column(values)?, ddof).map_err(|e| value_error(&e))
}

/// Desviacion estandar (`sqrt` de la varianza) con `ddof` grados de libertad restados
/// (por defecto `1`).
///
/// # Errors
/// Los de `var`.
#[pyfunction]
#[pyo3(name = "std", signature = (values, ddof=1))]
fn std_dev(values: &Bound<'_, PyAny>, ddof: usize) -> PyResult<f64> {
    std_dev_column(&input_column(values)?, ddof).map_err(|e| value_error(&e))
}

/// Correlacion de Pearson entre dos columnas, en `[-1, 1]`. Las filas con nulo en cualquiera
/// de las dos se descartan en ambas.
///
/// # Errors
/// `ValueError` si miden distinto o una es constante; `TypeError` como en `median`.
#[pyfunction]
fn corr(x: &Bound<'_, PyAny>, y: &Bound<'_, PyAny>) -> PyResult<f64> {
    correlation_columns(&input_column(x)?, &input_column(y)?).map_err(|e| value_error(&e))
}

/// Histograma de ancho fijo con `bins` cubos iguales. Devuelve un `dict` con `edges`
/// (`bins + 1` bordes), `counts` y los contadores de lo que no cae en ningun cubo: `below` y
/// `above` (fuera de `range`), `nulls` y `nans`. Los cubos son `[a, b)` salvo el ultimo, cerrado.
///
/// `range` es `(lo, hi)`; por defecto, `[min, max]` de los datos sin contar nulos ni `NaN`.
///
/// # Errors
/// `ValueError` si `bins` es 0, el rango esta invertido o no es finito, o no hay valores
/// validos y no se dio `range`; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (values, bins, range=None))]
fn histogram<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
    bins: usize,
    range: Option<(f64, f64)>,
) -> PyResult<Bound<'py, PyDict>> {
    let h = histogram_column(&input_column(values)?, bins, range).map_err(|e| value_error(&e))?;
    let out = PyDict::new(py);
    out.set_item("edges", h.edges)?;
    out.set_item("counts", h.counts)?;
    out.set_item("below", h.below)?;
    out.set_item("above", h.above)?;
    out.set_item("nulls", h.nulls)?;
    out.set_item("nans", h.nans)?;
    Ok(out)
}

/// Una clave de grupo como objeto de Python (`None` = el grupo de claves nulas).
fn key_to_py<'py>(py: Python<'py>, key: Option<&Value>) -> PyResult<Bound<'py, PyAny>> {
    match key {
        None => Ok(py.None().into_bound(py)),
        Some(Value::Bool(v)) => v.into_bound_py_any(py),
        Some(Value::Int32(v)) => v.into_bound_py_any(py),
        Some(Value::Int64(v)) => v.into_bound_py_any(py),
        Some(Value::Float64(v)) => v.into_bound_py_any(py),
        Some(Value::Utf8(v)) => v.as_str().into_bound_py_any(py),
    }
}

/// Agregado por grupo: base de `geoda.group_by(keys).<agg>(values)`. `agg` es `count`, `sum`,
/// `min`, `max`, `mean`, `var` o `std`. Devuelve `(claves, resultados)`, dos listas del mismo
/// largo en orden de primera aparicion; el grupo de claves nulas sale como `None`.
///
/// # Errors
/// `ValueError` si `agg` no existe, `keys` y `values` miden distinto, un grupo no tiene valores
/// validos para `mean`/`min`/`max`/`var`/`std` o `ddof >= n`; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (keys, values, agg, ddof=1))]
fn group_aggregate<'py>(
    py: Python<'py>,
    keys: &Bound<'py, PyAny>,
    values: &Bound<'py, PyAny>,
    agg: &str,
    ddof: usize,
) -> PyResult<(Bound<'py, PyList>, Bound<'py, PyList>)> {
    let op = match agg {
        "count" => GroupOp::Count,
        "sum" => GroupOp::Sum,
        "min" => GroupOp::Min,
        "max" => GroupOp::Max,
        "mean" => GroupOp::Mean,
        "var" => GroupOp::Var,
        "std" => GroupOp::Std,
        otro => {
            return Err(PyValueError::new_err(format!(
                "agregado desconocido: {otro:?} (count, sum, min, max, mean, var, std)"
            )));
        }
    };
    let result = group_by(op, ddof, &input_column(keys)?, &input_column(values)?)
        .map_err(|e| value_error(&e))?;
    let key_list = PyList::empty(py);
    for key in &result.keys {
        key_list.append(key_to_py(py, key.as_ref())?)?;
    }
    let value_list = match result.values {
        GroupValues::Int(v) => PyList::new(py, v)?,
        GroupValues::Float(v) => PyList::new(py, v)?,
    };
    Ok((key_list, value_list))
}

/// Modulo nativo `geoda._geoda`.
#[pymodule]
fn _geoda(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(column_len, m)?)?;
    m.add_function(wrap_pyfunction!(median, m)?)?;
    m.add_function(wrap_pyfunction!(percentile, m)?)?;
    m.add_function(wrap_pyfunction!(count, m)?)?;
    m.add_function(wrap_pyfunction!(sum, m)?)?;
    m.add_function(wrap_pyfunction!(min, m)?)?;
    m.add_function(wrap_pyfunction!(max, m)?)?;
    m.add_function(wrap_pyfunction!(mean, m)?)?;
    m.add_function(wrap_pyfunction!(var, m)?)?;
    m.add_function(wrap_pyfunction!(std_dev, m)?)?;
    m.add_function(wrap_pyfunction!(corr, m)?)?;
    m.add_function(wrap_pyfunction!(histogram, m)?)?;
    m.add_function(wrap_pyfunction!(group_aggregate, m)?)?;
    Ok(())
}
