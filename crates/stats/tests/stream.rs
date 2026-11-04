//! Pruebas de los agregados incrementales: deben coincidir bit a bit con `aggregate`.

use geoda_core::{Column, DataType, Error, PrimitiveBuilder};
use geoda_stats::{
    Accumulator, Aggregate, BLOCK, Count, Histogram, Max, Mean, Min, Sum, Variance, aggregate,
};

/// Agregado de juguete cuyo estado es la forma del arbol de fusion: cada bloque se
/// reconoce por su primer valor y `merge` anota los parentesis. Si dos arboles tienen la
/// misma forma, cualquier fusion no asociativa da los mismos bits.
struct Shape;

impl Aggregate for Shape {
    type State = String;
    type Output = String;

    fn init(&self) -> String {
        String::new()
    }

    fn update(&self, state: &mut String, block: &Column) -> Result<(), Error> {
        *state = block.values::<i64>()?[0].to_string();
        Ok(())
    }

    fn merge(&self, left: String, right: String) -> String {
        match (left.is_empty(), right.is_empty()) {
            (true, _) => right,
            (_, true) => left,
            _ => format!("({left} {right})"),
        }
    }

    fn finalize(&self, state: String) -> Result<String, Error> {
        Ok(state)
    }
}

/// `n` filas Int64; cada fila vale el indice de su bloque.
fn block_index_column(n: usize) -> Column {
    let mut b = PrimitiveBuilder::<i64>::new();
    for i in 0..n {
        b.append(i64::try_from(i / BLOCK).unwrap());
    }
    b.finish().unwrap()
}

/// Valores de magnitudes muy distintas: el orden de la suma cambia los ultimos bits.
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

/// Alimenta un acumulador con la columna partida en trozos de `size` filas.
fn run_in_chunks<A: Aggregate>(agg: A, column: &Column, size: usize) -> Result<A::Output, Error> {
    let mut acc = Accumulator::new(agg);
    let mut offset = 0;
    while offset < column.len() {
        let len = size.min(column.len() - offset);
        acc.push(&column.slice(offset, len)?)?;
        offset += len;
    }
    acc.finish()
}

#[test]
fn the_merge_tree_has_the_same_shape_as_the_whole_column() {
    for blocks in 0..=70 {
        for tail in [0, 5] {
            let column = block_index_column(blocks * BLOCK + tail);
            let expected = aggregate(&Shape, &column);
            // Los trozos diminutos copian el bloque en curso en cada `push`: solo con pocos bloques.
            let sizes: &[usize] = if blocks <= 8 {
                &[7, 1000, BLOCK, 3 * BLOCK + 1]
            } else {
                &[1000, BLOCK, 3 * BLOCK + 1]
            };
            for &size in sizes {
                // Con una columna vacia `Shape` no tiene primer valor: se salta.
                if column.is_empty() {
                    continue;
                }
                assert_eq!(
                    run_in_chunks(Shape, &column, size).unwrap(),
                    expected.as_ref().unwrap().clone(),
                    "bloques = {blocks}, cola = {tail}, trozo = {size}"
                );
            }
        }
    }
}

#[test]
fn every_aggregate_matches_the_whole_column_bit_by_bit() {
    let column = float_column(3 * BLOCK + 5 + 4000);
    let dtype = DataType::Float64;
    for size in [1, 7, BLOCK - 1, BLOCK, BLOCK + 1, 5000] {
        let ctx = format!("trozo = {size}");
        macro_rules! check {
            ($agg:expr) => {
                assert_eq!(
                    format!("{:?}", run_in_chunks($agg, &column, size)),
                    format!("{:?}", aggregate(&$agg, &column)),
                    "{}",
                    ctx
                );
            };
        }
        check!(Sum::new(dtype).unwrap());
        check!(Mean::new(dtype).unwrap());
        check!(Min::new(dtype).unwrap());
        check!(Max::new(dtype).unwrap());
        check!(Count::new(dtype).unwrap());
        check!(Variance::new(dtype, 1).unwrap());
        check!(Histogram::new(dtype, 8, -1.0, 1.0).unwrap());
    }
}

#[test]
fn no_chunks_gives_the_same_as_an_empty_column() {
    let sum = Sum::new(DataType::Float64).unwrap();
    let empty = float_column(0);
    assert_eq!(
        format!("{:?}", Accumulator::new(sum).finish()),
        format!("{:?}", aggregate(&sum, &empty))
    );
}

#[test]
fn a_chunk_of_another_type_is_rejected_and_the_accumulator_survives() {
    let sum = Sum::new(DataType::Float64).unwrap();
    let mut acc = Accumulator::new(sum);
    acc.push(&float_column(10)).unwrap();
    assert!(matches!(
        acc.push(&block_index_column(10)),
        Err(Error::TypeMismatch(_))
    ));
    acc.push(&float_column(10)).unwrap();
    let both = {
        let mut b = PrimitiveBuilder::<f64>::new();
        for _ in 0..2 {
            let c = float_column(10);
            for i in 0..10 {
                if c.is_null(i).unwrap() {
                    b.append_null();
                } else {
                    b.append(c.values::<f64>().unwrap()[i]);
                }
            }
        }
        b.finish().unwrap()
    };
    assert_eq!(
        format!("{:?}", acc.finish()),
        format!("{:?}", aggregate(&sum, &both))
    );
}

/// 10M de filas en trozos de 64K: el acumulador retiene unos pocos estados y menos de un
/// bloque, no la columna.
#[test]
fn memory_stays_logarithmic() {
    let chunk = float_column(64 * 1024);
    let mut acc = Accumulator::new(Sum::new(DataType::Float64).unwrap());
    let (mut max_states, mut max_pending) = (0, 0);
    let pushes: usize = 160;
    for _ in 0..pushes {
        acc.push(&chunk).unwrap();
        max_states = max_states.max(acc.live_states());
        max_pending = max_pending.max(acc.pending_rows());
    }
    let blocks: usize = pushes * 64;
    let bound = blocks.ilog2() as usize + 1;
    assert!(
        max_states <= bound,
        "{max_states} estados vivos, tope {bound}"
    );
    assert!(max_pending < BLOCK, "{max_pending} filas pendientes");
    acc.finish().unwrap();
}
