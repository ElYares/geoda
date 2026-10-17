//! `Column`: datos de un solo tipo, inmutables y baratos de clonar.

use crate::memory::{Buffer, Native, count_set_bits, get_bit};
use crate::{DataType, Error};

/// Columna inmutable: tipo, ventana `[offset, offset + len)` y buffers compartidos.
///
/// Solo se obtiene por [`Column::try_new`] (o un builder, que pasa por ella), que
/// valida los invariantes 2.3 de la arquitectura. Por eso el resto del crate puede
/// leer sus buffers sin volver a comprobar rangos. Clonar o cortar no copia bytes:
/// los buffers se comparten por `Arc`.
///
/// Layout de `buffers` por tipo:
/// - `Bool`: `[valores]`, un bit por valor.
/// - `Int32`, `Int64`, `Float64`: `[valores]`.
/// - `Utf8`: `[offsets i32, datos utf-8]`.
///
/// Los valores en posiciones nulas estan inicializados pero no tienen significado.
#[derive(Debug, Clone)]
pub struct Column {
    dtype: DataType,
    len: usize,
    offset: usize,
    validity: Option<Buffer>,
    buffers: Vec<Buffer>,
    null_count: usize,
}

impl Column {
    /// Construye una columna validando todos sus invariantes.
    ///
    /// `validity` es opcional: `None` significa "sin nulos". Si existe, el bit `i`
    /// (orden LSB) en 1 es valido y en 0 es nulo, y se indexa con `offset + i`.
    ///
    /// # Errors
    /// - [`Error::OutOfBounds`] si `offset + len` desborda.
    /// - [`Error::LengthMismatch`] si faltan o sobran buffers, o son muy cortos
    ///   para `offset + len`.
    /// - [`Error::InvalidArgument`] si los offsets de `Utf8` no son validos o el
    ///   buffer de offsets esta desalineado.
    /// - [`Error::Ffi`] si un valor `Utf8` no es UTF-8 valido.
    pub fn try_new(
        dtype: DataType,
        len: usize,
        offset: usize,
        validity: Option<Buffer>,
        buffers: Vec<Buffer>,
    ) -> Result<Self, Error> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Error::OutOfBounds(format!("offset {offset} + len {len} desborda")))?;

        if let Some(v) = &validity {
            require_bytes(v, end.div_ceil(8), "validity")?;
        }
        match dtype {
            DataType::Bool => {
                let [valores] = buffers.as_slice() else {
                    return Err(wrong_buffer_count(dtype, 1, buffers.len()));
                };
                require_bytes(valores, end.div_ceil(8), "valores")?;
            }
            DataType::Int32 => check_primitive::<i32>(dtype, end, &buffers)?,
            DataType::Int64 => check_primitive::<i64>(dtype, end, &buffers)?,
            DataType::Float64 => check_primitive::<f64>(dtype, end, &buffers)?,
            DataType::Utf8 => {
                let [offsets, datos] = buffers.as_slice() else {
                    return Err(wrong_buffer_count(dtype, 2, buffers.len()));
                };
                check_utf8(offsets, datos, offset, end)?;
            }
        }

        let null_count = match &validity {
            None => 0,
            Some(v) => len - count_set_bits(v.as_slice(), offset, len),
        };
        Ok(Self {
            dtype,
            len,
            offset,
            validity,
            buffers,
            null_count,
        })
    }

    /// Tipo de dato.
    #[must_use]
    pub fn dtype(&self) -> DataType {
        self.dtype
    }

    /// Cantidad de valores (nulos incluidos).
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` si no tiene valores.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Posicion del primer valor dentro de los buffers.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Cantidad de nulos, calculada al construir y siempre coherente con `validity`.
    #[must_use]
    pub fn null_count(&self) -> usize {
        self.null_count
    }

    /// Buffer de validez, si la columna puede tener nulos.
    #[must_use]
    pub fn validity(&self) -> Option<&Buffer> {
        self.validity.as_ref()
    }

    /// Buffers de datos (ver el layout por tipo en la documentacion del tipo).
    #[must_use]
    pub fn buffers(&self) -> &[Buffer] {
        &self.buffers
    }

    /// `true` si el valor `i` es nulo.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `i >= len`.
    pub fn is_null(&self, i: usize) -> Result<bool, Error> {
        self.check_index(i)?;
        Ok(self
            .validity
            .as_ref()
            .is_some_and(|v| !get_bit(v.as_slice(), self.offset + i)))
    }

    /// Valores de una columna `Int32`, `Int64` o `Float64`, como slice (incluye los
    /// valores en posiciones nulas; consultar [`Column::is_null`]).
    ///
    /// # Errors
    /// [`Error::TypeMismatch`] si `T` no es el tipo de la columna.
    pub fn values<T: Native>(&self) -> Result<&[T], Error> {
        if self.dtype != T::DATA_TYPE {
            return Err(Error::TypeMismatch(format!(
                "la columna es {:?}, se pidio {:?}",
                self.dtype,
                T::DATA_TYPE
            )));
        }
        // `try_new` ya comprobo que el buffer cubre `offset + len` valores.
        self.buffers[0]
            .typed::<T>()?
            .get(self.offset..self.offset + self.len)
            .ok_or_else(|| Error::OutOfBounds("valores fuera del buffer".into()))
    }

    /// Valor `i` de una columna `Bool`.
    ///
    /// # Errors
    /// [`Error::TypeMismatch`] si no es `Bool`; [`Error::OutOfBounds`] si `i >= len`.
    pub fn bool_value(&self, i: usize) -> Result<bool, Error> {
        self.expect(DataType::Bool)?;
        self.check_index(i)?;
        Ok(get_bit(self.buffers[0].as_slice(), self.offset + i))
    }

    /// Valor `i` de una columna `Utf8`.
    ///
    /// # Errors
    /// [`Error::TypeMismatch`] si no es `Utf8`; [`Error::OutOfBounds`] si `i >= len`.
    pub fn str_value(&self, i: usize) -> Result<&str, Error> {
        self.expect(DataType::Utf8)?;
        self.check_index(i)?;
        let offsets = self.buffers[0].typed::<i32>()?;
        let (desde, hasta) = (offsets[self.offset + i], offsets[self.offset + i + 1]);
        let datos = self.buffers[1].as_slice();
        // Re-valida en vez de confiar: es la unica forma segura de devolver `&str`.
        std::str::from_utf8(&datos[to_usize(desde)?..to_usize(hasta)?])
            .map_err(|e| Error::Ffi(format!("utf-8 invalido en el valor {i}: {e}")))
    }

    /// Sub-columna `[offset, offset + len)` sobre los mismos buffers, sin copiar.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si la ventana se sale de la columna.
    pub fn slice(&self, offset: usize, len: usize) -> Result<Self, Error> {
        offset
            .checked_add(len)
            .filter(|&end| end <= self.len)
            .ok_or_else(|| {
                Error::OutOfBounds(format!(
                    "slice({offset}, {len}) sobre una columna de {} valores",
                    self.len
                ))
            })?;
        let offset = self.offset + offset;
        // Los invariantes ya se cumplen para el tramo completo, asi que se cumplen
        // para cualquier sub-tramo: solo hay que recontar los nulos.
        let null_count = match &self.validity {
            None => 0,
            Some(v) => len - count_set_bits(v.as_slice(), offset, len),
        };
        Ok(Self {
            dtype: self.dtype,
            len,
            offset,
            validity: self.validity.clone(),
            buffers: self.buffers.clone(),
            null_count,
        })
    }

    fn check_index(&self, i: usize) -> Result<(), Error> {
        if i < self.len {
            Ok(())
        } else {
            Err(Error::OutOfBounds(format!(
                "indice {i} en una columna de {} valores",
                self.len
            )))
        }
    }

    fn expect(&self, esperado: DataType) -> Result<(), Error> {
        if self.dtype == esperado {
            Ok(())
        } else {
            Err(Error::TypeMismatch(format!(
                "la columna es {:?}, se pidio {esperado:?}",
                self.dtype
            )))
        }
    }
}

fn to_usize(offset: i32) -> Result<usize, Error> {
    usize::try_from(offset)
        .map_err(|_| Error::InvalidArgument(format!("offset negativo: {offset}")))
}

fn wrong_buffer_count(dtype: DataType, esperados: usize, recibidos: usize) -> Error {
    Error::LengthMismatch(format!(
        "{dtype:?} lleva {esperados} buffer(s), llegaron {recibidos}"
    ))
}

fn require_bytes(buffer: &Buffer, necesarios: usize, nombre: &str) -> Result<(), Error> {
    if buffer.len() >= necesarios {
        Ok(())
    } else {
        Err(Error::LengthMismatch(format!(
            "{nombre}: {} bytes, hacen falta al menos {necesarios}",
            buffer.len()
        )))
    }
}

fn check_primitive<T: Native>(
    dtype: DataType,
    end: usize,
    buffers: &[Buffer],
) -> Result<(), Error> {
    let [valores] = buffers else {
        return Err(wrong_buffer_count(dtype, 1, buffers.len()));
    };
    let necesarios = end.checked_mul(T::WIDTH).ok_or_else(|| {
        Error::OutOfBounds(format!("{end} valores de {} bytes desbordan", T::WIDTH))
    })?;
    require_bytes(valores, necesarios, "valores")
}

/// Invariante 4: offsets `i32` monotonicos sobre el tramo usado, dentro de `datos`,
/// y cada valor es UTF-8 valido por si solo (no basta validar el bloque entero:
/// un offset en medio de un caracter lo partiria).
fn check_utf8(offsets: &Buffer, datos: &Buffer, offset: usize, end: usize) -> Result<(), Error> {
    let todos = offsets.typed::<i32>()?;
    let tramo = todos.get(offset..=end).ok_or_else(|| {
        Error::LengthMismatch(format!(
            "offsets: {} valores, hacen falta al menos {}",
            todos.len(),
            end.saturating_add(1)
        ))
    })?;
    let bytes = datos.as_slice();
    let mut previo = to_usize(tramo[0])?;
    for (i, &actual) in tramo.iter().enumerate().skip(1) {
        let actual = to_usize(actual)?;
        if actual < previo {
            return Err(Error::InvalidArgument(format!(
                "offsets decrecientes en la posicion {}: {previo} -> {actual}",
                offset + i
            )));
        }
        let valor = bytes.get(previo..actual).ok_or_else(|| {
            Error::OutOfBounds(format!(
                "offset {actual} fuera de los {} bytes de datos",
                bytes.len()
            ))
        })?;
        std::str::from_utf8(valor).map_err(|e| {
            Error::Ffi(format!(
                "utf-8 invalido en el valor {}: {e}",
                offset + i - 1
            ))
        })?;
        previo = actual;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Column;
    use crate::memory::Buffer;
    use crate::{DataType, Error, PrimitiveBuilder, Utf8Builder};

    fn i32s(v: &[i32]) -> Buffer {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_ne_bytes()).collect();
        Buffer::from_bytes(&bytes).unwrap()
    }

    fn bytes(b: &[u8]) -> Buffer {
        Buffer::from_bytes(b).unwrap()
    }

    fn utf8(offsets: &[i32], datos: &[u8], len: usize) -> Result<Column, Error> {
        Column::try_new(
            DataType::Utf8,
            len,
            0,
            None,
            vec![i32s(offsets), bytes(datos)],
        )
    }

    /// `[Some(1), None, Some(3), None, None]`
    fn con_nulos() -> Column {
        let mut b = PrimitiveBuilder::<i32>::new();
        b.append(1);
        b.append_null();
        b.append(3);
        b.append_null();
        b.append_null();
        b.finish().unwrap()
    }

    // Invariante 2: cantidad y tamano de buffers por tipo.

    #[test]
    fn primitiva_valida_con_offset() {
        let c = Column::try_new(DataType::Int32, 2, 2, None, vec![i32s(&[1, 2, 3, 4])]).unwrap();
        assert_eq!(c.values::<i32>().unwrap(), &[3, 4]);
    }

    #[test]
    fn buffer_de_valores_muy_corto_es_err() {
        // 3 valores de i32 piden 12 bytes; hay 8.
        let r = Column::try_new(DataType::Int32, 3, 0, None, vec![i32s(&[1, 2])]);
        assert!(matches!(r, Err(Error::LengthMismatch(_))));
        // offset + len cuenta: 2 + 3 valores piden 20 bytes; hay 16.
        let r = Column::try_new(DataType::Int32, 3, 2, None, vec![i32s(&[1, 2, 3, 4])]);
        assert!(matches!(r, Err(Error::LengthMismatch(_))));
    }

    #[test]
    fn cantidad_de_buffers_incorrecta_es_err() {
        for (dtype, buffers) in [
            (DataType::Int64, vec![]),
            (DataType::Float64, vec![bytes(&[0; 8]), bytes(&[0; 8])]),
            (DataType::Bool, vec![]),
            (DataType::Utf8, vec![i32s(&[0, 0])]),
        ] {
            let r = Column::try_new(dtype, 1, 0, None, buffers);
            assert!(matches!(r, Err(Error::LengthMismatch(_))), "{dtype:?}");
        }
    }

    #[test]
    fn bool_con_pocos_bits_es_err() {
        // 9 valores piden 2 bytes.
        let r = Column::try_new(DataType::Bool, 9, 0, None, vec![bytes(&[0xFF])]);
        assert!(matches!(r, Err(Error::LengthMismatch(_))));
    }

    #[test]
    fn offset_mas_len_que_desborda_es_err() {
        let r = Column::try_new(DataType::Int32, usize::MAX, 1, None, vec![i32s(&[])]);
        assert!(matches!(r, Err(Error::OutOfBounds(_))));
        let r = Column::try_new(DataType::Int64, usize::MAX / 2, 0, None, vec![bytes(&[])]);
        assert!(matches!(r, Err(Error::OutOfBounds(_))));
    }

    // Invariante 3: validez.

    #[test]
    fn validity_que_no_cubre_offset_mas_len_es_err() {
        // 9 valores piden 2 bytes de validez.
        let r = Column::try_new(
            DataType::Int32,
            9,
            0,
            Some(bytes(&[0xFF])),
            vec![i32s(&[0; 9])],
        );
        assert!(matches!(r, Err(Error::LengthMismatch(_))));
    }

    #[test]
    fn null_count_sale_de_los_bits_en_cero() {
        let c = con_nulos();
        assert_eq!(c.len(), 5);
        assert_eq!(c.null_count(), 3);
        assert_eq!(
            [c.is_null(0), c.is_null(1), c.is_null(2)].map(Result::unwrap),
            [false, true, false]
        );
    }

    #[test]
    fn sin_validity_no_hay_nulos() {
        let c = Column::try_new(DataType::Int32, 2, 0, None, vec![i32s(&[1, 2])]).unwrap();
        assert_eq!(c.null_count(), 0);
        assert!(!c.is_null(1).unwrap());
    }

    #[test]
    fn is_null_fuera_de_rango_es_err() {
        assert!(matches!(con_nulos().is_null(5), Err(Error::OutOfBounds(_))));
    }

    // Invariante 4: Utf8.

    #[test]
    fn utf8_valida() {
        let c = utf8(&[0, 2, 2, 5], b"hola!", 3).unwrap();
        assert_eq!(c.str_value(0).unwrap(), "ho");
        assert_eq!(c.str_value(1).unwrap(), "");
        assert_eq!(c.str_value(2).unwrap(), "la!");
    }

    #[test]
    fn utf8_con_bytes_invalidos_es_err() {
        let r = utf8(&[0, 2], &[0xFF, 0xFE], 1);
        assert!(matches!(r, Err(Error::Ffi(_))));
    }

    #[test]
    fn utf8_con_offset_en_medio_de_un_caracter_es_err() {
        // "é" son 2 bytes (C3 A9): partirlo en [0,1,2] deja dos mitades invalidas
        // aunque el bloque entero sea UTF-8 valido.
        let r = utf8(&[0, 1, 2], "é".as_bytes(), 2);
        assert!(matches!(r, Err(Error::Ffi(_))));
    }

    #[test]
    fn utf8_con_offsets_decrecientes_es_err() {
        let r = utf8(&[0, 3, 1], b"abc", 2);
        assert!(matches!(r, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn utf8_con_primer_offset_negativo_es_err() {
        let r = utf8(&[-1, 2], b"ab", 1);
        assert!(matches!(r, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn utf8_con_offset_mas_alla_de_los_datos_es_err() {
        let r = utf8(&[0, 9], b"abc", 1);
        assert!(matches!(r, Err(Error::OutOfBounds(_))));
    }

    #[test]
    fn utf8_con_pocos_offsets_es_err() {
        // 3 valores piden 4 offsets.
        let r = utf8(&[0, 1, 2], b"abc", 3);
        assert!(matches!(r, Err(Error::LengthMismatch(_))));
    }

    #[test]
    fn utf8_con_offsets_desalineados_es_err() {
        // Un slice de 2 bytes deja el buffer de offsets mal alineado para i32.
        let desalineado = i32s(&[0, 0, 1, 2]).slice(2, 12).unwrap();
        let r = Column::try_new(DataType::Utf8, 2, 0, None, vec![desalineado, bytes(b"ab")]);
        assert!(matches!(r, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn utf8_solo_valida_el_tramo_usado() {
        // El valor 1 es invalido, pero la columna es solo el valor 0.
        let c = Column::try_new(
            DataType::Utf8,
            1,
            0,
            None,
            vec![i32s(&[0, 1, 3]), bytes(&[b'a', 0xFF, 0xFE])],
        )
        .unwrap();
        assert_eq!(c.str_value(0).unwrap(), "a");
    }

    // slice y clonado.

    #[test]
    fn slice_recuenta_los_nulos_del_tramo() {
        let c = con_nulos();
        for (offset, len, esperados) in [(0, 5, 3), (1, 3, 2), (2, 1, 0), (3, 2, 2), (5, 0, 0)] {
            let s = c.slice(offset, len).unwrap();
            let contados = (0..len).filter(|&i| s.is_null(i).unwrap()).count();
            assert_eq!(s.null_count(), esperados, "slice({offset}, {len})");
            assert_eq!(s.null_count(), contados, "slice({offset}, {len})");
        }
    }

    #[test]
    fn slice_de_slice_acumula_el_offset() {
        let s = con_nulos().slice(1, 4).unwrap().slice(1, 2).unwrap();
        assert_eq!(s.offset(), 2);
        assert_eq!(s.values::<i32>().unwrap()[0], 3);
        assert!(!s.is_null(0).unwrap());
        assert!(s.is_null(1).unwrap());
    }

    #[test]
    fn slice_fuera_de_rango_es_err() {
        let c = con_nulos();
        assert!(matches!(c.slice(6, 0), Err(Error::OutOfBounds(_))));
        assert!(matches!(c.slice(3, 3), Err(Error::OutOfBounds(_))));
        assert!(matches!(c.slice(usize::MAX, 2), Err(Error::OutOfBounds(_))));
    }

    #[test]
    fn slice_de_utf8_lee_el_tramo() {
        let mut b = Utf8Builder::new();
        for s in ["uno", "dos", "tres", "cuatro"] {
            b.append(s).unwrap();
        }
        let s = b.finish().unwrap().slice(1, 2).unwrap();
        assert_eq!(
            (s.str_value(0).unwrap(), s.str_value(1).unwrap()),
            ("dos", "tres")
        );
        assert!(matches!(s.str_value(2), Err(Error::OutOfBounds(_))));
    }

    #[test]
    fn clonar_y_cortar_comparten_los_buffers() {
        let c = con_nulos();
        let clon = c.clone();
        let corte = c.slice(1, 3).unwrap();
        for otra in [&clon, &corte] {
            assert_eq!(c.buffers()[0].as_ptr(), otra.buffers()[0].as_ptr());
            assert_eq!(
                c.validity().unwrap().as_ptr(),
                otra.validity().unwrap().as_ptr()
            );
        }
    }

    // Acceso tipado.

    #[test]
    fn values_con_otro_tipo_es_err() {
        assert!(matches!(
            con_nulos().values::<i64>(),
            Err(Error::TypeMismatch(_))
        ));
    }

    #[test]
    fn valores_nulos_estan_inicializados_en_cero() {
        assert_eq!(con_nulos().values::<i32>().unwrap(), &[1, 0, 3, 0, 0]);
    }

    #[test]
    fn bool_value_y_str_value_exigen_su_tipo() {
        let c = con_nulos();
        assert!(matches!(c.bool_value(0), Err(Error::TypeMismatch(_))));
        assert!(matches!(c.str_value(0), Err(Error::TypeMismatch(_))));
    }
}
