//! Estadisticos leidos de un archivo por trozos: iguales, bit a bit, a los de memoria.

use std::fs;

use geoda_core::file::{ColumnFileReader, ColumnFileWriter};
use geoda_core::{Column, DataType, Error, PrimitiveBuilder};
use geoda_stats::{Histogram, Mean, Sum, Variance, aggregate, aggregate_chunks};

fn float_column(n: usize) -> Column {
    let mut b = PrimitiveBuilder::<f64>::new();
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let x = i as f64;
        if i % 97 == 5 {
            b.append_null();
        } else {
            b.append((x * 0.1).sin() * 10f64.powi(i32::try_from(i % 13).unwrap() - 6));
        }
    }
    b.finish().unwrap()
}

#[test]
fn file_statistics_match_the_in_memory_ones_bit_by_bit() {
    let column = float_column(20_000);
    let path = std::env::temp_dir().join(format!("geoda-stats-{}", std::process::id()));
    let mut w = ColumnFileWriter::create(&path, DataType::Float64, 20_000, true).unwrap();
    w.append(&column).unwrap();
    w.finish().unwrap();

    let dtype = DataType::Float64;
    for chunk_rows in [8, 1000, 1024, 5000, 1_000_000] {
        macro_rules! check {
            ($agg:expr) => {
                assert_eq!(
                    format!(
                        "{:?}",
                        aggregate_chunks($agg, ColumnFileReader::open(&path, chunk_rows).unwrap())
                    ),
                    format!("{:?}", aggregate(&$agg, &column)),
                    "trozo = {chunk_rows}"
                );
            };
        }
        check!(Mean::new(dtype).unwrap());
        check!(Sum::new(dtype).unwrap());
        check!(Variance::new(dtype, 1).unwrap());
        check!(Histogram::new(dtype, 8, -1.0, 1.0).unwrap());
    }
    fs::remove_file(&path).unwrap();
}

#[test]
fn a_reading_error_stops_the_calculation() {
    let path = std::env::temp_dir().join(format!("geoda-stats-bad-{}", std::process::id()));
    fs::write(&path, b"no es una columna").unwrap();
    assert!(matches!(
        ColumnFileReader::open(&path, 8),
        Err(Error::Ffi(_))
    ));
    fs::remove_file(&path).unwrap();
    let chunks = [Err(Error::Ffi("trozo roto".to_string()))];
    assert!(matches!(
        aggregate_chunks(Sum::new(DataType::Float64).unwrap(), chunks),
        Err(Error::Ffi(_))
    ));
}
