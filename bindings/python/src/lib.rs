//! Binding de Python para geoda: `version`, `column_len`, `median`, `percentile` y los
//! estadisticos escalares (`count`, `sum`, `min`, `max`, `mean`, `var`, `std`, `corr`), `histogram`
//! y `group_aggregate` (la base de `geoda.group_by`), la clase `Graph` (fase 4) y el texto de la fase 5 (`edit_distance`, `dedupe`, `lis`,
//! `huffman_encode` y `huffman_decode`), el clustering de la fase 6 (`kmeans`,
//! `hierarchical` y `cut_tree`) el arbol de decision (`DecisionTree`) y las reglas de asociacion (`frequent_itemsets` y
//! `association_rules`).
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
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict, PyList};

use geoda_algo::{
    Dataset, DecisionTree as Tree, Huffman, KMeansParams, Method, Network, Transactions,
    TreeParams, Value, cut_merges, dedupe_column, edit_distance_columns, fit_tree, frequencies,
    huffman_from_counts, kmeans_columns, lis_column, median_column, percentile_column,
    single_linkage_columns,
};
use geoda_algo::{association_rules as rules_of, frequent_itemsets as itemsets_of};
use geoda_core::interop::arrow_c::{ArrowArray, ArrowSchema, import_column};
use geoda_core::{Column, DataType, Error, PrimitiveBuilder, Utf8Builder};
use geoda_stats::{
    GroupOp, GroupValues, Number, correlation_columns_with_workers, count_column,
    group_by_with_workers, histogram_column_with_workers, max_column_with_workers,
    mean_column_with_workers, min_column_with_workers, std_dev_column_with_workers,
    sum_column_with_workers, variance_column_with_workers,
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

/// Hilos pedidos desde Python: un entero >= 1. Lo negativo o cero es `ValueError`, no
/// `OverflowError`.
fn workers_of(workers: i64) -> PyResult<usize> {
    usize::try_from(workers)
        .ok()
        .filter(|&n| n >= 1)
        .ok_or_else(|| PyValueError::new_err(format!("workers debe ser al menos 1, no {workers}")))
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
#[pyo3(signature = (values, workers=1))]
fn sum<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
    workers: i64,
) -> PyResult<Bound<'py, PyAny>> {
    number_to_py(
        py,
        sum_column_with_workers(&input_column(values)?, workers_of(workers)?)
            .map_err(|e| value_error(&e))?,
    )
}

/// Minimo de los valores no nulos, con el tipo de la columna (ver `sum`). Un `NaN` da `NaN`.
///
/// # Errors
/// `ValueError` si no hay valores validos; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (values, workers=1))]
fn min<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
    workers: i64,
) -> PyResult<Bound<'py, PyAny>> {
    number_to_py(
        py,
        min_column_with_workers(&input_column(values)?, workers_of(workers)?)
            .map_err(|e| value_error(&e))?,
    )
}

/// Maximo de los valores no nulos, con el tipo de la columna (ver `sum`). Un `NaN` da `NaN`.
///
/// # Errors
/// `ValueError` si no hay valores validos; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (values, workers=1))]
fn max<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
    workers: i64,
) -> PyResult<Bound<'py, PyAny>> {
    number_to_py(
        py,
        max_column_with_workers(&input_column(values)?, workers_of(workers)?)
            .map_err(|e| value_error(&e))?,
    )
}

/// Media de los valores no nulos, siempre `float`.
///
/// # Errors
/// `ValueError` si no hay valores validos; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (values, workers=1))]
fn mean(values: &Bound<'_, PyAny>, workers: i64) -> PyResult<f64> {
    mean_column_with_workers(&input_column(values)?, workers_of(workers)?)
        .map_err(|e| value_error(&e))
}

/// Varianza con `ddof` grados de libertad restados. Por defecto `ddof=1` (muestral, como R y
/// pandas); `NumPy` usa `0` (S2).
///
/// # Errors
/// `ValueError` si no hay valores validos o `ddof >= n`; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (values, ddof=1, workers=1))]
fn var(values: &Bound<'_, PyAny>, ddof: usize, workers: i64) -> PyResult<f64> {
    variance_column_with_workers(&input_column(values)?, ddof, workers_of(workers)?)
        .map_err(|e| value_error(&e))
}

