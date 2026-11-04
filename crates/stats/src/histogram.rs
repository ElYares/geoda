//! Histograma de ancho fijo de una columna numerica (HU-023, decision S9).
//!
//! # Reglas
//! - El usuario da el numero de cubos. El rango es `[min, max]` de los datos o uno
//!   explicito. Los cubos miden lo mismo y sus intervalos son `[a, b)`, salvo el ultimo,
//!   que es cerrado: el maximo del rango cae en el ultimo cubo.
//! - Un valor se asigna **por los bordes** (`edges`), no por la formula de division: asi
//!   el conteo coincide siempre con lo que dicen los bordes devueltos, aunque el redondeo
//!   de la division se equivoque en un cubo.
//! - Lo que no entra en ningun cubo se cuenta aparte, nunca se pierde: `below` y `above`
//!   (fuera del rango explicito), `nulls` (F1) y `nans` (S5). La suma de todo es la
//!   longitud de la columna.
//! - **Rango degenerado:** si `min == max` el rango se ensancha a `[v - 0.5, v + 0.5]`
//!   (como `NumPy`) y los valores iguales caen todos en un mismo cubo.
//! - Los enteros se comparan como `f64`: por encima de 2^53 pueden perder precision.
//! - El rango debe ser finito: un `inf` en los datos sin rango explicito da
//!   [`Error::InvalidArgument`].
//!
//! # Por que el rango se fija antes
//! Dos histogramas solo se suman si tienen los mismos cubos. Por eso [`Histogram`] recibe
//! el rango al construirse y [`histogram_column`] hace dos pasadas cuando no se da: una
//! para el minimo y el maximo, otra para contar. Los conteos son enteros, asi que la
//! fusion es exacta y no depende del orden.

use geoda_core::{Column, DataType, Error};

use crate::aggregate::{Aggregate, aggregate_with_workers, for_each_valid};
use crate::basic::kind_of;

/// Tope de cubos: evita reservar memoria sin medida con un numero que viene de fuera.
pub const MAX_BINS: usize = 1 << 24;

/// Resultado de un histograma.
#[derive(Debug, Clone, PartialEq)]
pub struct HistogramData {
    /// `n_bins + 1` bordes crecientes; el cubo `i` es `[edges[i], edges[i + 1])` y el
    /// ultimo es cerrado.
    pub edges: Vec<f64>,
    /// Valores en cada cubo.
    pub counts: Vec<usize>,
    /// Valores menores que el primer borde (solo con rango explicito).
    pub below: usize,
    /// Valores mayores que el ultimo borde (solo con rango explicito).
    pub above: usize,
    /// Valores nulos.
    pub nulls: usize,
    /// Valores `NaN`.
    pub nans: usize,
}

/// Estado parcial de un histograma: lo que cuentan los bloques ya vistos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistogramState {
    counts: Vec<usize>,
    below: usize,
    above: usize,
    nulls: usize,
    nans: usize,
}

/// Histograma con cubos y rango ya fijados.
#[derive(Debug, Clone)]
pub struct Histogram {
    edges: Vec<f64>,
}

impl Histogram {
    /// Histograma de `n_bins` cubos iguales sobre `[lo, hi]` para una columna de tipo
    /// `dtype`. Si `lo == hi` el rango se ensancha (ver el modulo).
    ///
    /// # Errors
    /// - [`Error::Unsupported`] para `Bool` y `Utf8`.
    /// - [`Error::InvalidArgument`] si `n_bins` es 0 o mayor que [`MAX_BINS`], si `lo` o
    ///   `hi` no son finitos, si `lo > hi` o si el ancho del rango no es representable.
    pub fn new(dtype: DataType, n_bins: usize, lo: f64, hi: f64) -> Result<Self, Error> {
        kind_of(dtype)?;
        if n_bins == 0 || n_bins > MAX_BINS {
            return Err(Error::InvalidArgument(format!(
                "n_bins debe estar entre 1 y {MAX_BINS}, y es {n_bins}"
            )));
        }
        if !lo.is_finite() || !hi.is_finite() {
            return Err(Error::InvalidArgument(
                "el rango del histograma debe ser finito".to_string(),
            ));
        }
        if lo > hi {
            return Err(Error::InvalidArgument(format!(
                "rango invertido: {lo} > {hi}"
            )));
        }
        #[allow(clippy::float_cmp)] // igualdad exacta: es el caso degenerado
        let (lo, hi) = if lo == hi {
            (lo - 0.5, hi + 0.5)
        } else {
            (lo, hi)
        };
        let width = hi - lo;
        if lo >= hi || !width.is_finite() {
            return Err(Error::InvalidArgument(format!(
                "el rango [{lo}, {hi}] no se puede dividir en cubos"
            )));
        }
        let mut edges = Vec::with_capacity(n_bins + 1);
        // `n_bins <= MAX_BINS = 2^24`: cabe exacto en los 53 bits de `f64`.
        #[allow(clippy::cast_precision_loss)]
        for i in 0..n_bins {
            edges.push(lo + width * (i as f64 / n_bins as f64));
        }
        // El ultimo borde es exactamente `hi`, sin error de redondeo.
        edges.push(hi);
        Ok(Self { edges })
    }

