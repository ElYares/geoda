//! Binding de Python para geoda: `version`, `column_len`, `median`, `percentile` y los
//! estadisticos escalares (`count`, `sum`, `min`, `max`, `mean`, `var`, `std`, `corr`), `histogram`
//! y `group_aggregate` (la base de `geoda.group_by`), la clase `Graph` (fase 4) y el texto de la fase 5 (`edit_distance`, `dedupe`, `lis`,
//! `huffman_encode` y `huffman_decode`).
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

use geoda_algo::{
    Huffman, Method, Network, Value, dedupe_column, edit_distance_columns, frequencies,
    huffman_from_counts, lis_column, median_column, percentile_column,
};
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
    m.add_function(wrap_pyfunction!(huffman_encode, m)?)?;
    m.add_function(wrap_pyfunction!(huffman_decode, m)?)?;
    m.add_class::<Graph>()?;
    Ok(())
}
