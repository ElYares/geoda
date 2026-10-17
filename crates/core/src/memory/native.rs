//! Tipos numericos de ancho fijo que se pueden leer directo desde un `Buffer`.

use crate::DataType;

mod sealed {
    pub trait Sealed {}
}

/// Tipo primitivo de ancho fijo.
///
/// Sellado a proposito: `Buffer::typed` reinterpreta bytes como `[T]`, y eso solo
/// es seguro si todo patron de bits es un valor valido de `T` (cierto para enteros
/// y flotantes, falso para `bool` o `char`). Al no poder implementarlo nadie fuera
/// de este modulo, esa garantia no se puede romper.
pub trait Native: Copy + sealed::Sealed + 'static {
    /// Tipo logico que representa.
    const DATA_TYPE: DataType;
    /// Bytes por valor.
    const WIDTH: usize;
    /// Escribe el valor en `out` (de largo `WIDTH`) en el orden de bytes nativo.
    fn write_ne(self, out: &mut [u8]);
}

macro_rules! native {
    ($t:ty, $dt:expr) => {
        impl sealed::Sealed for $t {}
        impl Native for $t {
            const DATA_TYPE: DataType = $dt;
            const WIDTH: usize = std::mem::size_of::<$t>();
            fn write_ne(self, out: &mut [u8]) {
                out.copy_from_slice(&self.to_ne_bytes());
            }
        }
    };
}

native!(i32, DataType::Int32);
native!(i64, DataType::Int64);
native!(f64, DataType::Float64);

#[cfg(test)]
mod tests {
    use super::Native;
    use crate::DataType;

    #[test]
    fn anchos_y_tipos() {
        assert_eq!((i32::WIDTH, i32::DATA_TYPE), (4, DataType::Int32));
        assert_eq!((i64::WIDTH, i64::DATA_TYPE), (8, DataType::Int64));
        assert_eq!((f64::WIDTH, f64::DATA_TYPE), (8, DataType::Float64));
    }

    #[test]
    fn write_ne_ida_y_vuelta() {
        let mut out = [0_u8; 8];
        (-7_i64).write_ne(&mut out);
        assert_eq!(i64::from_ne_bytes(out), -7);
    }
}
