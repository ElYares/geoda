//! Codigos de error, buffer del ultimo error por hilo y la guarda contra panics.

use std::cell::RefCell;
use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

use geoda_core::Error;

/// Exito.
pub const GEODA_OK: i32 = 0;
/// Argumento invalido (por ejemplo, un puntero NULL).
pub const GEODA_ERR_INVALID_ARGUMENT: i32 = -1;
/// El tipo recibido no es el esperado.
pub const GEODA_ERR_TYPE_MISMATCH: i32 = -2;
/// Las longitudes no coinciden.
pub const GEODA_ERR_LENGTH_MISMATCH: i32 = -3;
/// Indice u offset fuera de rango.
pub const GEODA_ERR_OUT_OF_BOUNDS: i32 = -4;
/// Operacion o tipo aun no soportado.
pub const GEODA_ERR_UNSUPPORTED: i32 = -5;
/// Fallo al reservar memoria.
pub const GEODA_ERR_ALLOC: i32 = -6;
/// Datos mal formados que llegaron por la frontera.
pub const GEODA_ERR_FFI: i32 = -7;
/// Un panic interno fue atrapado antes de cruzar la frontera.
pub const GEODA_ERR_PANIC: i32 = -8;
/// Error interno que no encaja en ninguna otra categoria.
pub const GEODA_ERR_INTERNAL: i32 = -9;

thread_local! {
    /// Mensaje del ultimo error de este hilo (D4). `None` = ninguno aun.
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Guarda el mensaje del ultimo error del hilo. Los bytes cero se reemplazan para
/// que quepa en una cadena C.
pub(crate) fn set_last_error(mensaje: &str) {
    let limpio = CString::new(mensaje.replace('\0', "\u{fffd}")).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(limpio));
}

/// Puntero al ultimo mensaje del hilo, o a una cadena vacia si no hay.
pub(crate) fn last_error_message() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ref().map_or(c"".as_ptr(), |m| m.as_ptr()))
}

/// Error listo para cruzar la frontera: un codigo y su mensaje.
pub(crate) struct CError {
    code: i32,
    message: String,
}

impl From<Error> for CError {
    fn from(e: Error) -> Self {
        let code = match e {
            Error::InvalidArgument(_) => GEODA_ERR_INVALID_ARGUMENT,
            Error::TypeMismatch(_) => GEODA_ERR_TYPE_MISMATCH,
            Error::LengthMismatch(_) => GEODA_ERR_LENGTH_MISMATCH,
            Error::OutOfBounds(_) => GEODA_ERR_OUT_OF_BOUNDS,
            Error::Unsupported(_) => GEODA_ERR_UNSUPPORTED,
            Error::Alloc(_) => GEODA_ERR_ALLOC,
            Error::Ffi(_) => GEODA_ERR_FFI,
            // `Error` es `non_exhaustive`: una variante nueva llega aqui hasta que
            // se le asigne codigo propio.
            _ => GEODA_ERR_INTERNAL,
        };
        CError {
            code,
            message: e.to_string(),
        }
    }
}

/// Error de uso de la API desde C (no viene de `geoda-core`).
pub(crate) fn invalid_argument(que: &str) -> CError {
    CError {
        code: GEODA_ERR_INVALID_ARGUMENT,
        message: Error::InvalidArgument(que.to_owned()).to_string(),
    }
}

/// Corre `f` sin dejar que un panic cruce la frontera de C (cruzarla es
/// comportamiento indefinido, o aborta). Devuelve `GEODA_OK` o el codigo negativo
/// del error y, en ese caso, deja el mensaje en el buffer del hilo.
pub(crate) fn guard<F>(f: F) -> i32
where
    F: FnOnce() -> Result<(), CError>,
{
    // `AssertUnwindSafe`: si `f` entra en panic, no se vuelve a mirar su estado
    // (solo se guarda el mensaje y se devuelve el codigo).
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(())) => GEODA_OK,
        Ok(Err(e)) => {
            set_last_error(&e.message);
            e.code
        }
        Err(payload) => {
            let detalle = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "sin detalle".to_owned());
            set_last_error(&format!("panic interno: {detalle}"));
            GEODA_ERR_PANIC
        }
    }
}
