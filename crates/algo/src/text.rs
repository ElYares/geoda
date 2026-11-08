//! Texto y similitud sobre columnas: la capa que usan la capi y Python.
//!
//! Envuelve [`edit_distance`], [`lis`] y [`Huffman`] con el convenio de columnas del resto
//! del proyecto (decision 006): entran y salen columnas, nunca estructuras de Rust.
//!
//! - Los bytes de Huffman viajan como columna `Int32` con valores `0..=255` (geoda no tiene
//!   un tipo de byte) y los bits codificados como columna `Bool` de `bit_len` filas.
//! - La tabla de Huffman no viaja: se reconstruye a partir de las **frecuencias**, una columna
//!   `Int64` de 256 filas. El mismo vector da siempre la misma tabla (desempate fijo).

use geoda_core::{BoolBuilder, Column, DataType, Error, PrimitiveBuilder};

use crate::edit::edit_distance;
use crate::huffman::Huffman;
use crate::lis::lis;

/// Distancia de edicion fila a fila entre dos columnas `Utf8`.
///
/// Las dos miden lo mismo, o una de ellas mide 1 y se compara con todas las filas de la otra
/// (una cadena frente a una columna). Una fila nula en cualquiera de las dos da nulo. Sale una
/// columna `Int64`.
///
/// # Errors
/// [`Error::TypeMismatch`] si alguna no es `Utf8`; [`Error::LengthMismatch`] si miden distinto
/// y ninguna mide 1.
pub fn edit_distance_columns(a: &Column, b: &Column) -> Result<Column, Error> {
    for column in [a, b] {
        if column.dtype() != DataType::Utf8 {
            return Err(Error::TypeMismatch(format!(
                "la distancia de edicion pide Utf8, no {:?}",
                column.dtype()
            )));
        }
    }
    let rows = match (a.len(), b.len()) {
        (x, y) if x == y => x,
        (1, y) => y,
        (x, 1) => x,
        (x, y) => {
            return Err(Error::LengthMismatch(format!(
                "las columnas miden {x} y {y}: deben medir lo mismo o una de ellas 1"
            )));
        }
    };
    let row_of = |column: &Column, i: usize| if column.len() == 1 { 0 } else { i };
    let mut builder = PrimitiveBuilder::<i64>::new();
    for i in 0..rows {
        let (i_a, i_b) = (row_of(a, i), row_of(b, i));
        if a.is_null(i_a)? || b.is_null(i_b)? {
            builder.append_null();
        } else {
            let distance = edit_distance(a.str_value(i_a)?, b.str_value(i_b)?);
            builder.append(i64::try_from(distance).map_err(|_| {
                Error::OutOfBounds(format!("la distancia {distance} no cabe en i64"))
            })?);
        }
    }
    builder.finish()
}

/// Posiciones de la subsecuencia estrictamente creciente mas larga de una columna `Int32`,
/// `Int64` o `Float64`, como columna `Int64` de menor a mayor posicion.
///
/// # Errors
/// [`Error::InvalidArgument`] si hay algun nulo o `NaN`; [`Error::Unsupported`] para `Bool` y
/// `Utf8`.
pub fn lis_column(column: &Column) -> Result<Column, Error> {
    if column.null_count() > 0 {
        return Err(Error::InvalidArgument(
            "la subsecuencia creciente no admite nulos".to_string(),
        ));
    }
    let result = match column.dtype() {
        DataType::Int32 => lis(column.values::<i32>()?)?,
        DataType::Int64 => lis(column.values::<i64>()?)?,
        DataType::Float64 => lis(column.values::<f64>()?)?,
        other => {
            return Err(Error::Unsupported(format!(
                "la subsecuencia creciente no esta definida para {other:?}"
            )));
        }
    };
    let mut builder = PrimitiveBuilder::<i64>::new();
    for index in result.indices {
        builder.append(
            i64::try_from(index)
                .map_err(|_| Error::OutOfBounds(format!("{index} no cabe en i64")))?,
        );
    }
    builder.finish()
}

