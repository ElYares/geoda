//! Runner de `spec/` (ver `spec/README.md`): recorre `spec/cases/**/*.json` y ejecuta
//! cada caso a traves de la C-API, tal como lo haria un binding.
//!
//! La entrada se arma con los builders de `geoda-core` y se entrega por
//! `geoda_column_from_arrow`; todo lo demas (`len`, `null_count`, `slice`, export) pasa por
//! funciones `geoda_*`. Lo unico que se lee con `geoda-core` es el resultado exportado.

#![allow(unsafe_code)]

use std::ffi::CStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;

use geoda::{
    GEODA_AGG_COUNT, GEODA_AGG_MAX, GEODA_AGG_MEAN, GEODA_AGG_MIN, GEODA_AGG_STD, GEODA_AGG_SUM,
    GEODA_AGG_VAR, GEODA_ERR_ALLOC, GEODA_ERR_FFI, GEODA_ERR_INTERNAL, GEODA_ERR_INVALID_ARGUMENT,
    GEODA_ERR_LENGTH_MISMATCH, GEODA_ERR_OUT_OF_BOUNDS, GEODA_ERR_PANIC, GEODA_ERR_TYPE_MISMATCH,
    GEODA_ERR_UNSUPPORTED, GEODA_OK, GeodaColumn, GeodaGraph, geoda_column_corr,
    geoda_column_count, geoda_column_dedupe, geoda_column_edit_distance, geoda_column_free,
    geoda_column_from_arrow, geoda_column_group_by, geoda_column_histogram, geoda_column_len,
    geoda_column_lis, geoda_column_max_f64, geoda_column_max_i64, geoda_column_mean,
    geoda_column_median, geoda_column_min_f64, geoda_column_min_i64, geoda_column_null_count,
    geoda_column_percentile, geoda_column_slice, geoda_column_std, geoda_column_sum_f64,
    geoda_column_sum_i64, geoda_column_to_arrow, geoda_column_var, geoda_dendrogram_cut,
    geoda_graph_bfs, geoda_graph_components, geoda_graph_dijkstra, geoda_graph_free,
    geoda_graph_mst, geoda_graph_new, geoda_graph_topological_order, geoda_hierarchical,
    geoda_huffman_decode, geoda_huffman_encode, geoda_kmeans, geoda_last_error_message,
};
use geoda_core::interop::arrow_c::{
    ArrowArray, ArrowSchema, export_column, export_field, import_column,
};
use geoda_core::{BoolBuilder, Column, DataType, Field, PrimitiveBuilder, Utf8Builder};
use serde_json::Value;

const SPEC_VERSION: i64 = 1;

/// Resultado de un caso: `Err` lleva el motivo del fallo.
type CaseResult = Result<(), String>;

/// Handle de la capi que se libera solo al salir de alcance.
struct Handle(*mut GeodaColumn);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: el handle viene de la capi y se libera una sola vez, aqui.
        unsafe { geoda_column_free(self.0) };
    }
}

/// Grafo de la capi que se libera solo al salir de alcance.
struct GraphHandle(*mut GeodaGraph);

impl Drop for GraphHandle {
    fn drop(&mut self) {
        // SAFETY: el handle viene de la capi y se libera una sola vez, aqui.
        unsafe { geoda_graph_free(self.0) };
    }
}

/// Nombre de categoria de un codigo de error de la capi (ver `spec/README.md`).
fn category(code: i32) -> &'static str {
    match code {
        GEODA_ERR_INVALID_ARGUMENT => "INVALID_ARGUMENT",
        GEODA_ERR_TYPE_MISMATCH => "TYPE_MISMATCH",
        GEODA_ERR_LENGTH_MISMATCH => "LENGTH_MISMATCH",
        GEODA_ERR_OUT_OF_BOUNDS => "OUT_OF_BOUNDS",
        GEODA_ERR_UNSUPPORTED => "UNSUPPORTED",
        GEODA_ERR_ALLOC => "ALLOC",
        GEODA_ERR_FFI => "FFI",
        GEODA_ERR_PANIC => "PANIC",
        GEODA_ERR_INTERNAL => "INTERNAL",
        _ => "DESCONOCIDO",
    }
}

fn last_error() -> String {
    // SAFETY: nunca es NULL y vale hasta la siguiente llamada en este hilo.
    unsafe { CStr::from_ptr(geoda_last_error_message()) }
        .to_string_lossy()
        .into_owned()
}

fn dtype_name(dtype: DataType) -> &'static str {
    match dtype {
        DataType::Bool => "bool",
        DataType::Int32 => "int32",
        DataType::Int64 => "int64",
        DataType::Float64 => "float64",
        DataType::Utf8 => "utf8",
    }
}

