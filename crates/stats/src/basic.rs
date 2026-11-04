//! Conteo, suma, media, minimo y maximo de una columna numerica (HU-020).
//!
//! Cada estadistico es un [`Aggregate`](crate::Aggregate), asi que se calcula por bloques
//! y se fusiona con el arbol fijo de [`crate::aggregate`]. Las reglas numericas:
//!
//! - **Enteros (S1):** la suma se acumula en `i128`; solo el resultado final tiene que
//!   caber en `i64` (si no, [`Error::OutOfBounds`]). La media sale de la suma `i128` y la
//!   cuenta, asi que no pierde exactitud antes de dividir.
//! - **Flotantes (S3):** dentro de un bloque la suma es por pares, y los bloques se
//!   fusionan con el arbol fijo. El error crece como `log n` y no como `n`.
//! - **`NaN` (S5):** un `NaN` en la columna da `NaN` en `sum`, `mean`, `min` y `max`.
//! - **Sin valores validos (S6):** `count` y `sum` dan 0; `mean`, `min` y `max` dan
//!   [`Error::InvalidArgument`].
//! - **Nulos (F1):** se ignoran, tambien en `count`.

use geoda_core::{Column, DataType, Error};

use crate::aggregate::{Aggregate, aggregate, aggregate_with_workers, for_each_valid};

/// Resultado de un estadistico que conserva el tipo de la columna.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    /// Valor de una columna `Int32` o `Int64`.
    Int(i64),
    /// Valor de una columna `Float64`.
    Float(f64),
}

/// Familia numerica de una columna: `Int32` e `Int64` se tratan igual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Int,
    Float,
}

pub(crate) fn kind_of(dtype: DataType) -> Result<Kind, Error> {
    match dtype {
        DataType::Int32 | DataType::Int64 => Ok(Kind::Int),
        DataType::Float64 => Ok(Kind::Float),
        DataType::Bool | DataType::Utf8 => Err(Error::Unsupported(format!(
            "estadistico basico sobre una columna {dtype:?}"
        ))),
    }
}

/// Comprueba que el bloque es de la familia que el agregado espera.
pub(crate) fn check_block(expected: Kind, block: &Column) -> Result<(), Error> {
    if kind_of(block.dtype())? == expected {
        Ok(())
    } else {
        Err(Error::TypeMismatch(format!(
            "el agregado espera {expected:?} y el bloque es {:?}",
            block.dtype()
        )))
    }
}

/// Llama a `f` con cada valor valido del bloque, ya convertido a `i64`.
pub(crate) fn for_each_int(block: &Column, mut f: impl FnMut(i64)) -> Result<(), Error> {
    match block.dtype() {
        DataType::Int32 => for_each_valid::<i32>(block, |v| f(i64::from(v))),
        _ => for_each_valid::<i64>(block, f),
    }
}

/// Los valores validos del bloque (a lo sumo `BLOCK`), para sumarlos por pares.
pub(crate) fn valid_floats(block: &Column) -> Result<Vec<f64>, Error> {
    let mut values = Vec::with_capacity(block.len() - block.null_count());
    for_each_valid::<f64>(block, |v| values.push(v))?;
    Ok(values)
}

/// Suma por pares: parte el tramo a la mitad y suma las dos sumas. Los tramos cortos se
/// suman en orden, que es lo mas rapido y ya no acumula error apreciable.
pub(crate) fn pairwise_sum(values: &[f64]) -> f64 {
    const SEQUENTIAL: usize = 8;
    if values.len() <= SEQUENTIAL {
        return values.iter().sum();
    }
    let (left, right) = values.split_at(values.len() / 2);
    pairwise_sum(left) + pairwise_sum(right)
}

// ---------------------------------------------------------------- count

/// Cuenta los valores no nulos.
#[derive(Debug, Clone, Copy)]
pub struct Count;

impl Count {
    /// Agregado de conteo para una columna de tipo `dtype`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(dtype: DataType) -> Result<Self, Error> {
        kind_of(dtype).map(|_| Self)
    }
}

impl Aggregate for Count {
    type State = usize;
    type Output = usize;

    fn init(&self) -> usize {
        0
    }

    fn update(&self, state: &mut usize, block: &Column) -> Result<(), Error> {
        kind_of(block.dtype())?;
        *state += block.len() - block.null_count();
        Ok(())
    }

    fn merge(&self, left: usize, right: usize) -> usize {
        left + right
    }

    fn finalize(&self, state: usize) -> Result<usize, Error> {
        Ok(state)
    }
}

// ------------------------------------------------------------------ sum

/// Estado de una suma: solo uno de los dos campos se usa, segun la familia.
#[derive(Debug, Clone, Copy, Default)]
pub struct SumState {
    int: i128,
    float: f64,
}