/// Desviacion estandar (`sqrt` de la varianza) con `ddof` grados de libertad restados
/// (por defecto `1`).
///
/// # Errors
/// Los de `var`.
#[pyfunction]
#[pyo3(name = "std", signature = (values, ddof=1, workers=1))]
fn std_dev(values: &Bound<'_, PyAny>, ddof: usize, workers: i64) -> PyResult<f64> {
    std_dev_column_with_workers(&input_column(values)?, ddof, workers_of(workers)?)
        .map_err(|e| value_error(&e))
}

/// Correlacion de Pearson entre dos columnas, en `[-1, 1]`. Las filas con nulo en cualquiera
/// de las dos se descartan en ambas.
///
/// # Errors
/// `ValueError` si miden distinto o una es constante; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (x, y, workers=1))]
fn corr(x: &Bound<'_, PyAny>, y: &Bound<'_, PyAny>, workers: i64) -> PyResult<f64> {
    correlation_columns_with_workers(&input_column(x)?, &input_column(y)?, workers_of(workers)?)
        .map_err(|e| value_error(&e))
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
#[pyo3(signature = (values, bins, range=None, workers=1))]
fn histogram<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
    bins: usize,
    range: Option<(f64, f64)>,
    workers: i64,
) -> PyResult<Bound<'py, PyDict>> {
    let h =
        histogram_column_with_workers(&input_column(values)?, bins, range, workers_of(workers)?)
            .map_err(|e| value_error(&e))?;
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
#[pyo3(signature = (keys, values, agg, ddof=1, workers=1))]
fn group_aggregate<'py>(
    py: Python<'py>,
    keys: &Bound<'py, PyAny>,
    values: &Bound<'py, PyAny>,
    agg: &str,
    ddof: usize,
    workers: i64,
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
    let result = group_by_with_workers(
        op,
        ddof,
        &input_column(keys)?,
        &input_column(values)?,
        workers_of(workers)?,
    )
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

/// Columna `Utf8` de entrada: un `str` (una fila), una lista de `str` o `None`, o un objeto
/// Arrow de texto. Devuelve la columna y si venia como un solo `str`.
fn text_column(values: &Bound<'_, PyAny>) -> PyResult<(Column, bool)> {
    if values.hasattr("__arrow_c_array__")? {
        return Ok((arrow_column(values)?, false));
    }
    let (rows, single): (Vec<Option<String>>, bool) = match values.extract::<String>() {
        Ok(text) => (vec![Some(text)], true),
        Err(_) => (values.extract()?, false),
    };
    let mut builder = Utf8Builder::new();
    for row in &rows {
        match row {
            Some(text) => builder.append(text).map_err(|e| value_error(&e))?,
            None => builder.append_null(),
        }
    }
    Ok((builder.finish().map_err(|e| value_error(&e))?, single))
}

/// Distancia de edicion (Levenshtein) entre cadenas, contada en caracteres Unicode.
///
/// `a` y `b` pueden ser un `str`, una lista de `str` o `None` (nulo), o una columna Arrow de
/// texto. Con dos `str` devuelve un `int`; si no, una lista con la distancia de cada fila
/// (`None` si alguna de las dos es nula). Las dos miden lo mismo, o una mide 1 y se compara con
/// todas las filas de la otra.
///
/// # Errors
/// `ValueError` si miden distinto y ninguna mide 1; `TypeError` si una columna Arrow no es de
/// texto.
#[pyfunction]
fn edit_distance<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let ((a, single_a), (b, single_b)) = (text_column(a)?, text_column(b)?);
    let distances = edit_distance_columns(&a, &b).map_err(|e| match e {
        Error::TypeMismatch(_) => PyTypeError::new_err(e.to_string()),
        _ => value_error(&e),
    })?;
    let values: Vec<Option<i64>> = distances
        .values::<i64>()
        .map_err(|e| value_error(&e))?
        .iter()
        .enumerate()
        .map(|(i, &d)| (!distances.is_null(i).unwrap_or(true)).then_some(d))
        .collect();
    if single_a && single_b {
        return values[0].into_bound_py_any(py);
    }
    PyList::new(py, values)?.into_bound_py_any(py)
}

