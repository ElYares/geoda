//! Builders: la via de construir una `Column` valida desde Rust seguro.
//!
//! Cada `finish` termina en [`Column::try_new`], asi que un builder nunca puede
//! producir una columna que `try_new` rechazaria.

use crate::memory::{Bitmap, Buffer, Native};
use crate::{Column, DataType, Error};

/// Buffer de validez a partir de un `bool` por valor. `None` si no hay nulos.
fn validity_buffer(validos: &[bool]) -> Result<Option<Buffer>, Error> {
    if validos.iter().all(|&v| v) {
        return Ok(None);
    }
    let mut bitmap = Bitmap::new(validos.len())?;
    for (i, _) in validos.iter().enumerate().filter(|(_, v)| **v) {
        bitmap.set(i, true)?;
    }
    Ok(Some(bitmap.into_buffer()))
}

/// Builder de columnas `Int32`, `Int64` y `Float64`.
#[derive(Debug, Clone)]
pub struct PrimitiveBuilder<T: Native> {
    bytes: Vec<u8>,
    validos: Vec<bool>,
    _tipo: std::marker::PhantomData<T>,
}

impl<T: Native> Default for PrimitiveBuilder<T> {
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            validos: Vec::new(),
            _tipo: std::marker::PhantomData,
        }
    }
}

impl<T: Native> PrimitiveBuilder<T> {
    /// Builder vacio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Agrega un valor.
    pub fn append(&mut self, valor: T) {
        let desde = self.bytes.len();
        self.bytes.resize(desde + T::WIDTH, 0);
        valor.write_ne(&mut self.bytes[desde..]);
        self.validos.push(true);
    }

    /// Agrega un nulo (su valor queda en cero, inicializado).
    pub fn append_null(&mut self) {
        self.bytes.resize(self.bytes.len() + T::WIDTH, 0);
        self.validos.push(false);
    }

    /// Cantidad de valores agregados, nulos incluidos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.validos.len()
    }

    /// `true` si no se agrego nada.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.validos.is_empty()
    }

    /// Termina y devuelve la columna.
    ///
    /// # Errors
    /// Los de [`Column::try_new`] y los de reservar memoria.
    pub fn finish(self) -> Result<Column, Error> {
        Column::try_new(
            T::DATA_TYPE,
            self.validos.len(),
            0,
            validity_buffer(&self.validos)?,
            vec![Buffer::from_bytes(&self.bytes)?],
        )
    }
}

/// Builder de columnas `Bool`.
#[derive(Debug, Clone, Default)]
pub struct BoolBuilder {
    valores: Vec<bool>,
    validos: Vec<bool>,
}

impl BoolBuilder {
    /// Builder vacio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Agrega un valor.
    pub fn append(&mut self, valor: bool) {
        self.valores.push(valor);
        self.validos.push(true);
    }

    /// Agrega un nulo (su valor queda en `false`).
    pub fn append_null(&mut self) {
        self.valores.push(false);
        self.validos.push(false);
    }

    /// Cantidad de valores agregados, nulos incluidos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.validos.len()
    }

    /// `true` si no se agrego nada.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.validos.is_empty()
    }

    /// Termina y devuelve la columna.
    ///
    /// # Errors
    /// Los de [`Column::try_new`] y los de reservar memoria.
    pub fn finish(self) -> Result<Column, Error> {
        let mut bits = Bitmap::new(self.valores.len())?;
        for (i, _) in self.valores.iter().enumerate().filter(|(_, v)| **v) {
            bits.set(i, true)?;
        }
        Column::try_new(
            DataType::Bool,
            self.validos.len(),
            0,
            validity_buffer(&self.validos)?,
            vec![bits.into_buffer()],
        )
    }
}

/// Builder de columnas `Utf8`.
#[derive(Debug, Clone)]
pub struct Utf8Builder {
    offsets: Vec<i32>,
    datos: Vec<u8>,
    validos: Vec<bool>,
}

impl Default for Utf8Builder {
    fn default() -> Self {
        Self {
            offsets: vec![0],
            datos: Vec::new(),
            validos: Vec::new(),
        }
    }
}