/// Codifica con Huffman una columna `Int32` de bytes (`0..=255`, sin nulos). Devuelve
/// `(bits, frecuencias)`: una columna `Bool` con los bits codificados y una `Int64` de 256
/// filas con cuantas veces aparece cada byte, que es todo lo necesario para decodificar.
///
/// # Errors
/// [`Error::TypeMismatch`] si no es `Int32`; [`Error::InvalidArgument`] si hay nulos o un valor
/// fuera de `0..=255`.
pub fn huffman_encode_column(data: &Column) -> Result<(Column, Column), Error> {
    let bytes = column_bytes(data)?;
    let huffman = Huffman::from_data(&bytes)?;
    let encoded = huffman.encode(&bytes)?;
    let mut bits = BoolBuilder::new();
    for position in 0..encoded.bit_len {
        bits.append((encoded.bytes[position / 8] >> (7 - position % 8)) & 1 == 1);
    }
    let mut frequencies = PrimitiveBuilder::<i64>::new();
    for count in crate::huffman::frequencies(&bytes) {
        frequencies.append(
            i64::try_from(count)
                .map_err(|_| Error::OutOfBounds(format!("{count} no cabe en i64")))?,
        );
    }
    Ok((bits.finish()?, frequencies.finish()?))
}

/// Decodifica una columna `Bool` de bits con la tabla que sale de las frecuencias (`Int64`, 256
/// filas). Devuelve una columna `Int32` de bytes.
///
/// # Errors
/// [`Error::TypeMismatch`] si los tipos no son `Int64` y `Bool`; [`Error::InvalidArgument`] si
/// hay nulos, las frecuencias no son 256 valores no negativos, o los bits no se pueden decodificar
/// con esa tabla.
pub fn huffman_decode_column(frequencies: &Column, bits: &Column) -> Result<Column, Error> {
    if frequencies.dtype() != DataType::Int64 || bits.dtype() != DataType::Bool {
        return Err(Error::TypeMismatch(format!(
            "se esperaba Int64 y Bool, no {:?} y {:?}",
            frequencies.dtype(),
            bits.dtype()
        )));
    }
    if frequencies.null_count() > 0 || bits.null_count() > 0 {
        return Err(Error::InvalidArgument(
            "las frecuencias y los bits no admiten nulos".to_string(),
        ));
    }
    let huffman = huffman_from_counts(frequencies.values::<i64>()?)?;
    let mut packed = vec![0u8; bits.len().div_ceil(8)];
    for position in 0..bits.len() {
        if bits.bool_value(position)? {
            packed[position / 8] |= 0x80 >> (position % 8);
        }
    }
    let decoded = huffman.decode(&packed, bits.len())?;
    let mut out = PrimitiveBuilder::<i32>::new();
    for byte in decoded {
        out.append(i32::from(byte));
    }
    out.finish()
}

/// Tabla de Huffman a partir de 256 frecuencias (una por byte).
///
/// # Errors
/// [`Error::InvalidArgument`] si no son 256 valores o alguno es negativo.
pub fn huffman_from_counts(counts: &[i64]) -> Result<Huffman, Error> {
    if counts.len() != 256 {
        return Err(Error::InvalidArgument(format!(
            "se esperaban 256 frecuencias, no {}",
            counts.len()
        )));
    }
    let mut table = [0u64; 256];
    for (slot, &count) in table.iter_mut().zip(counts) {
        *slot = u64::try_from(count)
            .map_err(|_| Error::InvalidArgument(format!("frecuencia negativa: {count}")))?;
    }
    Huffman::from_frequencies(&table)
}