/// Agrupa las cadenas casi iguales: dos estan cerca si su distancia de edicion es a lo sumo
/// `max_distance` (por caracteres Unicode), y un grupo es una componente conexa de «estar cerca»
/// (si `a~b` y `b~c`, los tres van juntos aunque `a` y `c` no lo esten). `values` es una lista de
/// `str`/`None` o una columna Arrow de texto. Devuelve `{"ids": [int | None], "count": int}`: el
/// grupo de cada fila (desde 0, por primera aparicion; `None` en las filas nulas) y cuantos hay.
///
/// # Errors
/// `TypeError` si la columna Arrow no es de texto.
#[pyfunction]
#[pyo3(signature = (values, max_distance=1))]
fn dedupe<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
    max_distance: usize,
) -> PyResult<Bound<'py, PyDict>> {
    let (column, _) = text_column(values)?;
    let result = dedupe_column(&column, max_distance).map_err(|e| match e {
        Error::TypeMismatch(_) => PyTypeError::new_err(e.to_string()),
        _ => value_error(&e),
    })?;
    let out = PyDict::new(py);
    out.set_item("ids", result.ids)?;
    out.set_item("count", result.count)?;
    Ok(out)
}

/// Lista de columnas de entrada (una matriz): cada elemento es una lista de numeros o una columna
/// Arrow numerica. Sin nulos ni `NaN`.
fn matrix_columns(columns: &Bound<'_, PyAny>) -> PyResult<Vec<Column>> {
    columns
        .try_iter()?
        .map(|item| input_column(&item?))
        .collect()
}

/// K-means con inicializacion k-means++. `columns` es una lista de columnas (una por variable);
/// `seed` fija el azar, asi que la misma entrada, `k` y semilla dan el mismo resultado. Devuelve
/// `{"labels": [int], "centroids": [[float]], "inertia": float, "iterations": int}`: el grupo de
/// cada fila, `k` centroides de una fila de valores por variable, la suma de distancias al
/// cuadrado a su centroide y las iteraciones hechas.
///
/// # Errors
/// `ValueError` si hay `None` o `NaN`, las columnas miden distinto, `k` es 0 o mayor que las
/// filas, o `max_iter`/`tol` no valen; `TypeError` como en `median`.
#[pyfunction]
#[pyo3(signature = (columns, k, seed=0, max_iter=100, tol=1e-4))]
fn kmeans<'py>(
    py: Python<'py>,
    columns: &Bound<'py, PyAny>,
    k: usize,
    seed: u64,
    max_iter: usize,
    tol: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let owned = matrix_columns(columns)?;
    let refs: Vec<&Column> = owned.iter().collect();
    let params = KMeansParams {
        k,
        max_iter,
        tol,
        seed,
    };
    let result = kmeans_columns(&refs, params).map_err(|e| value_error(&e))?;
    let out = PyDict::new(py);
    out.set_item("labels", result.labels)?;
    let centroids: Vec<Vec<f64>> = result
        .centroids
        .chunks(result.dims)
        .map(<[f64]>::to_vec)
        .collect();
    out.set_item("centroids", centroids)?;
    out.set_item("inertia", result.inertia)?;
    out.set_item("iterations", result.iterations)?;
    Ok(out)
}

/// Dendrograma de enlace simple (clustering jerarquico con distancia euclidiana) de una lista de
/// columnas, hasta 5000 filas. Devuelve `{"n": int, "left": [int], "right": [int], "height":
/// [float], "size": [int]}` con `n - 1` uniones: los grupos `0..n` son las filas y la union `s`
/// crea el grupo `n + s`.
///
/// # Errors
/// `ValueError` como en `kmeans`, o si hay mas de 5000 filas.
#[pyfunction]
fn hierarchical<'py>(py: Python<'py>, columns: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let owned = matrix_columns(columns)?;
    let refs: Vec<&Column> = owned.iter().collect();
    let dendrogram = single_linkage_columns(&refs).map_err(|e| value_error(&e))?;
    let out = PyDict::new(py);
    out.set_item("n", dendrogram.n)?;
    out.set_item("left", dendrogram.left)?;
    out.set_item("right", dendrogram.right)?;
    out.set_item("height", dendrogram.height)?;
    out.set_item("size", dendrogram.size)?;
    Ok(out)
}