/// Suma de los valores no nulos. En enteros da [`Number::Int`] y avisa si no cabe en
/// `i64`; en flotantes da [`Number::Float`].
#[derive(Debug, Clone, Copy)]
pub struct Sum {
    kind: Kind,
}

impl Sum {
    /// Agregado de suma para una columna de tipo `dtype`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(dtype: DataType) -> Result<Self, Error> {
        Ok(Self {
            kind: kind_of(dtype)?,
        })
    }
}

impl Aggregate for Sum {
    type State = SumState;
    type Output = Number;

    fn init(&self) -> SumState {
        SumState::default()
    }

    fn update(&self, state: &mut SumState, block: &Column) -> Result<(), Error> {
        check_block(self.kind, block)?;
        match self.kind {
            Kind::Int => for_each_int(block, |v| state.int += i128::from(v))?,
            Kind::Float => state.float += pairwise_sum(&valid_floats(block)?),
        }
        Ok(())
    }

    fn merge(&self, left: SumState, right: SumState) -> SumState {
        // Cada estado es a lo sumo `n * 2^63` con `n < 2^64` valores: cabe en `i128`.
        SumState {
            int: left.int + right.int,
            float: left.float + right.float,
        }
    }

    fn finalize(&self, state: SumState) -> Result<Number, Error> {
        match self.kind {
            Kind::Int => i64::try_from(state.int)
                .map(Number::Int)
                .map_err(|_| Error::OutOfBounds(format!("la suma {} no cabe en Int64", state.int))),
            Kind::Float => Ok(Number::Float(state.float)),
        }
    }
}

// ----------------------------------------------------------------- mean

/// Estado de una media: la suma y cuantos valores la forman.
#[derive(Debug, Clone, Copy, Default)]
pub struct MeanState {
    sum: SumState,
    count: usize,
}

/// Media de los valores no nulos, siempre en `f64`.
#[derive(Debug, Clone, Copy)]
pub struct Mean {
    sum: Sum,
}

impl Mean {
    /// Agregado de media para una columna de tipo `dtype`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(dtype: DataType) -> Result<Self, Error> {
        Ok(Self {
            sum: Sum::new(dtype)?,
        })
    }
}

impl Aggregate for Mean {
    type State = MeanState;
    type Output = f64;

    fn init(&self) -> MeanState {
        MeanState::default()
    }

    fn update(&self, state: &mut MeanState, block: &Column) -> Result<(), Error> {
        self.sum.update(&mut state.sum, block)?;
        state.count += block.len() - block.null_count();
        Ok(())
    }

    fn merge(&self, left: MeanState, right: MeanState) -> MeanState {
        MeanState {
            sum: self.sum.merge(left.sum, right.sum),
            count: left.count + right.count,
        }
    }

    #[allow(clippy::cast_precision_loss)] // la media es `f64` por decision S1
    fn finalize(&self, state: MeanState) -> Result<f64, Error> {
        if state.count == 0 {
            return Err(Error::InvalidArgument(
                "la media necesita al menos un valor valido".to_string(),
            ));
        }
        let total = match self.sum.kind {
            Kind::Int => state.sum.int as f64,
            Kind::Float => state.sum.float,
        };
        Ok(total / state.count as f64)
    }
}

// ------------------------------------------------------------- min y max

fn min_of(a: Number, b: Number) -> Number {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => Number::Int(x.min(y)),
        // `f64::min` ignora el NaN; S5 pide que se propague.
        (Number::Float(x), Number::Float(y)) if x.is_nan() || y.is_nan() => Number::Float(f64::NAN),
        (Number::Float(x), Number::Float(y)) => Number::Float(x.min(y)),
        _ => a,
    }
}

fn max_of(a: Number, b: Number) -> Number {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => Number::Int(x.max(y)),
        (Number::Float(x), Number::Float(y)) if x.is_nan() || y.is_nan() => Number::Float(f64::NAN),
        (Number::Float(x), Number::Float(y)) => Number::Float(x.max(y)),
        _ => a,
    }
}

/// Resume el bloque con `pick` (minimo o maximo) y lo junta al estado.
fn fold_extreme(
    kind: Kind,
    state: &mut Option<Number>,
    block: &Column,
    pick: fn(Number, Number) -> Number,
) -> Result<(), Error> {
    check_block(kind, block)?;
    let mut join = |value: Number| {
        *state = Some(state.map_or(value, |current| pick(current, value)));
    };
    match kind {
        Kind::Int => for_each_int(block, |v| join(Number::Int(v))),
        Kind::Float => for_each_valid::<f64>(block, |v| join(Number::Float(v))),
    }
}

fn merge_extreme(
    left: Option<Number>,
    right: Option<Number>,
    pick: fn(Number, Number) -> Number,
) -> Option<Number> {
    match (left, right) {
        (Some(a), Some(b)) => Some(pick(a, b)),
        (a, b) => a.or(b),
    }
}