/// Flotante del JSON: numero, o una de las cuatro cadenas especiales.
fn parse_f64(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| format!("flotante invalido: {v}")),
        Value::String(s) => match s.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            "-0.0" => Ok(-0.0),
            otro => Err(format!("flotante especial desconocido: {otro:?}")),
        },
        _ => Err(format!("no es un flotante: {v}")),
    }
}

/// Mismos flotantes si tienen los mismos bits, o si ambos son NaN.
fn same_f64(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

fn values_of(input: &Value) -> Result<&Vec<Value>, String> {
    input["values"]
        .as_array()
        .ok_or_else(|| "input.values debe ser una lista".to_owned())
}

fn int_of<T: TryFrom<i64>>(v: &Value) -> Result<T, String> {
    v.as_i64()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| format!("entero fuera de rango o invalido: {v}"))
}

/// Arma la columna de entrada. Devuelve ademas el formato Arrow que hay que forzar,
/// para los tipos de prueba negativa que geoda no maneja (`decimal128`).
fn build_input(input: &Value) -> Result<(Column, Option<&'static CStr>), String> {
    let dtype = input["dtype"].as_str().ok_or("input.dtype falta")?;
    let values = values_of(input)?;
    let column = match dtype {
        "bool" => {
            let mut b = BoolBuilder::new();
            for v in values {
                match v {
                    Value::Null => b.append_null(),
                    Value::Bool(x) => b.append(*x),
                    _ => return Err(format!("no es un bool: {v}")),
                }
            }
            b.finish()
        }
        "int32" | "decimal128" => {
            let mut b = PrimitiveBuilder::<i32>::new();
            for v in values {
                if v.is_null() {
                    b.append_null();
                } else {
                    b.append(int_of(v)?);
                }
            }
            b.finish()
        }
        "int64" => {
            let mut b = PrimitiveBuilder::<i64>::new();
            for v in values {
                if v.is_null() {
                    b.append_null();
                } else {
                    b.append(int_of(v)?);
                }
            }
            b.finish()
        }
        "float64" => {
            let mut b = PrimitiveBuilder::<f64>::new();
            for v in values {
                if v.is_null() {
                    b.append_null();
                } else {
                    b.append(parse_f64(v)?);
                }
            }
            b.finish()
        }
        "utf8" => {
            let mut b = Utf8Builder::new();
            for v in values {
                match v {
                    Value::Null => b.append_null(),
                    Value::String(s) => b.append(s).map_err(|e| e.to_string())?,
                    _ => return Err(format!("no es texto: {v}")),
                }
            }
            b.finish()
        }
        otro => return Err(format!("dtype desconocido: {otro:?}")),
    }
    .map_err(|e| e.to_string())?;
    let forced = (dtype == "decimal128").then_some(c"d:10,2");
    Ok((column, forced))
}

/// Entrega la columna a la capi. `Ok(Err(codigo))` si la capi la rechaza.
fn import_through_capi(input: &Value) -> Result<Result<Handle, i32>, String> {
    let (column, forced) = build_input(input)?;
    let mut schema: ArrowSchema =
        export_field(&Field::new("x", column.dtype(), true)).map_err(|e| e.to_string())?;
    let mut array: ArrowArray = export_column(&column).map_err(|e| e.to_string())?;
    if let Some(format) = forced {
        schema.format = format.as_ptr();
    }
    let mut out: *mut GeodaColumn = ptr::null_mut();
    // SAFETY: structs recien exportadas por `geoda-core`, validas y distintas.
    let code = unsafe { geoda_column_from_arrow(&raw const schema, &raw mut array, &raw mut out) };
    // El esquema siempre es nuestro; el array solo si la capi no lo consumio.
    // SAFETY: cada `release` se llama una vez y solo si sigue presente.
    unsafe {
        if let Some(release) = array.release {
            release(&raw mut array);
        }
        if let Some(release) = schema.release {
            release(&raw mut schema);
        }
    }
    Ok(if code == GEODA_OK {
        Ok(Handle(out))
    } else {
        Err(code)
    })
}

/// Exporta el handle por la capi y lo vuelve a leer con `geoda-core`.
fn export_through_capi(handle: &Handle) -> Result<Column, String> {
    // SAFETY: ceros es un valor valido para estas structs de punteros y enteros.
    let mut schema: ArrowSchema = unsafe { std::mem::zeroed() };
    let mut array: ArrowArray = unsafe { std::mem::zeroed() };
    // SAFETY: handle valido y structs escribibles.
    let code = unsafe { geoda_column_to_arrow(handle.0, &raw mut schema, &raw mut array) };
    if code != GEODA_OK {
        return Err(format!(
            "to_arrow fallo: {} ({})",
            category(code),
            last_error()
        ));
    }
    // SAFETY: recien exportadas por la capi; el import consume `array`.
    let column = unsafe { import_column(&schema, &mut array) }.map_err(|e| e.to_string());
    // SAFETY: el esquema es nuestro y no se ha liberado.
    unsafe {
        if let Some(release) = schema.release {
            release(&raw mut schema);
        }
        // Si el import fallo, el array sigue siendo nuestro.
        if let Some(release) = array.release {
            release(&raw mut array);
        }
    }
    column
}

