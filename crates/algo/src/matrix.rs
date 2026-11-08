//! Datos numericos para mineria: una matriz de filas por variables y un generador pseudoaleatorio.
//!
//! Hasta la fase 7 no hay un tipo matriz: una tabla de datos es una **lista de columnas** (una
//! por variable), decision 013 (M1). [`Dataset`] la copia a un arreglo contiguo por filas, que es
//! como la leen k-means, el clustering jerarquico y el arbol de decision.
//!
//! # Que se acepta
//! Columnas `Int32`, `Int64` o `Float64` del mismo largo, **sin nulos y sin `NaN`** (M2): no hay
//! una respuesta razonable para una fila incompleta, asi que se rechaza con
//! [`Error::InvalidArgument`] en vez de imputar a escondidas. `inf` tampoco: rompe las distancias.

use geoda_core::{Column, DataType, Error};

/// Matriz de `rows` filas por `cols` variables, guardada por filas.
#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl Dataset {
    /// Construye la matriz desde una lista de columnas (una por variable).
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si no hay columnas, hay un nulo, un valor no finito, o una
    /// columna es de otro tipo que `Int32`/`Int64`/`Float64`; [`Error::LengthMismatch`] si las
    /// columnas miden distinto; [`Error::Alloc`] si no hay memoria.
    pub fn from_columns(columns: &[&Column]) -> Result<Self, Error> {
        let first = columns
            .first()
            .ok_or_else(|| Error::InvalidArgument("se necesita al menos una columna".into()))?;
        let rows = first.len();
        let cols = columns.len();
        let total = rows
            .checked_mul(cols)
            .ok_or_else(|| Error::OutOfBounds("la matriz no cabe en usize".into()))?;
        let mut data = Vec::new();
        data.try_reserve_exact(total)
            .map_err(|_| Error::Alloc(format!("no hay memoria para {total} valores")))?;
        data.resize(total, 0.0);
        for (j, column) in columns.iter().enumerate() {
            if column.len() != rows {
                return Err(Error::LengthMismatch(format!(
                    "la columna {j} mide {} y la primera {rows}",
                    column.len()
                )));
            }
            if column.null_count() > 0 {
                return Err(Error::InvalidArgument(format!(
                    "la columna {j} tiene nulos"
                )));
            }
            let values: Vec<f64> = match column.dtype() {
                DataType::Float64 => column.values::<f64>()?.to_vec(),
                DataType::Int32 => column
                    .values::<i32>()?
                    .iter()
                    .map(|&v| f64::from(v))
                    .collect(),
                #[allow(clippy::cast_precision_loss)]
                // mas alla de 2^53 se pierde precision, documentado
                DataType::Int64 => column.values::<i64>()?.iter().map(|&v| v as f64).collect(),
                other => {
                    return Err(Error::InvalidArgument(format!(
                        "la columna {j} es {other:?}: se esperaba Int32, Int64 o Float64"
                    )));
                }
            };
            for (i, value) in values.into_iter().enumerate() {
                if !value.is_finite() {
                    return Err(Error::InvalidArgument(format!(
                        "la columna {j}, fila {i} no es finita ({value})"
                    )));
                }
                data[i * cols + j] = value;
            }
        }
        Ok(Self { rows, cols, data })
    }

    /// Cantidad de filas.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Cantidad de variables.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Fila `i` (`cols()` valores).
    ///
    /// # Panics
    /// Si `i >= rows()`.
    #[must_use]
    pub fn row(&self, i: usize) -> &[f64] {
        &self.data[i * self.cols..(i + 1) * self.cols]
    }

    /// Valor de la fila `i` y la variable `j`.
    ///
    /// # Panics
    /// Si `i >= rows()` o `j >= cols()`.
    #[must_use]
    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.cols + j]
    }
}

/// Distancia euclidiana al cuadrado entre dos filas del mismo largo.
#[must_use]
pub fn squared_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

/// Generador pseudoaleatorio splitmix64: corto, sin dependencias y reproducible con la misma
/// semilla (M3).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// Generador con `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Siguiente numero de 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        crate::hash::mix64(self.0)
    }

    /// Real uniforme en `[0, 1)` con 53 bits.
    pub fn next_f64(&mut self) -> f64 {
        // Los 53 bits altos caben exactos en un `f64`.
        #[allow(clippy::cast_precision_loss)]
        let bits = (self.next_u64() >> 11) as f64;
        bits / 9_007_199_254_740_992.0
    }

    /// Entero uniforme en `0..bound` (`bound > 0`), por rechazo para no sesgar.
    ///
    /// # Panics
    /// Si `bound` es 0.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "bound debe ser positivo");
        let bound = bound as u64;
        let zone = u64::MAX - u64::MAX % bound;
        loop {
            let value = self.next_u64();
            if value < zone {
                return usize::try_from(value % bound).expect("menor que bound");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Dataset, Rng};
    use geoda_core::{Column, Error, PrimitiveBuilder, Utf8Builder};

    fn floats(values: &[f64]) -> Column {
        let mut b = PrimitiveBuilder::<f64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    fn ints(values: &[i64]) -> Column {
        let mut b = PrimitiveBuilder::<i64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    #[test]
    fn arma_la_matriz_por_filas() {
        let (x, y) = (floats(&[1.0, 2.0, 3.0]), ints(&[10, 20, 30]));
        let data = Dataset::from_columns(&[&x, &y]).unwrap();
        assert_eq!((data.rows(), data.cols()), (3, 2));
        assert_eq!(data.row(1), [2.0, 20.0]);
        assert!((data.get(2, 1) - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rechaza_lo_que_no_es_valido() {
        let x = floats(&[1.0, 2.0]);
        assert!(matches!(
            Dataset::from_columns(&[]),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            Dataset::from_columns(&[&x, &floats(&[1.0])]),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            Dataset::from_columns(&[&floats(&[1.0, f64::NAN])]),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            Dataset::from_columns(&[&floats(&[1.0, f64::INFINITY])]),
            Err(Error::InvalidArgument(_))
        ));
        let mut with_null = PrimitiveBuilder::<f64>::new();
        with_null.append(1.0);
        with_null.append_null();
        assert!(matches!(
            Dataset::from_columns(&[&with_null.finish().unwrap()]),
            Err(Error::InvalidArgument(_))
        ));
        let mut text = Utf8Builder::new();
        text.append("a").unwrap();
        assert!(matches!(
            Dataset::from_columns(&[&text.finish().unwrap()]),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn el_generador_es_reproducible_y_uniforme() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut rng = Rng::new(1);
        let mut counts = [0usize; 5];
        for _ in 0..10_000 {
            let r = rng.next_f64();
            assert!((0.0..1.0).contains(&r));
            counts[rng.below(5)] += 1;
        }
        assert!(
            counts.iter().all(|&c| (1700..2300).contains(&c)),
            "{counts:?}"
        );
    }
}
