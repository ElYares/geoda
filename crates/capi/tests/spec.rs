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
    GEODA_ERR_ALLOC, GEODA_ERR_FFI, GEODA_ERR_INTERNAL, GEODA_ERR_INVALID_ARGUMENT,
    GEODA_ERR_LENGTH_MISMATCH, GEODA_ERR_OUT_OF_BOUNDS, GEODA_ERR_PANIC, GEODA_ERR_TYPE_MISMATCH,
    GEODA_ERR_UNSUPPORTED, GEODA_OK, GeodaColumn, geoda_column_free, geoda_column_from_arrow,
    geoda_column_len, geoda_column_null_count, geoda_column_slice, geoda_column_to_arrow,
    geoda_last_error_message,
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

fn check_expect(expect: &Value, handle: &Handle) -> CaseResult {
    check_counts(expect, handle)?;
    check_exported(expect, handle)
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

/// Resultado de una operacion: un handle, o el codigo con el que fallo.
type OpResult = Result<Handle, i32>;

fn run_op(op: &str, case: &Value, handle: Handle) -> Result<OpResult, String> {
    match op {
        "roundtrip" | "len" | "null_count" => Ok(Ok(handle)),
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
                Ok(Handle(out))
            } else {
                Err(code)
            })
        }
        otro => Err(format!("op desconocida: {otro:?}")),
    }
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

    let imported = import_through_capi(&case["input"])?;
    let result = match imported {
        Ok(handle) => run_op(op, case, handle)?,
        Err(code) => Err(code),
    };
    match result {
        Ok(handle) if expect.get("error").is_some() => {
            let _ = handle;
            Err(format!(
                "esperaba el error {}, pero la operacion tuvo exito",
                expect["error"]
            ))
        }
        Ok(handle) => check_expect(expect, &handle),
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