/// Corta en `k` grupos un dendrograma (el `dict` de `hierarchical`). Devuelve el grupo (`0..k`) de
/// cada fila, numerados por la primera fila en que aparece cada uno.
///
/// # Errors
/// `ValueError` si `k` es 0 o mayor que `n`, o las uniones no son validas; `KeyError` si falta una
/// clave del `dict`.
#[pyfunction]
fn cut_tree(dendrogram: &Bound<'_, PyDict>, k: usize) -> PyResult<Vec<usize>> {
    let get = |key: &str| -> PyResult<Bound<'_, PyAny>> {
        dendrogram
            .get_item(key)?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    };
    let n: usize = get("n")?.extract()?;
    let (left, right): (Vec<usize>, Vec<usize>) =
        (get("left")?.extract()?, get("right")?.extract()?);
    cut_merges(n, &left, &right, k).map_err(|e| value_error(&e))
}

/// Etiquetas de clase de entrada: una lista de enteros o una columna Arrow `Int32`/`Int64`, sin
/// nulos.
fn class_labels(labels: &Bound<'_, PyAny>) -> PyResult<Vec<i64>> {
    if labels.hasattr("__arrow_c_array__")? {
        let column = arrow_column(labels)?;
        if column.null_count() > 0 {
            return Err(PyValueError::new_err("las etiquetas tienen nulos"));
        }
        return match column.dtype() {
            DataType::Int64 => Ok(column
                .values::<i64>()
                .map_err(|e| value_error(&e))?
                .to_vec()),
            DataType::Int32 => Ok(column
                .values::<i32>()
                .map_err(|e| value_error(&e))?
                .iter()
                .map(|&v| i64::from(v))
                .collect()),
            other => Err(PyTypeError::new_err(format!(
                "las etiquetas deben ser enteros, no {other:?}"
            ))),
        };
    }
    labels.extract()
}

/// Arbol de decision de clasificacion (CART con impureza de Gini, solo variables numericas).
///
/// `DecisionTree(max_depth=8, min_samples_leaf=1).fit(columnas, etiquetas)` entrena y devuelve el
/// propio arbol; `predict(columnas)` da la clase de cada fila. `columnas` es una lista de columnas
/// (una por variable, sin `None` ni `NaN`) y las etiquetas, enteros `>= 0` (pueden ser dispersos).
/// Es determinista: los mismos datos dan el mismo arbol.
///
/// # Errors
/// `ValueError` si las columnas o las etiquetas miden distinto, hay nulos o no finitos, una etiqueta
/// es negativa, no hay filas, o `min_samples_leaf` es 0; `RuntimeError` si se predice antes de
/// entrenar.
#[pyclass(module = "geoda")]
struct DecisionTree {
    max_depth: usize,
    min_samples_leaf: usize,
    tree: Option<Tree>,
}

#[pymethods]
impl DecisionTree {
    #[new]
    #[pyo3(signature = (max_depth=8, min_samples_leaf=1))]
    fn new(max_depth: usize, min_samples_leaf: usize) -> Self {
        Self {
            max_depth,
            min_samples_leaf,
            tree: None,
        }
    }

    /// Entrena el arbol (descarta el anterior) y lo devuelve.
    fn fit<'py>(
        mut slf: PyRefMut<'py, Self>,
        columns: &Bound<'py, PyAny>,
        labels: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let owned = matrix_columns(columns)?;
        let refs: Vec<&Column> = owned.iter().collect();
        let labels = class_labels(labels)?;
        let data = Dataset::from_columns(&refs).map_err(|e| value_error(&e))?;
        let params = TreeParams {
            max_depth: slf.max_depth,
            min_samples_leaf: slf.min_samples_leaf,
        };
        slf.tree = Some(fit_tree(&data, &labels, params).map_err(|e| value_error(&e))?);
        Ok(slf)
    }

    /// Clase que predice el arbol para cada fila.
    fn predict(&self, columns: &Bound<'_, PyAny>) -> PyResult<Vec<i64>> {
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("el arbol no esta entrenado: llama a fit()"))?;
        let owned = matrix_columns(columns)?;
        let refs: Vec<&Column> = owned.iter().collect();
        tree.predict_columns(&refs).map_err(|e| value_error(&e))
    }

    /// Cantidad de nodos (hojas incluidas), o 0 si no esta entrenado.
    #[getter]
    fn n_nodes(&self) -> usize {
        self.tree.as_ref().map_or(0, |t| t.nodes().len())
    }

    /// Profundidad del arbol (0 si es una sola hoja o no esta entrenado).
    #[getter]
    fn depth(&self) -> usize {
        self.tree.as_ref().map_or(0, Tree::depth)
    }
}