    fn n_bins(&self) -> usize {
        self.edges.len() - 1
    }

    /// Indice del cubo de `x`, o `None` si cae fuera del rango. Decide por los bordes.
    fn bin_of(&self, x: f64) -> Option<usize> {
        let n = self.n_bins();
        let lo = self.edges[0];
        let hi = self.edges[n];
        if x < lo || x > hi {
            return None;
        }
        // El ultimo cubo es cerrado: incluye `hi`.
        if x >= self.edges[n - 1] {
            return Some(n - 1);
        }
        // Estimacion por division; los bordes corrigen un posible error de redondeo.
        #[allow(clippy::cast_precision_loss)] // n <= 2^24
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // se acota con `min`
        let guess = ((x - lo) / (hi - lo) * n as f64) as usize;
        let mut index = guess.min(n - 1);
        while index > 0 && x < self.edges[index] {
            index -= 1;
        }
        while x >= self.edges[index + 1] {
            index += 1;
        }
        Some(index)
    }
}

impl Aggregate for Histogram {
    type State = HistogramState;
    type Output = HistogramData;

    fn init(&self) -> HistogramState {
        HistogramState {
            counts: vec![0; self.n_bins()],
            below: 0,
            above: 0,
            nulls: 0,
            nans: 0,
        }
    }

    fn update(&self, state: &mut HistogramState, block: &Column) -> Result<(), Error> {
        kind_of(block.dtype())?;
        state.nulls += block.null_count();
        for_each_f64(block, |x| {
            if x.is_nan() {
                state.nans += 1;
            } else if let Some(index) = self.bin_of(x) {
                state.counts[index] += 1;
            } else if x < self.edges[0] {
                state.below += 1;
            } else {
                state.above += 1;
            }
        })
    }

    fn merge(&self, mut left: HistogramState, right: HistogramState) -> HistogramState {
        for (a, b) in left.counts.iter_mut().zip(right.counts) {
            *a += b;
        }
        left.below += right.below;
        left.above += right.above;
        left.nulls += right.nulls;
        left.nans += right.nans;
        left
    }

    fn finalize(&self, state: HistogramState) -> Result<HistogramData, Error> {
        Ok(HistogramData {
            edges: self.edges.clone(),
            counts: state.counts,
            below: state.below,
            above: state.above,
            nulls: state.nulls,
            nans: state.nans,
        })
    }
}

/// Llama a `f` con cada valor valido del bloque como `f64`.
fn for_each_f64(block: &Column, mut f: impl FnMut(f64)) -> Result<(), Error> {
    match block.dtype() {
        DataType::Float64 => for_each_valid::<f64>(block, f),
        DataType::Int32 => for_each_valid::<i32>(block, |v| f(f64::from(v))),
        #[allow(clippy::cast_precision_loss)] // documentado: pierde precision sobre 2^53
        _ => for_each_valid::<i64>(block, |v| f(v as f64)),
    }
}

/// Minimo y maximo de los valores validos que no son `NaN`.
struct Extent;

impl Aggregate for Extent {
    type State = Option<(f64, f64)>;
    type Output = Option<(f64, f64)>;

    fn init(&self) -> Self::State {
        None
    }

    fn update(&self, state: &mut Self::State, block: &Column) -> Result<(), Error> {
        for_each_f64(block, |x| {
            if !x.is_nan() {
                *state = Some(state.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))));
            }
        })
    }

    fn merge(&self, left: Self::State, right: Self::State) -> Self::State {
        match (left, right) {
            (Some((a, b)), Some((c, d))) => Some((a.min(c), b.max(d))),
            (one, None) | (None, one) => one,
        }
    }

    fn finalize(&self, state: Self::State) -> Result<Self::Output, Error> {
        Ok(state)
    }
}

/// Histograma de una columna numerica con `n_bins` cubos iguales.
///
/// Con `range = None` el rango es `[min, max]` de los valores validos que no son `NaN`
/// (dos pasadas). Con `Some((lo, hi))` es ese rango y lo que lo excede se cuenta en
/// `below` y `above`.
///
/// # Errors
/// - [`Error::Unsupported`] para `Bool` y `Utf8`.
/// - [`Error::InvalidArgument`] por los motivos de [`Histogram::new`], y si no hay rango
///   explicito y la columna no tiene valores validos.
pub fn histogram_column(
    column: &Column,
    n_bins: usize,
    range: Option<(f64, f64)>,
) -> Result<HistogramData, Error> {
    histogram_column_with_workers(column, n_bins, range, 1)
}

/// Como [`histogram_column`], repartiendo los bloques entre `workers` hilos. El resultado
/// no depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`histogram_column`].
pub fn histogram_column_with_workers(
    column: &Column,
    n_bins: usize,
    range: Option<(f64, f64)>,
    workers: usize,
) -> Result<HistogramData, Error> {
    kind_of(column.dtype())?;
    let (lo, hi) = match range {
        Some(range) => range,
        None => aggregate_with_workers(&Extent, column, workers)?.ok_or_else(|| {
            Error::InvalidArgument("histograma sin rango y sin valores validos".to_string())
        })?,
    };
    aggregate_with_workers(
        &Histogram::new(column.dtype(), n_bins, lo, hi)?,
        column,
        workers,
    )
}
