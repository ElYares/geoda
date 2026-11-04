//! Pruebas del archivo de columna: ida y vuelta por trozos y rechazo de archivos mal hechos.

use std::fs;
use std::path::PathBuf;

use geoda_core::file::{ColumnFileReader, ColumnFileWriter};
use geoda_core::{Column, DataType, Error, PrimitiveBuilder};

/// Ruta unica por prueba; se borra al soltarla.
struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        Self(std::env::temp_dir().join(format!("geoda-file-{}-{name}", std::process::id())))
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn float_column(n: usize) -> Column {
    let mut b = PrimitiveBuilder::<f64>::new();
    for i in 0..n {
        if i % 11 == 3 {
            b.append_null();
        } else {
            #[allow(clippy::cast_precision_loss)]
            b.append(i as f64 * 0.5 - 7.0);
        }
    }
    b.finish().unwrap()
}

fn int64_column(n: usize) -> Column {
    let mut b = PrimitiveBuilder::<i64>::new();
    for i in 0..n {
        b.append(i64::try_from(i).unwrap() * 3 - 7);
    }
    b.finish().unwrap()
}

fn int32_column(n: usize) -> Column {
    let mut b = PrimitiveBuilder::<i32>::new();
    for i in 0..n {
        b.append(i32::try_from(i).unwrap() * 3 - 7);
    }
    b.finish().unwrap()
}

fn write(path: &Tmp, column: &Column, with_validity: bool) {
    let mut w =
        ColumnFileWriter::create(&path.0, column.dtype(), column.len() as u64, with_validity)
            .unwrap();
    // En trozos desiguales para ejercitar el mapa de validez.
    let mut offset = 0;
    for size in [5, 16, 1, 40].into_iter().cycle() {
        if offset == column.len() {
            break;
        }
        let len = size.min(column.len() - offset);
        w.append(&column.slice(offset, len).unwrap()).unwrap();
        offset += len;
    }
    w.finish().unwrap();
}