fn cell_matches(column: &Column, i: usize, expected: &Value) -> Result<bool, String> {
    let is_null = column.is_null(i).map_err(|e| e.to_string())?;
    if expected.is_null() || is_null {
        return Ok(expected.is_null() && is_null);
    }
    Ok(match column.dtype() {
        DataType::Bool => {
            expected.as_bool() == Some(column.bool_value(i).map_err(|e| e.to_string())?)
        }
        DataType::Int32 => {
            expected.as_i64()
                == Some(i64::from(
                    column.values::<i32>().map_err(|e| e.to_string())?[i],
                ))
        }
        DataType::Int64 => {
            expected.as_i64() == Some(column.values::<i64>().map_err(|e| e.to_string())?[i])
        }
        DataType::Float64 => same_f64(
            column.values::<f64>().map_err(|e| e.to_string())?[i],
            parse_f64(expected)?,
        ),
        DataType::Utf8 => {
            expected.as_str() == Some(column.str_value(i).map_err(|e| e.to_string())?)
        }
    })
}

/// Compara las claves `dtype` y `values` de `expect` con la columna exportada.
fn check_exported(expect: &Value, handle: &Handle) -> CaseResult {
    if expect.get("dtype").is_none() && expect.get("values").is_none() {
        return Ok(());
    }
    let column = export_through_capi(handle)?;
    if let Some(dtype) = expect.get("dtype")
        && dtype.as_str() != Some(dtype_name(column.dtype()))
    {
        return Err(format!(
            "dtype: esperado {dtype}, salio {}",
            dtype_name(column.dtype())
        ));
    }
    if let Some(values) = expect.get("values") {
        let values = values
            .as_array()
            .ok_or("expect.values debe ser una lista")?;
        if values.len() != column.len() {
            return Err(format!(
                "values: esperados {}, salieron {}",
                values.len(),
                column.len()
            ));
        }
        for (i, expected) in values.iter().enumerate() {
            if !cell_matches(&column, i, expected)? {
                return Err(format!("values[{i}]: esperado {expected}"));
            }
        }
    }
    Ok(())
}

/// Compara `len` y `null_count` de `expect` con lo que dice el handle.
fn check_counts(expect: &Value, handle: &Handle) -> CaseResult {
    // SAFETY: handle valido.
    let (len, nulls) = unsafe {
        (
            geoda_column_len(handle.0),
            geoda_column_null_count(handle.0),
        )
    };
    for (key, actual) in [("len", len), ("null_count", nulls)] {
        if let Some(expected) = expect.get(key)
            && expected.as_i64() != Some(actual)
        {
            return Err(format!("{key}: esperado {expected}, salio {actual}"));
        }
    }
    Ok(())
}

/// Compara `expect.columns` con las columnas que devolvio la operacion, en orden.
fn check_columns(expect: &Value, handles: &[Handle]) -> CaseResult {
    let expected = expect
        .get("columns")
        .and_then(Value::as_array)
        .ok_or("expect.columns falta (debe ser una lista)")?;
    if expected.len() != handles.len() {
        return Err(format!(
            "expect.columns tiene {} entradas y la operacion devolvio {} columnas",
            expected.len(),
            handles.len()
        ));
    }
    for (i, (want, handle)) in expected.iter().zip(handles).enumerate() {
        check_exported(want, handle).map_err(|e| format!("columns[{i}]: {e}"))?;
    }
    Ok(())
}

fn check_expect(expect: &Value, outcome: &Outcome) -> CaseResult {
    match outcome {
        Outcome::Column(handle) => {
            if expect.get("value").is_some() {
                return Err(
                    "expect.value pide un numero, pero la operacion dio una columna".into(),
                );
            }
            check_counts(expect, handle)?;
            check_exported(expect, handle)
        }
        Outcome::Graph(handles, extras) => {
            check_columns(expect, handles)?;
            for (key, actual) in extras {
                let want = expect.get(*key).ok_or(format!("expect.{key} falta"))?;
                match actual {
                    Extra::Int(actual) if want.as_i64() == Some(*actual) => {}
                    Extra::Float(actual) if same_f64(parse_f64(want)?, *actual) => {}
                    Extra::Int(actual) => {
                        return Err(format!("{key}: esperado {want}, salio {actual}"));
                    }
                    Extra::Float(actual) => {
                        return Err(format!("{key}: esperado {want}, salio {actual:?}"));
                    }
                }
            }
            Ok(())
        }
        Outcome::Columns(handles) => check_columns(expect, handles),
        Outcome::Int(actual) => {
            let expected = expect
                .get("value")
                .and_then(Value::as_i64)
                .ok_or("expect.value debe ser un entero de JSON")?;
            if expected == *actual {
                Ok(())
            } else {
                Err(format!("value: esperado {expected}, salio {actual}"))
            }
        }
        Outcome::Value(actual) => {
            let expected = parse_f64(expect.get("value").ok_or("expect.value falta")?)?;
            if same_f64(expected, *actual) {
                Ok(())
            } else {
                Err(format!("value: esperado {expected:?}, salio {actual:?}"))
            }
        }
    }
}