/// Los bytes de una columna `Int32` (`0..=255`, sin nulos).
fn column_bytes(data: &Column) -> Result<Vec<u8>, Error> {
    if data.dtype() != DataType::Int32 {
        return Err(Error::TypeMismatch(format!(
            "Huffman pide una columna Int32 de bytes, no {:?}",
            data.dtype()
        )));
    }
    if data.null_count() > 0 {
        return Err(Error::InvalidArgument(
            "los bytes no admiten nulos".to_string(),
        ));
    }
    data.values::<i32>()?
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            u8::try_from(v).map_err(|_| {
                Error::InvalidArgument(format!(
                    "el valor {v} en la fila {i} no es un byte (0..=255)"
                ))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use geoda_core::{BoolBuilder, Column, Error, PrimitiveBuilder, Utf8Builder};

    use super::{edit_distance_columns, huffman_decode_column, huffman_encode_column, lis_column};

    fn utf8(values: &[Option<&str>]) -> Column {
        let mut builder = Utf8Builder::new();
        for value in values {
            match value {
                Some(v) => builder.append(v).unwrap(),
                None => builder.append_null(),
            }
        }
        builder.finish().unwrap()
    }

    fn int32(values: &[i32]) -> Column {
        let mut builder = PrimitiveBuilder::<i32>::new();
        for &v in values {
            builder.append(v);
        }
        builder.finish().unwrap()
    }

    fn ints(column: &Column) -> Vec<Option<i64>> {
        let values = column.values::<i64>().unwrap();
        (0..column.len())
            .map(|i| (!column.is_null(i).unwrap()).then_some(values[i]))
            .collect()
    }

    #[test]
    fn distancia_fila_a_fila_con_nulos() {
        let a = utf8(&[Some("kitten"), None, Some("casa")]);
        let b = utf8(&[Some("sitting"), Some("x"), Some("casa")]);
        let result = edit_distance_columns(&a, &b).unwrap();
        assert_eq!(ints(&result), [Some(3), None, Some(0)]);
    }

    #[test]
    fn una_cadena_frente_a_una_columna() {
        let column = utf8(&[Some("casa"), Some("cosa"), None]);
        let one = utf8(&[Some("casa")]);
        assert_eq!(
            ints(&edit_distance_columns(&column, &one).unwrap()),
            [Some(0), Some(1), None]
        );
        assert_eq!(
            ints(&edit_distance_columns(&one, &column).unwrap()),
            [Some(0), Some(1), None]
        );
    }

    #[test]
    fn distancia_con_tipos_o_longitudes_malas() {
        let a = utf8(&[Some("a"), Some("b")]);
        let c = utf8(&[Some("a"), Some("b"), Some("c")]);
        assert!(matches!(
            edit_distance_columns(&a, &c),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            edit_distance_columns(&a, &int32(&[1, 2])),
            Err(Error::TypeMismatch(_))
        ));
    }

    #[test]
    fn lis_de_una_columna() {
        let column = int32(&[10, 9, 2, 5, 3, 7, 101, 18]);
        let result = lis_column(&column).unwrap();
        assert_eq!(result.len(), 4);
        let mut with_null = PrimitiveBuilder::<i64>::new();
        with_null.append(1);
        with_null.append_null();
        assert!(matches!(
            lis_column(&with_null.finish().unwrap()),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            lis_column(&utf8(&[Some("a")])),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn huffman_ida_y_vuelta_por_columnas() {
        let bytes: Vec<i32> = b"abracadabra".iter().map(|&b| i32::from(b)).collect();
        let data = int32(&bytes);
        let (bits, frequencies) = huffman_encode_column(&data).unwrap();
        assert_eq!(frequencies.len(), 256);
        assert!(bits.len() < bytes.len() * 8);
        let decoded = huffman_decode_column(&frequencies, &bits).unwrap();
        assert_eq!(decoded.values::<i32>().unwrap(), bytes);
    }

    #[test]
    fn huffman_rechaza_entradas_malas() {
        assert!(matches!(
            huffman_encode_column(&int32(&[1, 256])),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            huffman_encode_column(&int32(&[-1])),
            Err(Error::InvalidArgument(_))
        ));
        let (_, frequencies) = huffman_encode_column(&int32(&[1, 2, 2])).unwrap();
        let mut bits = BoolBuilder::new();
        bits.append(true);
        bits.append_null();
        assert!(matches!(
            huffman_decode_column(&frequencies, &bits.finish().unwrap()),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            huffman_decode_column(&int32(&[1]), &BoolBuilder::new().finish().unwrap()),
            Err(Error::TypeMismatch(_))
        ));
    }
}