fn read_all(path: &Tmp, chunk_rows: usize) -> Vec<Column> {
    ColumnFileReader::open(&path.0, chunk_rows)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Valores y nulos de varias columnas, uno tras otro, como texto comparable.
fn dump(chunks: &[Column]) -> Vec<String> {
    let mut out = Vec::new();
    for c in chunks {
        for i in 0..c.len() {
            out.push(if c.is_null(i).unwrap() {
                "null".to_string()
            } else {
                match c.dtype() {
                    DataType::Int32 => c.values::<i32>().unwrap()[i].to_string(),
                    DataType::Int64 => c.values::<i64>().unwrap()[i].to_string(),
                    _ => c.values::<f64>().unwrap()[i].to_bits().to_string(),
                }
            });
        }
    }
    out
}

#[test]
fn round_trip_keeps_values_and_nulls_for_every_type() {
    for (name, column) in [
        ("f64", float_column(100)),
        ("i32", int32_column(100)),
        ("i64", int64_column(100)),
    ] {
        // Con nulos hace falta el mapa de validez; sin nulos se prueba con y sin el.
        let options: &[bool] = if column.null_count() > 0 {
            &[true]
        } else {
            &[false, true]
        };
        for &validity in options {
            let tmp = Tmp::new(&format!("rt-{name}-{validity}"));
            write(&tmp, &column, validity);
            for chunk_rows in [8, 13, 24, 1000] {
                assert_eq!(
                    dump(&read_all(&tmp, chunk_rows)),
                    dump(std::slice::from_ref(&column)),
                    "{name}, validez = {validity}, trozo = {chunk_rows}"
                );
            }
        }
    }
}

#[test]
fn chunk_rows_is_rounded_down_to_a_multiple_of_8() {
    let tmp = Tmp::new("round");
    write(&tmp, &int64_column(50), false);
    let sizes: Vec<usize> = read_all(&tmp, 13).iter().map(Column::len).collect();
    assert_eq!(sizes, [8, 8, 8, 8, 8, 8, 2]);
    let sizes: Vec<usize> = read_all(&tmp, 0).iter().map(Column::len).collect();
    assert_eq!(sizes[0], 8);
}

#[test]
fn an_empty_column_round_trips_to_no_chunks() {
    let tmp = Tmp::new("empty");
    write(&tmp, &int64_column(0), true);
    let reader = ColumnFileReader::open(&tmp.0, 8).unwrap();
    assert_eq!((reader.rows(), reader.dtype()), (0, DataType::Int64));
    assert_eq!(reader.count(), 0);
}

fn good_file(name: &str) -> (Tmp, Vec<u8>) {
    let tmp = Tmp::new(name);
    write(&tmp, &float_column(40), true);
    let bytes = fs::read(&tmp.0).unwrap();
    (tmp, bytes)
}

fn open_error(tmp: &Tmp, bytes: &[u8]) -> Error {
    fs::write(&tmp.0, bytes).unwrap();
    match ColumnFileReader::open(&tmp.0, 8) {
        Err(e) => e,
        Ok(_) => panic!("el archivo malo se abrio"),
    }
}

fn is_ffi(e: &Error) -> bool {
    matches!(e, Error::Ffi(_))
}

#[test]
fn a_truncated_or_extended_file_is_rejected() {
    let (tmp, bytes) = good_file("trunc");
    for len in [0, 10, 31, bytes.len() - 1, bytes.len() / 2] {
        assert!(is_ffi(&open_error(&tmp, &bytes[..len])), "largo {len}");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(is_ffi(&open_error(&tmp, &longer)));
}

#[test]
fn a_bad_header_is_rejected() {
    let (tmp, good) = good_file("header");
    let patch = |at: usize, with: &[u8]| {
        let mut b = good.clone();
        b[at..at + with.len()].copy_from_slice(with);
        b
    };
    for (what, bytes) in [
        ("magia", patch(0, b"GEODAXXX")),
        ("version", patch(8, &2_u32.to_le_bytes())),
        ("tipo 0", patch(12, &0_u32.to_le_bytes())),
        ("tipo 9", patch(12, &9_u32.to_le_bytes())),
        ("banderas", patch(24, &2_u32.to_le_bytes())),
        ("reservado", patch(28, &1_u32.to_le_bytes())),
    ] {
        assert!(is_ffi(&open_error(&tmp, &bytes)), "{what}");
    }
}

/// Una cabecera que miente sobre las filas no puede pedir memoria que el archivo no tiene.
#[test]
fn rows_that_do_not_fit_the_file_are_rejected_without_allocating() {
    let (tmp, good) = good_file("rows");
    for rows in [
        u64::MAX,
        u64::MAX / 8 + 1,
        (isize::MAX as u64) / 8 + 1,
        1 << 40,
        41,
    ] {
        let mut b = good.clone();
        b[16..24].copy_from_slice(&rows.to_le_bytes());
        assert!(is_ffi(&open_error(&tmp, &b)), "filas = {rows}");
    }
}

#[test]
fn a_file_that_shrinks_after_opening_gives_an_error_and_stops() {
    let (tmp, bytes) = good_file("shrink");
    let mut reader = ColumnFileReader::open(&tmp.0, 8).unwrap();
    fs::write(&tmp.0, &bytes[..40]).unwrap();
    assert!(matches!(reader.next(), Some(Err(Error::Ffi(_)))));
    assert!(reader.next().is_none());
}

#[test]
fn a_missing_file_is_an_invalid_argument() {
    let tmp = Tmp::new("missing");
    assert!(matches!(
        ColumnFileReader::open(&tmp.0, 8),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn the_writer_rejects_misuse() {
    let tmp = Tmp::new("writer");
    assert!(matches!(
        ColumnFileWriter::create(&tmp.0, DataType::Bool, 1, false),
        Err(Error::Unsupported(_))
    ));
    let mut w = ColumnFileWriter::create(&tmp.0, DataType::Int64, 10, false).unwrap();
    assert!(matches!(
        w.append(&float_column(3)),
        Err(Error::TypeMismatch(_))
    ));
    assert!(matches!(
        w.append(&int64_column(11)),
        Err(Error::LengthMismatch(_))
    ));
    w.append(&int64_column(4)).unwrap();
    assert!(matches!(w.finish(), Err(Error::LengthMismatch(_))));
    let mut w = ColumnFileWriter::create(&tmp.0, DataType::Float64, 40, false).unwrap();
    assert!(matches!(
        w.append(&float_column(40)),
        Err(Error::InvalidArgument(_))
    ));
}