/// La operacion fallo con `code`: el caso pasa solo si esperaba esa categoria.
fn check_error(expect: &Value, code: i32) -> CaseResult {
    match expect.get("error").and_then(Value::as_str) {
        Some(expected) if expected == category(code) => Ok(()),
        Some(expected) => Err(format!(
            "esperaba el error {expected}, salio {} ({})",
            category(code),
            last_error()
        )),
        None => Err(format!(
            "fallo inesperado: {} ({})",
            category(code),
            last_error()
        )),
    }
}

/// Lo que produce una operacion: una columna, un entero (`count` y `sum`/`min`/`max` sobre
/// enteros) o un `float64`.
enum Outcome {
    Column(Handle),
    /// Varias columnas nuevas (`histogram`, `group_by`), en el orden de `spec/README.md`.
    Columns(Vec<Handle>),
    /// Columnas mas escalares con nombre (`count`, `total`, `components`) de una consulta de grafo.
    Graph(Vec<Handle>, Vec<(&'static str, Extra)>),
    Int(i64),
    Value(f64),
}

/// Escalar con nombre que una consulta de grafo devuelve junto a sus columnas.
enum Extra {
    Int(i64),
    Float(f64),
}

/// Resultado de una operacion, o el codigo con el que fallo.
type OpResult = Result<Outcome, i32>;

fn run_op(op: &str, case: &Value, handle: Handle) -> Result<OpResult, String> {
    match op {
        "roundtrip" | "len" | "null_count" => Ok(Ok(Outcome::Column(handle))),
        "median" | "percentile" => {
            let mut out = 0.0_f64;
            // SAFETY: handle valido y `out` escribible.
            let code = unsafe {
                if op == "median" {
                    geoda_column_median(handle.0, &raw mut out)
                } else {
                    let p = parse_f64(&case["args"]["p"])?;
                    geoda_column_percentile(handle.0, p, &raw mut out)
                }
            };
            Ok(if code == GEODA_OK {
                Ok(Outcome::Value(out))
            } else {
                Err(code)
            })
        }
        "count" | "sum" | "min" | "max" | "mean" | "var" | "std" | "corr" => {
            run_scalar(op, case, &handle)
        }
        "histogram" => run_histogram(case, &handle),
        "edit_distance" | "huffman_decode" | "dendrogram_cut" => run_pair(op, case, &handle),
        "dedupe" => {
            let (mut out, mut count) = (ptr::null_mut::<GeodaColumn>(), -1_i64);
            // SAFETY: handle valido y los `out` son escribibles.
            let code = unsafe {
                geoda_column_dedupe(
                    handle.0,
                    usize_arg(case, "max_distance")?,
                    &raw mut out,
                    &raw mut count,
                )
            };
            if code != GEODA_OK {
                return collect_outs(code, &[out]);
            }
            Ok(Ok(Outcome::Graph(
                vec![Handle(out)],
                vec![("count", Extra::Int(count))],
            )))
        }
        "lis" => {
            let mut out: *mut GeodaColumn = ptr::null_mut();
            // SAFETY: handle valido y `out` escribible.
            let code = unsafe { geoda_column_lis(handle.0, &raw mut out) };
            collect_outs(code, &[out])
        }
        "huffman_encode" => {
            let mut outs = [ptr::null_mut::<GeodaColumn>(); 2];
            let [bits, frequencies] = &mut outs;
            // SAFETY: handle valido y los `out` son escribibles.
            let code = unsafe { geoda_huffman_encode(handle.0, bits, frequencies) };
            collect_outs(code, &outs)
        }
        "group_by" => run_group_by(case, &handle),
        "slice" => {
            let arg = |name: &str| -> Result<usize, String> {
                case["args"][name]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| format!("args.{name} falta o no es un entero >= 0"))
            };
            let (offset, len) = (arg("offset")?, arg("len")?);
            let mut out: *mut GeodaColumn = ptr::null_mut();
            // SAFETY: handle valido y `out` escribible.
            let code = unsafe { geoda_column_slice(handle.0, offset, len, &raw mut out) };
            Ok(if code == GEODA_OK {
                Ok(Outcome::Column(Handle(out)))
            } else {
                Err(code)
            })
        }
        otro => Err(format!("op desconocida: {otro:?}")),
    }
}

/// Operaciones que reciben dos columnas (`input` e `input2`) y devuelven una: `edit_distance`
/// (`a`, `b`), `huffman_decode` (`frequencies`, `bits`) y `dendrogram_cut` (`left`, `right`).
fn run_pair(op: &str, case: &Value, handle: &Handle) -> Result<OpResult, String> {
    let other = match import_through_capi(case.get("input2").ok_or("input2 falta")?)? {
        Ok(other) => other,
        Err(code) => return Ok(Err(code)),
    };
    let mut out: *mut GeodaColumn = ptr::null_mut();
    // SAFETY: handles validos y `out` escribible.
    let code = unsafe {
        match op {
            "edit_distance" => geoda_column_edit_distance(handle.0, other.0, &raw mut out),
            "dendrogram_cut" => geoda_dendrogram_cut(
                handle.0,
                other.0,
                usize_arg(case, "n")?,
                usize_arg(case, "k")?,
                &raw mut out,
            ),
            _ => geoda_huffman_decode(handle.0, other.0, &raw mut out),
        }
    };
    collect_outs(code, &[out])
}

/// Operaciones sobre una matriz: `case.inputs` es la lista de columnas (una por variable) y se
/// entregan a la capi como un arreglo de handles (`kmeans`, `hierarchical`).
fn run_matrix_op(op: &str, case: &Value, inputs: &Value) -> Result<OpResult, String> {
    let specs = inputs.as_array().ok_or("inputs debe ser una lista")?;
    let mut owned = Vec::new();
    for spec in specs {
        match import_through_capi(spec)? {
            Ok(handle) => owned.push(handle),
            Err(code) => return Ok(Err(code)),
        }
    }
    let pointers: Vec<*const GeodaColumn> = owned.iter().map(|h| h.0.cast_const()).collect();
    match op {
        "kmeans" => {
            let max_iter = match case["args"].get("max_iter") {
                None | Some(Value::Null) => 100,
                Some(_) => usize_arg(case, "max_iter")?,
            };
            let tol = match case["args"].get("tol") {
                None | Some(Value::Null) => 1e-4,
                Some(value) => parse_f64(value)?,
            };
            let seed = case["args"]["seed"].as_u64().ok_or("args.seed falta")?;
            let mut outs = [ptr::null_mut::<GeodaColumn>(); 2];
            let [labels, centroids] = &mut outs;
            let (mut inertia, mut iterations) = (f64::NAN, -1_i64);
            // SAFETY: handles validos, arreglo de `pointers.len()` punteros y `out` escribibles.
            let code = unsafe {
                geoda_kmeans(
                    pointers.as_ptr(),
                    pointers.len(),
                    usize_arg(case, "k")?,
                    max_iter,
                    tol,
                    seed,
                    labels,
                    centroids,
                    &raw mut inertia,
                    &raw mut iterations,
                )
            };
            collect_with_extras(
                code,
                &outs,
                vec![
                    ("inertia", Extra::Float(inertia)),
                    ("iterations", Extra::Int(iterations)),
                ],
            )
        }
        "hierarchical" => {
            let mut outs = [ptr::null_mut::<GeodaColumn>(); 4];
            let [left, right, height, size] = &mut outs;
            // SAFETY: handles validos, arreglo de `pointers.len()` punteros y `out` escribibles.
            let code = unsafe {
                geoda_hierarchical(pointers.as_ptr(), pointers.len(), left, right, height, size)
            };
            collect_with_extras(code, &outs, Vec::new())
        }
        otro => Err(format!("op de matriz desconocida: {otro:?}")),
    }
}

/// Como `collect_outs`, pero con escalares con nombre que se comparan con `expect`.
fn collect_with_extras(
    code: i32,
    outs: &[*mut GeodaColumn],
    extras: Vec<(&'static str, Extra)>,
) -> Result<OpResult, String> {
    match collect_outs(code, outs)? {
        Ok(Outcome::Columns(handles)) => Ok(Ok(Outcome::Graph(handles, extras))),
        other => Ok(other),
    }
}

/// `n_bins` o `ddof`: un entero >= 0 de `args`.
fn usize_arg(case: &Value, name: &str) -> Result<usize, String> {
    case["args"][name]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("args.{name} falta o no es un entero >= 0"))
}

/// `args.workers`: hilos de la operacion; si falta, 1. Acepta 0 para probar el error.
fn workers_arg(case: &Value) -> Result<usize, String> {
    match case["args"].get("workers") {
        None | Some(Value::Null) => Ok(1),
        Some(_) => usize_arg(case, "workers"),
    }
}

/// Recoge los handles que la capi dejo en los `out`; si fallo, todos deben ser NULL.
fn collect_outs(code: i32, outs: &[*mut GeodaColumn]) -> Result<OpResult, String> {
    if code != GEODA_OK {
        if outs.iter().any(|p| !p.is_null()) {
            return Err("la capi fallo pero dejo un out sin poner en NULL".into());
        }
        return Ok(Err(code));
    }
    if outs.iter().any(|p| p.is_null()) {
        return Err("la capi devolvio GEODA_OK con un out NULL".into());
    }
    Ok(Ok(Outcome::Columns(
        outs.iter().map(|&p| Handle(p)).collect(),
    )))
}

fn run_histogram(case: &Value, handle: &Handle) -> Result<OpResult, String> {
    let n_bins = usize_arg(case, "n_bins")?;
    let range = match case["args"].get("range") {
        None | Some(Value::Null) => None,
        Some(Value::Array(pair)) if pair.len() == 2 => {
            Some((parse_f64(&pair[0])?, parse_f64(&pair[1])?))
        }
        Some(_) => return Err("args.range debe ser [lo, hi]".into()),
    };
    let mut outs = [ptr::null_mut::<GeodaColumn>(); 3];
    let [edges, counts, outside] = &mut outs;
    let (lo, hi) = range.unwrap_or((0.0, 0.0));
    // SAFETY: handle valido y los `out` son escribibles.
    let code = unsafe {
        geoda_column_histogram(
            handle.0,
            n_bins,
            range.is_some(),
            lo,
            hi,
            workers_arg(case)?,
            edges,
            counts,
            outside,
        )
    };
    collect_outs(code, &outs)
}

fn run_group_by(case: &Value, handle: &Handle) -> Result<OpResult, String> {
    let agg = match case["args"]["agg"].as_str().ok_or("args.agg falta")? {
        "count" => GEODA_AGG_COUNT,
        "sum" => GEODA_AGG_SUM,
        "min" => GEODA_AGG_MIN,
        "max" => GEODA_AGG_MAX,
        "mean" => GEODA_AGG_MEAN,
        "var" => GEODA_AGG_VAR,
        "std" => GEODA_AGG_STD,
        otro => return Err(format!("args.agg desconocido: {otro:?}")),
    };
    let ddof = if matches!(agg, GEODA_AGG_VAR | GEODA_AGG_STD) {
        usize_arg(case, "ddof")?
    } else {
        0
    };
    let values = match import_through_capi(case.get("input2").ok_or("input2 falta")?)? {
        Ok(values) => values,
        Err(code) => return Ok(Err(code)),
    };
    let mut outs = [ptr::null_mut::<GeodaColumn>(); 2];
    let [out_keys, out_values] = &mut outs;
    // SAFETY: handles validos y `out` escribibles.
    let code = unsafe {
        geoda_column_group_by(
            handle.0,
            values.0,
            agg,
            ddof,
            workers_arg(case)?,
            out_keys,
            out_values,
        )
    };
    collect_outs(code, &outs)
}

/// Estadisticos escalares: `sum`, `min` y `max` llaman a la variante `_i64` si la columna de
/// entrada es entera y a la `_f64` si no (ver `spec/README.md`).
fn run_scalar(op: &str, case: &Value, handle: &Handle) -> Result<OpResult, String> {
    let int_input = matches!(case["input"]["dtype"].as_str(), Some("int32" | "int64"));
    let ddof = || -> Result<usize, String> {
        case["args"]["ddof"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| "args.ddof falta o no es un entero >= 0".to_string())
    };
    let workers = workers_arg(case)?;
    let (mut int, mut float) = (0_i64, 0.0_f64);
    // SAFETY: handle valido y cada `out` escribible.
    let code = unsafe {
        match (op, int_input) {
            ("count", _) => geoda_column_count(handle.0, &raw mut int),
            ("sum", true) => geoda_column_sum_i64(handle.0, workers, &raw mut int),
            ("sum", false) => geoda_column_sum_f64(handle.0, workers, &raw mut float),
            ("min", true) => geoda_column_min_i64(handle.0, workers, &raw mut int),
            ("min", false) => geoda_column_min_f64(handle.0, workers, &raw mut float),
            ("max", true) => geoda_column_max_i64(handle.0, workers, &raw mut int),
            ("max", false) => geoda_column_max_f64(handle.0, workers, &raw mut float),
            ("mean", _) => geoda_column_mean(handle.0, workers, &raw mut float),
            ("var", _) => geoda_column_var(handle.0, ddof()?, workers, &raw mut float),
            ("std", _) => geoda_column_std(handle.0, ddof()?, workers, &raw mut float),
            ("corr", _) => match import_through_capi(case.get("input2").ok_or("input2 falta")?)? {
                Ok(other) => geoda_column_corr(handle.0, other.0, workers, &raw mut float),
                Err(code) => code,
            },
            _ => return Err(format!("op escalar desconocida: {op:?}")),
        }
    };
    let gives_int = op == "count" || (int_input && matches!(op, "sum" | "min" | "max"));
    Ok(if code != GEODA_OK {
        Err(code)
    } else if gives_int {
        Ok(Outcome::Int(int))
    } else {
        Ok(Outcome::Value(float))
    })
}

/// Construye el grafo de `case.graph` por la capi: las aristas viajan como dos columnas
/// `Int64` y los pesos, si hay, como una `Float64`. `Ok(Err(codigo))` si la capi lo rechaza.
fn build_graph(spec: &Value) -> Result<Result<GraphHandle, i32>, String> {
    let n = int_of::<usize>(&spec["n"])?;
    let edges = spec["edges"]
        .as_array()
        .ok_or("graph.edges debe ser una lista")?;
    let mut ends: [Vec<Value>; 2] = [Vec::new(), Vec::new()];
    for edge in edges {
        let pair = edge
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or("cada arista es [u, v]")?;
        ends[0].push(pair[0].clone());
        ends[1].push(pair[1].clone());
    }
    let column = |dtype: &str, values: &[Value]| {
        import_through_capi(&serde_json::json!({ "dtype": dtype, "values": values }))
    };
    let from = match column("int64", &ends[0])? {
        Ok(handle) => handle,
        Err(code) => return Ok(Err(code)),
    };
    let to = match column("int64", &ends[1])? {
        Ok(handle) => handle,
        Err(code) => return Ok(Err(code)),
    };
    let weights = match spec.get("weights") {
        None | Some(Value::Null) => None,
        Some(w) => match column(
            "float64",
            w.as_array().ok_or("graph.weights debe ser una lista")?,
        )? {
            Ok(handle) => Some(handle),
            Err(code) => return Ok(Err(code)),
        },
    };
    let directed = spec["directed"].as_bool().ok_or("graph.directed falta")?;
    let mut out: *mut GeodaGraph = ptr::null_mut();
    // SAFETY: handles validos (o NULL en `weights`) y `out` escribible.
    let code = unsafe {
        geoda_graph_new(
            n,
            from.0,
            to.0,
            weights.as_ref().map_or(ptr::null(), |w| w.0),
            directed,
            &raw mut out,
        )
    };
    if code != GEODA_OK {
        if !out.is_null() {
            return Err("geoda_graph_new fallo pero dejo el out sin poner en NULL".into());
        }
        return Ok(Err(code));
    }
    Ok(Ok(GraphHandle(out)))
}

/// Ejecuta una consulta de grafo (`graph_*`), a la que `expect` compara columnas y escalares.
fn run_graph_op(op: &str, case: &Value, graph: &GraphHandle) -> Result<OpResult, String> {
    let source = || usize_arg(case, "source");
    let mut outs = [ptr::null_mut::<GeodaColumn>(); 3];
    let (mut count, mut components, mut total) = (0_i64, 0_i64, 0.0_f64);
    let [first, second, third] = &mut outs;
    // SAFETY: handle valido y los `out` son escribibles.
    let (code, used, extras) = unsafe {
        match op {
            "graph_bfs" => (
                geoda_graph_bfs(graph.0, source()?, first, second, third),
                3,
                Vec::new(),
            ),
            "graph_components" => {
                let code = geoda_graph_components(graph.0, first, &raw mut count);
                (code, 1, vec![("count", Extra::Int(count))])
            }
            "graph_topological_order" => {
                (geoda_graph_topological_order(graph.0, first), 1, Vec::new())
            }
            "graph_dijkstra" => (
                geoda_graph_dijkstra(graph.0, source()?, first, second),
                2,
                Vec::new(),
            ),
            "graph_mst" => {
                let code = geoda_graph_mst(graph.0, first, &raw mut total, &raw mut components);
                (
                    code,
                    1,
                    vec![
                        ("total", Extra::Float(total)),
                        ("components", Extra::Int(components)),
                    ],
                )
            }
            otro => return Err(format!("op de grafo desconocida: {otro:?}")),
        }
    };
    if code != GEODA_OK {
        if outs.iter().any(|p| !p.is_null()) {
            return Err("la capi fallo pero dejo un out sin poner en NULL".into());
        }
        return Ok(Err(code));
    }
    if outs[..used].iter().any(|p| p.is_null()) {
        return Err("la capi devolvio GEODA_OK con un out NULL".into());
    }
    let handles = outs[..used].iter().map(|&p| Handle(p)).collect();
    Ok(Ok(Outcome::Graph(handles, extras)))
}

/// Ejecuta un caso. Cualquier cosa inesperada es `Err`, incluida una `op` que el
/// runner no conoce: no se salta en silencio.
fn run_case(case: &Value) -> CaseResult {
    if case["spec_version"].as_i64() != Some(SPEC_VERSION) {
        return Err(format!(
            "spec_version {} no soportada",
            case["spec_version"]
        ));
    }
    let op = case["op"].as_str().ok_or("op falta")?;
    let expect = case.get("expect").ok_or("expect falta")?;

    let result = if let Some(inputs) = case.get("inputs") {
        run_matrix_op(op, case, inputs)?
    } else if let Some(spec) = case.get("graph") {
        match build_graph(spec)? {
            Ok(graph) => run_graph_op(op, case, &graph)?,
            Err(code) => Err(code),
        }
    } else {
        match import_through_capi(&case["input"])? {
            Ok(handle) => run_op(op, case, handle)?,
            Err(code) => Err(code),
        }
    };
    match result {
        Ok(outcome) if expect.get("error").is_some() => {
            let _ = outcome;
            Err(format!(
                "esperaba el error {}, pero la operacion tuvo exito",
                expect["error"]
            ))
        }
        Ok(outcome) => check_expect(expect, &outcome),
        Err(code) => check_error(expect, code),
    }
}

fn collect_cases(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("no se puede leer {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_cases(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/cases")
}

#[test]
fn spec_all_cases_pass() {
    let mut files = Vec::new();
    collect_cases(&cases_dir(), &mut files);
    assert!(
        files.len() >= 4,
        "se esperaban al menos 4 casos, hay {}",
        files.len()
    );

    let mut failures = Vec::new();
    for path in &files {
        let name = path
            .strip_prefix(cases_dir())
            .unwrap()
            .display()
            .to_string();
        let case: Value = match serde_json::from_str(&fs::read_to_string(path).unwrap()) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{name}: JSON invalido: {e}"));
                continue;
            }
        };
        // El `id` debe coincidir con la ruta (`grupo/nombre`).
        let expected_id = name.trim_end_matches(".json");
        if case["id"].as_str() != Some(expected_id) {
            failures.push(format!("{name}: id {} no coincide con la ruta", case["id"]));
            continue;
        }
        if let Err(e) = run_case(&case) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "casos que fallan:\n  {}",
        failures.join("\n  ")
    );
}