/// Transacciones de entrada: una lista de transacciones, cada una con sus items (enteros `>= 0`;
/// un item repetido dentro de una transaccion cuenta una vez).
fn transactions_of(transactions: &[Vec<i64>]) -> PyResult<Transactions> {
    Transactions::from_lists(transactions).map_err(|e| value_error(&e))
}

/// Conjuntos de items frecuentes (algoritmo Apriori). `transactions` es una lista de listas de
/// items enteros `>= 0`; `min_support` es un conteo absoluto de transacciones (al menos 1).
/// Devuelve `{"itemsets": [[int]], "support": [int], "transactions": int}`: los conjuntos por
/// tamano y luego en orden lexicografico (cada uno con sus items de menor a mayor), cuantas
/// transacciones contienen a cada uno, y el total de transacciones.
///
/// # Errors
/// `ValueError` si `min_support` es 0 o hay un item negativo.
#[pyfunction]
// PyO3 extrae la lista de Python por valor; no hay forma de recibirla por referencia.
#[allow(clippy::needless_pass_by_value)]
fn frequent_itemsets(
    py: Python<'_>,
    transactions: Vec<Vec<i64>>,
    min_support: usize,
) -> PyResult<Bound<'_, PyDict>> {
    let found =
        itemsets_of(&transactions_of(&transactions)?, min_support).map_err(|e| value_error(&e))?;
    let out = PyDict::new(py);
    out.set_item("itemsets", found.itemsets)?;
    out.set_item("support", found.support)?;
    out.set_item("transactions", found.transactions)?;
    Ok(out)
}

/// Reglas de asociacion `antecedent -> consequent` con soporte `>= min_support` y confianza
/// `>= min_confidence` (en `(0, 1]`). Devuelve una lista de `dict` con `antecedent` y
/// `consequent` (listas de items), `support` (transacciones con los dos), `confidence`
/// (`soporte(A u C) / soporte(A)`) y `lift` (`confianza / (soporte(C) / N)`), por conjunto
/// frecuente y, dentro de cada uno, por la mascara de bits del antecedente.
///
/// # Errors
/// `ValueError` si `min_support` es 0, `min_confidence` no esta en `(0, 1]`, hay un item negativo
/// o un conjunto frecuente tiene mas de 20 items.
#[pyfunction]
// PyO3 extrae la lista de Python por valor; no hay forma de recibirla por referencia.
#[allow(clippy::needless_pass_by_value)]
fn association_rules(
    py: Python<'_>,
    transactions: Vec<Vec<i64>>,
    min_support: usize,
    min_confidence: f64,
) -> PyResult<Bound<'_, PyList>> {
    let found =
        itemsets_of(&transactions_of(&transactions)?, min_support).map_err(|e| value_error(&e))?;
    let rules = rules_of(&found, min_confidence).map_err(|e| value_error(&e))?;
    let out = PyList::empty(py);
    for rule in rules {
        let entry = PyDict::new(py);
        entry.set_item("antecedent", &found.itemsets[rule.antecedent])?;
        entry.set_item("consequent", &found.itemsets[rule.consequent])?;
        entry.set_item("support", rule.support)?;
        entry.set_item("confidence", rule.confidence)?;
        entry.set_item("lift", rule.lift)?;
        out.append(entry)?;
    }
    Ok(out)
}

