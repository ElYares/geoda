//! Errores de `geoda-core`.

use std::fmt;

/// Error de `geoda-core`. Las variantes con `String` llevan el detalle humano.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// El tipo recibido no es el esperado.
    TypeMismatch(String),
    /// Las longitudes no coinciden.
    LengthMismatch(String),
    /// Indice u offset fuera de rango.
    OutOfBounds(String),
    /// Argumento invalido.
    InvalidArgument(String),
    /// Operacion o tipo aun no soportado.
    Unsupported(String),
    /// Fallo al reservar memoria.
    Alloc(String),
    /// Datos mal formados que llegaron por la frontera.
    Ffi(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (categoria, detalle) = match self {
            Error::TypeMismatch(d) => ("tipo incorrecto", d),
            Error::LengthMismatch(d) => ("longitud incorrecta", d),
            Error::OutOfBounds(d) => ("fuera de rango", d),
            Error::InvalidArgument(d) => ("argumento invalido", d),
            Error::Unsupported(d) => ("no soportado", d),
            Error::Alloc(d) => ("memoria", d),
            Error::Ffi(d) => ("frontera", d),
        };
        write!(f, "{categoria}: {detalle}")
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn display_no_esta_vacio() {
        let e = Error::InvalidArgument("nombre repetido".into());
        assert_eq!(e.to_string(), "argumento invalido: nombre repetido");
    }

    #[test]
    fn implementa_std_error() {
        fn requiere<E: std::error::Error>(_: &E) {}
        requiere(&Error::Alloc("sin memoria".into()));
    }
}
