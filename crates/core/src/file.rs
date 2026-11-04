//! Archivo de una columna, leido por trozos (decision 008).
//!
//! Formato propio y minimo, todo en little-endian:
//!
//! ```text
//! 0..8    magia "GEODACOL"
//! 8..12   version (u32, hoy 1)
//! 12..16  tipo (u32: 1 = Int32, 2 = Int64, 3 = Float64)
//! 16..24  filas (u64)
//! 24..28  banderas (u32; bit 0 = hay mapa de validez)
//! 28..32  reservado (debe ser 0)
//! 32..    valores: filas * ancho bytes
//! luego   validez (solo con la bandera): ceil(filas / 8) bytes, bit 0 = fila 0,
//!         1 = valida (igual que Arrow)
//! ```
//!
//! La longitud del archivo debe ser exactamente la que dicen la cabecera y el tipo. Eso
//! acota todo lo que se reserva por lo que de verdad hay en disco: una cabecera que
//! miente no puede pedir memoria que el archivo no tiene.
//!
//! # Errores
//! Un archivo con contenido invalido da [`Error::Ffi`]; un fallo del sistema de archivos
//! (no existe, sin permiso, disco lleno) da [`Error::InvalidArgument`] con el detalle.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::{Column, DataType, Error, PrimitiveBuilder};

const MAGIC: &[u8; 8] = b"GEODACOL";
const VERSION: u32 = 1;
const HEADER_LEN: u64 = 32;
const FLAG_VALIDITY: u32 = 1;

fn io_error(what: &str, e: &std::io::Error) -> Error {
    Error::InvalidArgument(format!("{what}: {e}"))
}

fn malformed(detail: impl Into<String>) -> Error {
    Error::Ffi(format!("archivo de columna invalido: {}", detail.into()))
}

fn code_of(dtype: DataType) -> Result<u32, Error> {
    match dtype {
        DataType::Int32 => Ok(1),
        DataType::Int64 => Ok(2),
        DataType::Float64 => Ok(3),
        other => Err(Error::Unsupported(format!(
            "archivo de columna de tipo {other:?}"
        ))),
    }
}

fn dtype_of(code: u32) -> Result<DataType, Error> {
    match code {
        1 => Ok(DataType::Int32),
        2 => Ok(DataType::Int64),
        3 => Ok(DataType::Float64),
        other => Err(malformed(format!("tipo desconocido ({other})"))),
    }
}

fn width_of(dtype: DataType) -> Result<u64, Error> {
    match dtype {
        DataType::Int32 => Ok(4),
        _ => code_of(dtype).map(|_| 8),
    }
}

/// Escribe una columna en un archivo, trozo a trozo, sin tenerla entera en memoria.
///
/// Las filas totales se declaran al crearlo (van en la cabecera). El mapa de validez, si
/// lo hay, se guarda en memoria (un bit por fila) y se escribe en [`Self::finish`].
pub struct ColumnFileWriter {
    out: BufWriter<File>,
    dtype: DataType,
    rows: u64,
    written: u64,
    /// `None` si el archivo no admite nulos.
    validity: Option<Vec<u8>>,
}

impl ColumnFileWriter {
    /// Crea (o trunca) `path` para `rows` filas de tipo `dtype`.
    ///
    /// # Errors
    /// - [`Error::Unsupported`] si `dtype` no es `Int32`, `Int64` ni `Float64`.
    /// - [`Error::InvalidArgument`] si no se puede crear el archivo.
    pub fn create(
        path: &Path,
        dtype: DataType,
        rows: u64,
        with_validity: bool,
    ) -> Result<Self, Error> {
        let code = code_of(dtype)?;
        let mut out = BufWriter::new(
            File::create(path).map_err(|e| io_error("no se pudo crear el archivo", &e))?,
        );
        let flags = if with_validity { FLAG_VALIDITY } else { 0 };
        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.extend_from_slice(&code.to_le_bytes());
        header.extend_from_slice(&rows.to_le_bytes());
        header.extend_from_slice(&flags.to_le_bytes());
        header.extend_from_slice(&0_u32.to_le_bytes());
        out.write_all(&header)
            .map_err(|e| io_error("no se pudo escribir la cabecera", &e))?;
        Ok(Self {
            out,
            dtype,
            rows,
            written: 0,
            validity: with_validity.then(Vec::new),
        })
    }