/// Subsecuencia estrictamente creciente mas larga (LIS) de una lista de numeros o una columna
/// Arrow numerica. Devuelve `{"length": int, "indices": [int]}` con las posiciones de sus
/// elementos, de menor a mayor.
///
/// # Errors
/// `ValueError` si hay un `None` o un `NaN`; `TypeError` como en `median`.
#[pyfunction]
fn lis<'py>(py: Python<'py>, values: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let indices = lis_column(&input_column(values)?).map_err(|e| value_error(&e))?;
    let indices: Vec<i64> = indices
        .values::<i64>()
        .map_err(|e| value_error(&e))?
        .to_vec();
    let out = PyDict::new(py);
    out.set_item("length", indices.len())?;
    out.set_item("indices", indices)?;
    Ok(out)
}

/// Codifica con Huffman unos bytes (`bytes`, `bytearray` o un `str`, que se codifica en UTF-8).
/// Devuelve `{"bits": bytes, "bit_len": int, "frequencies": [int] * 256}`: los bits van
/// empaquetados con el primero en el bit mas alto del primer byte, y `bit_len` son los validos.
/// Las frecuencias son todo lo que `huffman_decode` necesita para reconstruir la tabla.
///
/// # Errors
/// `ValueError` si el codigo no se puede construir (no ocurre con datos reales).
#[pyfunction]
fn huffman_encode<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let bytes: Vec<u8> = match data.extract::<String>() {
        Ok(text) => text.into_bytes(),
        Err(_) => data.extract()?,
    };
    let huffman = Huffman::from_data(&bytes).map_err(|e| value_error(&e))?;
    let encoded = huffman.encode(&bytes).map_err(|e| value_error(&e))?;
    let out = PyDict::new(py);
    out.set_item("bits", pyo3::types::PyBytes::new(py, &encoded.bytes))?;
    out.set_item("bit_len", encoded.bit_len)?;
    out.set_item("frequencies", frequencies(&bytes).to_vec())?;
    Ok(out)
}

/// Decodifica los primeros `bit_len` bits de `bits` con la tabla que sale de `frequencies`
/// (las que devolvio `huffman_encode`). Devuelve `bytes`.
///
/// # Errors
/// `ValueError` si `frequencies` no son 256 enteros no negativos, `bit_len` pasa de los bits que
/// hay, o los bits no se pueden decodificar con esa tabla.
#[pyfunction]
// PyO3 extrae la lista de Python por valor; no hay forma de recibirla por referencia.
#[allow(clippy::needless_pass_by_value)]
fn huffman_decode<'py>(
    py: Python<'py>,
    bits: &[u8],
    bit_len: usize,
    frequencies: Vec<i64>,
) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    let huffman = huffman_from_counts(&frequencies).map_err(|e| value_error(&e))?;
    let decoded = huffman.decode(bits, bit_len).map_err(|e| value_error(&e))?;
    Ok(pyo3::types::PyBytes::new(py, &decoded))
}

/// Error de una consulta de grafo: `TypeMismatch` (falta el peso que la consulta necesita) y
/// `Unsupported` son `TypeError`; el resto, `ValueError`.
fn graph_error(e: &Error) -> PyErr {
    match e {
        Error::TypeMismatch(_) | Error::Unsupported(_) => PyTypeError::new_err(e.to_string()),
        _ => PyValueError::new_err(e.to_string()),
    }
}

/// Un nodo de Python (`int >= 0`) como indice.
fn node_of(node: i64, what: &str) -> PyResult<usize> {
    usize::try_from(node)
        .map_err(|_| PyValueError::new_err(format!("{what} no puede ser negativo: {node}")))
}

/// Grafo con o sin pesos: `Graph(n, edges, directed=False, weights=None)`.
///
/// `edges` es una lista de parejas `(u, v)` con nodos de 0 a `n - 1`; `weights`, si se da, tiene
/// un `float` por arista. Se acepta un multigrafo (aristas repetidas y bucles).
///
/// # Errors
/// `ValueError` si un nodo es negativo o `>= n`, o `weights` y `edges` miden distinto.
#[pyclass(frozen, module = "geoda")]
struct Graph {
    network: Network,
}