/// Caso en memoria para probar al propio runner.
fn inline(op: &str, expect: &Value) -> Value {
    serde_json::json!({
        "spec_version": 1,
        "id": "inline",
        "description": "caso en memoria",
        "input": { "dtype": "int32", "values": [1, null, 3] },
        "op": op,
        "args": { "offset": 0, "len": 9 },
        "expect": expect,
    })
}

#[test]
fn spec_unknown_op_fails() {
    let err = run_case(&inline("explode", &serde_json::json!({ "len": 3 }))).unwrap_err();
    assert!(err.contains("op desconocida"), "{err}");
}

#[test]
fn spec_len_op_reports_len() {
    run_case(&inline("len", &serde_json::json!({ "len": 3 }))).unwrap();
    assert!(run_case(&inline("len", &serde_json::json!({ "len": 4 }))).is_err());
}

#[test]
fn spec_error_passes_only_with_that_category() {
    let slice_oob = |category: &str| inline("slice", &serde_json::json!({ "error": category }));
    run_case(&slice_oob("OUT_OF_BOUNDS")).unwrap();
    let err = run_case(&slice_oob("TYPE_MISMATCH")).unwrap_err();
    assert!(err.contains("OUT_OF_BOUNDS"), "{err}");
}

#[test]
fn spec_error_expected_but_operation_succeeds_fails() {
    let err = run_case(&inline("len", &serde_json::json!({ "error": "FFI" }))).unwrap_err();
    assert!(err.contains("tuvo exito"), "{err}");
}

#[test]
fn spec_unsupported_format_is_reported_by_the_capi() {
    let mut case = inline("roundtrip", &serde_json::json!({ "error": "UNSUPPORTED" }));
    case["input"]["dtype"] = "decimal128".into();
    run_case(&case).unwrap();
}

#[test]
fn spec_wrong_spec_version_fails() {
    let mut case = inline("len", &serde_json::json!({ "len": 3 }));
    case["spec_version"] = 2.into();
    assert!(run_case(&case).is_err());
}

#[test]
fn spec_value_is_compared_bit_by_bit() {
    let case = |value: Value| {
        let mut c = inline("median", &serde_json::json!({ "value": value }));
        c["input"]["dtype"] = "int64".into();
        c["input"]["values"] = serde_json::json!([1, 2, 3, 4]);
        c
    };
    run_case(&case(2.5.into())).unwrap();
    assert!(run_case(&case(2.0.into())).is_err());
    assert!(run_case(&case("NaN".into())).is_err());
}

#[test]
fn spec_percentile_without_p_fails() {
    let mut case = inline("percentile", &serde_json::json!({ "value": 1.0 }));
    case["args"] = serde_json::json!({});
    assert!(run_case(&case).is_err());
}