fn finalize_extreme(state: Option<Number>, name: &str) -> Result<Number, Error> {
    state.ok_or_else(|| {
        Error::InvalidArgument(format!("el {name} necesita al menos un valor valido"))
    })
}

/// Minimo de los valores no nulos.
#[derive(Debug, Clone, Copy)]
pub struct Min {
    kind: Kind,
}

impl Min {
    /// Agregado de minimo para una columna de tipo `dtype`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(dtype: DataType) -> Result<Self, Error> {
        Ok(Self {
            kind: kind_of(dtype)?,
        })
    }
}

impl Aggregate for Min {
    type State = Option<Number>;
    type Output = Number;

    fn init(&self) -> Option<Number> {
        None
    }

    fn update(&self, state: &mut Option<Number>, block: &Column) -> Result<(), Error> {
        fold_extreme(self.kind, state, block, min_of)
    }

    fn merge(&self, left: Option<Number>, right: Option<Number>) -> Option<Number> {
        merge_extreme(left, right, min_of)
    }

    fn finalize(&self, state: Option<Number>) -> Result<Number, Error> {
        finalize_extreme(state, "minimo")
    }
}

/// Maximo de los valores no nulos.
#[derive(Debug, Clone, Copy)]
pub struct Max {
    kind: Kind,
}

impl Max {
    /// Agregado de maximo para una columna de tipo `dtype`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(dtype: DataType) -> Result<Self, Error> {
        Ok(Self {
            kind: kind_of(dtype)?,
        })
    }
}

impl Aggregate for Max {
    type State = Option<Number>;
    type Output = Number;

    fn init(&self) -> Option<Number> {
        None
    }

    fn update(&self, state: &mut Option<Number>, block: &Column) -> Result<(), Error> {
        fold_extreme(self.kind, state, block, max_of)
    }

    fn merge(&self, left: Option<Number>, right: Option<Number>) -> Option<Number> {
        merge_extreme(left, right, max_of)
    }

    fn finalize(&self, state: Option<Number>) -> Result<Number, Error> {
        finalize_extreme(state, "maximo")
    }
}

// ------------------------------------------------- atajos sobre una columna

/// Cantidad de valores no nulos de una columna numerica.
///
/// # Errors
/// [`Error::Unsupported`] para `Bool` y `Utf8`.
pub fn count_column(column: &Column) -> Result<usize, Error> {
    aggregate(&Count::new(column.dtype())?, column)
}

/// Suma de una columna numerica. Vacia o sin valores validos da 0.
///
/// # Errors
/// [`Error::Unsupported`] para `Bool` y `Utf8`; [`Error::OutOfBounds`] si la suma de una
/// columna entera no cabe en `i64`.
pub fn sum_column(column: &Column) -> Result<Number, Error> {
    sum_column_with_workers(column, 1)
}

/// Como [`sum_column`], repartiendo los bloques entre `workers` hilos. El resultado no
/// depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`sum_column`].
pub fn sum_column_with_workers(column: &Column, workers: usize) -> Result<Number, Error> {
    aggregate_with_workers(&Sum::new(column.dtype())?, column, workers)
}

/// Media de una columna numerica, en `f64`.
///
/// # Errors
/// [`Error::Unsupported`] para `Bool` y `Utf8`; [`Error::InvalidArgument`] sin valores
/// validos.
pub fn mean_column(column: &Column) -> Result<f64, Error> {
    mean_column_with_workers(column, 1)
}

/// Como [`mean_column`], repartiendo los bloques entre `workers` hilos. El resultado no
/// depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`mean_column`].
pub fn mean_column_with_workers(column: &Column, workers: usize) -> Result<f64, Error> {
    aggregate_with_workers(&Mean::new(column.dtype())?, column, workers)
}

/// Minimo de una columna numerica.
///
/// # Errors
/// [`Error::Unsupported`] para `Bool` y `Utf8`; [`Error::InvalidArgument`] sin valores
/// validos.
pub fn min_column(column: &Column) -> Result<Number, Error> {
    min_column_with_workers(column, 1)
}

/// Como [`min_column`], repartiendo los bloques entre `workers` hilos. El resultado no
/// depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`min_column`].
pub fn min_column_with_workers(column: &Column, workers: usize) -> Result<Number, Error> {
    aggregate_with_workers(&Min::new(column.dtype())?, column, workers)
}

/// Maximo de una columna numerica.
///
/// # Errors
/// [`Error::Unsupported`] para `Bool` y `Utf8`; [`Error::InvalidArgument`] sin valores
/// validos.
pub fn max_column(column: &Column) -> Result<Number, Error> {
    max_column_with_workers(column, 1)
}

/// Como [`max_column`], repartiendo los bloques entre `workers` hilos. El resultado no
/// depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`max_column`].
pub fn max_column_with_workers(column: &Column, workers: usize) -> Result<Number, Error> {
    aggregate_with_workers(&Max::new(column.dtype())?, column, workers)
}