#[pymethods]
impl Graph {
    #[new]
    #[pyo3(signature = (n, edges, directed=false, weights=None))]
    // PyO3 extrae los argumentos por valor; no hay forma de recibirlos por referencia.
    #[allow(clippy::needless_pass_by_value)]
    fn new(
        n: usize,
        edges: Vec<[i64; 2]>,
        directed: bool,
        weights: Option<Vec<f64>>,
    ) -> PyResult<Self> {
        let edges = edges
            .into_iter()
            .map(|[u, v]| Ok((node_of(u, "un nodo")?, node_of(v, "un nodo")?)))
            .collect::<PyResult<Vec<_>>>()?;
        let network =
            Network::new(n, &edges, weights.as_deref(), directed).map_err(|e| graph_error(&e))?;
        Ok(Self { network })
    }

    /// Cantidad de nodos.
    #[getter]
    fn n(&self) -> usize {
        self.network.graph().node_count()
    }

    /// `True` si las aristas tienen sentido.
    #[getter]
    fn directed(&self) -> bool {
        self.network.graph().is_directed()
    }

    /// `True` si cada arista lleva un peso.
    #[getter]
    fn weighted(&self) -> bool {
        self.network.is_weighted()
    }

    /// Recorrido en anchura: `{"order", "distance", "parent"}`. `distance` y `parent` tienen un
    /// valor por nodo, con `None` si no es alcanzable (y en el origen, para `parent`).
    fn bfs<'py>(&self, py: Python<'py>, source: i64) -> PyResult<Bound<'py, PyDict>> {
        let result = self
            .network
            .bfs(node_of(source, "source")?)
            .map_err(|e| graph_error(&e))?;
        let out = PyDict::new(py);
        out.set_item("order", result.order)?;
        out.set_item("distance", result.distance)?;
        out.set_item("parent", result.parent)?;
        Ok(out)
    }

    /// Componentes conexas (debiles en un dirigido): `{"ids", "count"}`.
    fn components<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let result = self.network.components().map_err(|e| graph_error(&e))?;
        let out = PyDict::new(py);
        out.set_item("ids", result.id)?;
        out.set_item("count", result.count)?;
        Ok(out)
    }

    /// Orden topologico (lista de nodos). `ValueError` si hay un ciclo o el grafo no es dirigido.
    fn topological_order(&self) -> PyResult<Vec<u32>> {
        self.network
            .topological_order()
            .map_err(|e| graph_error(&e))
    }

    /// Caminos minimos desde `source`: `{"distance", "parent"}`, con `inf` y `None` para los no
    /// alcanzables. `TypeError` sin pesos; `ValueError` con un peso negativo o `NaN`.
    fn dijkstra<'py>(&self, py: Python<'py>, source: i64) -> PyResult<Bound<'py, PyDict>> {
        let result = self
            .network
            .dijkstra(node_of(source, "source")?)
            .map_err(|e| graph_error(&e))?;
        let out = PyDict::new(py);
        out.set_item("distance", result.distance)?;
        out.set_item("parent", result.parent)?;
        Ok(out)
    }

    /// Bosque de expansion minimo (Kruskal): `{"edges", "total", "components"}`, con `edges` las
    /// posiciones en la lista de aristas. `TypeError` sin pesos; `ValueError` si es dirigido.
    fn mst<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let forest = self.network.mst().map_err(|e| graph_error(&e))?;
        let out = PyDict::new(py);
        out.set_item("edges", forest.edges)?;
        out.set_item("total", forest.total_weight)?;
        out.set_item("components", forest.components)?;
        Ok(out)
    }
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
    m.add_function(wrap_pyfunction!(edit_distance, m)?)?;
    m.add_function(wrap_pyfunction!(dedupe, m)?)?;
    m.add_function(wrap_pyfunction!(lis, m)?)?;
    m.add_function(wrap_pyfunction!(kmeans, m)?)?;
    m.add_function(wrap_pyfunction!(hierarchical, m)?)?;
    m.add_function(wrap_pyfunction!(cut_tree, m)?)?;
    m.add_class::<DecisionTree>()?;
    m.add_function(wrap_pyfunction!(frequent_itemsets, m)?)?;
    m.add_function(wrap_pyfunction!(association_rules, m)?)?;
    m.add_function(wrap_pyfunction!(huffman_encode, m)?)?;
    m.add_function(wrap_pyfunction!(huffman_decode, m)?)?;
    m.add_class::<Graph>()?;
    Ok(())
}