    /// Añade el siguiente trozo.
    ///
    /// # Errors
    /// - [`Error::TypeMismatch`] si el tipo no es el del archivo.
    /// - [`Error::LengthMismatch`] si se pasaria de las filas declaradas.
    /// - [`Error::InvalidArgument`] si el trozo tiene nulos y el archivo no los admite, o
    ///   si falla la escritura.
    pub fn append(&mut self, chunk: &Column) -> Result<(), Error> {
        if chunk.dtype() != self.dtype {
            return Err(Error::TypeMismatch(format!(
                "el trozo es {:?} y el archivo {:?}",
                chunk.dtype(),
                self.dtype
            )));
        }
        let len = chunk.len() as u64;
        if self.written.checked_add(len).is_none_or(|t| t > self.rows) {
            return Err(Error::LengthMismatch(format!(
                "el trozo pasa de las {} filas declaradas",
                self.rows
            )));
        }
        if chunk.null_count() > 0 && self.validity.is_none() {
            return Err(Error::InvalidArgument(
                "el trozo tiene nulos y el archivo se creo sin mapa de validez".to_string(),
            ));
        }
        let mut bytes = Vec::new();
        match self.dtype {
            DataType::Int32 => {
                for v in chunk.values::<i32>()? {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
            DataType::Int64 => {
                for v in chunk.values::<i64>()? {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
            _ => {
                for v in chunk.values::<f64>()? {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        self.out
            .write_all(&bytes)
            .map_err(|e| io_error("no se pudo escribir", &e))?;
        if let Some(bits) = &mut self.validity {
            for i in 0..chunk.len() {
                let row = self.written + i as u64;
                if row.is_multiple_of(8) {
                    bits.push(0);
                }
                if !chunk.is_null(i)?
                    && let Some(last) = bits.last_mut()
                {
                    *last |= 1 << (row % 8);
                }
            }
        }
        self.written += len;
        Ok(())
    }

    /// Termina el archivo.
    ///
    /// # Errors
    /// - [`Error::LengthMismatch`] si faltan filas por escribir.
    /// - [`Error::InvalidArgument`] si falla la escritura.
    pub fn finish(mut self) -> Result<(), Error> {
        if self.written != self.rows {
            return Err(Error::LengthMismatch(format!(
                "se declararon {} filas y se escribieron {}",
                self.rows, self.written
            )));
        }
        if let Some(bits) = &self.validity {
            self.out
                .write_all(bits)
                .map_err(|e| io_error("no se pudo escribir la validez", &e))?;
        }
        self.out
            .flush()
            .map_err(|e| io_error("no se pudo cerrar el archivo", &e))
    }
}

/// Lee un archivo de columna por trozos. Es un iterador de columnas: cada elemento es el
/// siguiente trozo, o el error que impidio leerlo (despues de un error no devuelve mas).
pub struct ColumnFileReader {
    file: File,
    dtype: DataType,
    rows: u64,
    position: u64,
    chunk_rows: u64,
    values_start: u64,
    validity_start: Option<u64>,
}

impl ColumnFileReader {
    /// Abre `path` y valida la cabecera y la longitud. `chunk_rows` es el tamano de los
    /// trozos; se redondea hacia abajo a un multiplo de 8 (minimo 8) para que cada trozo
    /// empiece en un limite de byte del mapa de validez.
    ///
    /// # Errors
    /// - [`Error::Ffi`] si el archivo esta truncado o la cabecera no es valida.
    /// - [`Error::InvalidArgument`] si no se puede abrir.
    pub fn open(path: &Path, chunk_rows: usize) -> Result<Self, Error> {
        let mut file = File::open(path).map_err(|e| io_error("no se pudo abrir el archivo", &e))?;
        let file_len = file
            .metadata()
            .map_err(|e| io_error("no se pudo leer el tamano", &e))?
            .len();
        let mut header = [0_u8; 32];
        file.read_exact(&mut header)
            .map_err(|_| malformed("mas corto que la cabecera"))?;
        if &header[0..8] != MAGIC {
            return Err(malformed("la magia no es GEODACOL"));
        }
        let word = |from: usize| {
            u32::from_le_bytes([
                header[from],
                header[from + 1],
                header[from + 2],
                header[from + 3],
            ])
        };
        if word(8) != VERSION {
            return Err(malformed(format!("version {} no soportada", word(8))));
        }
        let dtype = dtype_of(word(12))?;
        let mut rows_bytes = [0_u8; 8];
        rows_bytes.copy_from_slice(&header[16..24]);
        let rows = u64::from_le_bytes(rows_bytes);
        let flags = word(24);
        if flags & !FLAG_VALIDITY != 0 || word(28) != 0 {
            return Err(malformed("banderas o reservado con bits desconocidos"));
        }
        let values_len = rows
            .checked_mul(width_of(dtype)?)
            .filter(|&n| isize::try_from(n).is_ok())
            .ok_or_else(|| malformed("las filas de la cabecera no caben"))?;
        let validity_len = if flags & FLAG_VALIDITY != 0 {
            rows.div_ceil(8)
        } else {
            0
        };
        let expected = HEADER_LEN
            .checked_add(values_len)
            .and_then(|n| n.checked_add(validity_len))
            .ok_or_else(|| malformed("el tamano de la cabecera no cabe"))?;
        if expected != file_len {
            return Err(malformed(format!(
                "la cabecera pide {expected} bytes y el archivo tiene {file_len}"
            )));
        }
        Ok(Self {
            file,
            dtype,
            rows,
            position: 0,
            chunk_rows: (chunk_rows.max(8) & !7) as u64,
            values_start: HEADER_LEN,
            validity_start: (flags & FLAG_VALIDITY != 0).then_some(HEADER_LEN + values_len),
        })
    }

    /// Tipo de la columna del archivo.
    #[must_use]
    pub fn dtype(&self) -> DataType {
        self.dtype
    }

    /// Filas totales del archivo.
    #[must_use]
    pub fn rows(&self) -> u64 {
        self.rows
    }

    fn read_chunk(&mut self) -> Result<Column, Error> {
        let len = self.chunk_rows.min(self.rows - self.position);
        let width = width_of(self.dtype)?;
        // `len` <= filas del archivo, que ya cupo en su longitud real: no sobre-reserva.
        let n = usize::try_from(len).map_err(|_| malformed("el trozo no cabe en usize"))?;
        let mut values = vec![0_u8; n * usize::try_from(width).map_err(|_| malformed("ancho"))?];
        self.file
            .seek(SeekFrom::Start(self.values_start + self.position * width))
            .and_then(|_| self.file.read_exact(&mut values))
            .map_err(|_| malformed("el archivo se acorto mientras se leia"))?;
        let mut validity = Vec::new();
        if let Some(start) = self.validity_start {
            validity = vec![0_u8; n.div_ceil(8)];
            self.file
                .seek(SeekFrom::Start(start + self.position / 8))
                .and_then(|_| self.file.read_exact(&mut validity))
                .map_err(|_| malformed("el archivo se acorto mientras se leia"))?;
        }
        let is_valid =
            |i: usize| self.validity_start.is_none() || validity[i / 8] >> (i % 8) & 1 == 1;
        self.position += len;
        match self.dtype {
            DataType::Int32 => build::<i32, 4>(&values, n, is_valid, i32::from_le_bytes),
            DataType::Int64 => build::<i64, 8>(&values, n, is_valid, i64::from_le_bytes),
            _ => build::<f64, 8>(&values, n, is_valid, f64::from_le_bytes),
        }
    }
}

fn build<T: crate::memory::Native, const W: usize>(
    bytes: &[u8],
    n: usize,
    is_valid: impl Fn(usize) -> bool,
    decode: fn([u8; W]) -> T,
) -> Result<Column, Error> {
    let mut builder = PrimitiveBuilder::<T>::new();
    for (i, &raw) in bytes.as_chunks::<W>().0.iter().take(n).enumerate() {
        if is_valid(i) {
            builder.append(decode(raw));
        } else {
            builder.append_null();
        }
    }
    builder.finish()
}

impl Iterator for ColumnFileReader {
    type Item = Result<Column, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.rows {
            return None;
        }
        let chunk = self.read_chunk();
        if chunk.is_err() {
            self.position = self.rows;
        }
        Some(chunk)
    }
}