impl Utf8Builder {
    /// Builder vacio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Agrega una cadena.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si los datos acumulados superarian `i32::MAX`
    /// bytes (los offsets son `i32`).
    pub fn append(&mut self, valor: &str) -> Result<(), Error> {
        let nuevo = i32::try_from(self.datos.len() + valor.len()).map_err(|_| {
            Error::InvalidArgument("los datos de una columna Utf8 superan i32::MAX bytes".into())
        })?;
        self.datos.extend_from_slice(valor.as_bytes());
        self.offsets.push(nuevo);
        self.validos.push(true);
        Ok(())
    }

    /// Agrega un nulo (ocupa cero bytes de datos).
    pub fn append_null(&mut self) {
        let ultimo = self.offsets[self.offsets.len() - 1];
        self.offsets.push(ultimo);
        self.validos.push(false);
    }

    /// Cantidad de valores agregados, nulos incluidos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.validos.len()
    }

    /// `true` si no se agrego nada.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.validos.is_empty()
    }

    /// Termina y devuelve la columna.
    ///
    /// # Errors
    /// Los de [`Column::try_new`] y los de reservar memoria.
    pub fn finish(self) -> Result<Column, Error> {
        let offsets: Vec<u8> = self.offsets.iter().flat_map(|o| o.to_ne_bytes()).collect();
        Column::try_new(
            DataType::Utf8,
            self.validos.len(),
            0,
            validity_buffer(&self.validos)?,
            vec![
                Buffer::from_bytes(&offsets)?,
                Buffer::from_bytes(&self.datos)?,
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{BoolBuilder, PrimitiveBuilder, Utf8Builder};
    use crate::DataType;

    #[test]
    fn primitivo_sin_nulos_no_lleva_validity() {
        let mut b = PrimitiveBuilder::<i64>::new();
        b.append(-1);
        b.append(i64::MAX);
        let c = b.finish().unwrap();
        assert_eq!(c.dtype(), DataType::Int64);
        assert!(c.validity().is_none());
        assert_eq!(c.null_count(), 0);
        assert_eq!(c.values::<i64>().unwrap(), &[-1, i64::MAX]);
    }

    #[test]
    fn float64_con_nulos() {
        let mut b = PrimitiveBuilder::<f64>::new();
        b.append(1.5);
        b.append_null();
        b.append(f64::NAN);
        let c = b.finish().unwrap();
        assert_eq!(c.null_count(), 1);
        assert_eq!(c.values::<f64>().unwrap()[0].to_bits(), 1.5_f64.to_bits());
        assert!(c.values::<f64>().unwrap()[2].is_nan());
    }

    #[test]
    fn valores_arrancan_alineados_a_64() {
        let mut b = PrimitiveBuilder::<i32>::new();
        (0..100).for_each(|i| b.append(i));
        let c = b.finish().unwrap();
        assert_eq!(c.buffers()[0].as_ptr() as usize % 64, 0);
    }

    #[test]
    fn vacio_es_una_columna_valida() {
        assert_eq!(PrimitiveBuilder::<i32>::new().finish().unwrap().len(), 0);
        assert_eq!(BoolBuilder::new().finish().unwrap().len(), 0);
        let c = Utf8Builder::new().finish().unwrap();
        assert!(c.is_empty() && c.null_count() == 0);
    }

    #[test]
    fn bool_con_nulos() {
        let mut b = BoolBuilder::new();
        b.append(true);
        b.append_null();
        b.append(false);
        b.append(true);
        let c = b.finish().unwrap();
        assert_eq!(c.null_count(), 1);
        let leidos: Vec<bool> = (0..4).map(|i| c.bool_value(i).unwrap()).collect();
        assert_eq!(leidos, [true, false, false, true]);
        assert!(c.is_null(1).unwrap());
    }

    #[test]
    fn utf8_con_vacios_nulos_y_multibyte() {
        let mut b = Utf8Builder::new();
        b.append("año").unwrap();
        b.append_null();
        b.append("").unwrap();
        b.append("日本").unwrap();
        assert_eq!(b.len(), 4);
        let c = b.finish().unwrap();
        assert_eq!(c.null_count(), 1);
        assert_eq!(c.str_value(0).unwrap(), "año");
        assert!(c.is_null(1).unwrap());
        assert_eq!(c.str_value(1).unwrap(), "");
        assert_eq!(c.str_value(2).unwrap(), "");
        assert_eq!(c.str_value(3).unwrap(), "日本");
    }
}
