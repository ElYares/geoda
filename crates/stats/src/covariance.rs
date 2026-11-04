//! Covarianza y correlacion de Pearson entre dos columnas, por bloques (HU-022).
//!
//! # Que se guarda
//! Igual que la varianza (ver [`crate::variance`]) pero para dos variables: la cuenta, las
//! dos medias, `m2x` y `m2y` (sumas de cuadrados de desviaciones) y `c2`, el co-momento:
//! la suma de `(x - media_x) * (y - media_y)`. Nada de eso resta numeros grandes parecidos.
//!
//! - **Dentro de un bloque** se hacen dos pasadas: las medias y luego los tres momentos.
//! - **Entre bloques** se junta con Chan: con `dx = mx_b - mx_a` y `dy = my_b - my_a`,
//!   `c2 = c2a + c2b + dx * dy * na * nb / n` (y lo analogo para `m2x`, `m2y`).
//!
//! La covarianza es `c2 / (n - ddof)` y la correlacion `c2 / sqrt(m2x * m2y)`, acotada a
//! `[-1, 1]` para absorber el error de redondeo (puede salir `1.0000000000000002`).
//!
//! # Nulos y `NaN` (decisiones S5 y S7)
//! Una fila donde `x` o `y` es nula se descarta en las dos columnas. Un `NaN` en cualquiera
//! da `NaN`. Si una de las columnas es constante la correlacion no esta definida y es
//! [`Error::InvalidArgument`], no `NaN`.

use geoda_core::{Column, DataType, Error};

use crate::aggregate::fold_blocks;
use crate::basic::{Kind, check_block, kind_of, pairwise_sum};

/// Estado de un tramo: cuenta, medias y momentos de las filas donde ambas son validas.
#[derive(Debug, Clone, Copy, Default)]
pub struct Comoments {
    n: usize,
    mean_x: f64,
    mean_y: f64,
    m2x: f64,
    m2y: f64,
    c2: f64,
}

impl Comoments {
    /// Junta dos tramos (Chan). Un tramo vacio es el neutro.
    #[allow(clippy::cast_precision_loss)]
    fn combine(left: Self, right: Self) -> Self {
        if left.n == 0 {
            return right;
        }
        if right.n == 0 {
            return left;
        }
        let n = left.n + right.n;
        let (na, nb, nt) = (left.n as f64, right.n as f64, n as f64);
        let dx = right.mean_x - left.mean_x;
        let dy = right.mean_y - left.mean_y;
        Self {
            n,
            mean_x: left.mean_x + dx * nb / nt,
            mean_y: left.mean_y + dy * nb / nt,
            m2x: left.m2x + right.m2x + dx * dx * na * nb / nt,
            m2y: left.m2y + right.m2y + dy * dy * na * nb / nt,
            c2: left.c2 + right.c2 + dx * dy * na * nb / nt,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn covariance(&self, ddof: usize) -> Result<f64, Error> {
        if self.n == 0 {
            return Err(Error::InvalidArgument(
                "la covarianza necesita al menos un par de valores validos".to_string(),
            ));
        }
        if ddof >= self.n {
            return Err(Error::InvalidArgument(format!(
                "ddof = {ddof} exige mas de {ddof} pares validos, hay {}",
                self.n
            )));
        }
        Ok(self.c2 / (self.n - ddof) as f64)
    }

    fn correlation(&self) -> Result<f64, Error> {
        if self.n < 2 {
            return Err(Error::InvalidArgument(
                "la correlacion necesita al menos dos pares validos".to_string(),
            ));
        }
        if self.m2x == 0.0 || self.m2y == 0.0 {
            return Err(Error::InvalidArgument(
                "la correlacion no esta definida si una columna es constante".to_string(),
            ));
        }
        // Con NaN en los momentos el resultado sigue siendo NaN: `clamp` lo deja pasar.
        Ok((self.c2 / (self.m2x * self.m2y).sqrt()).clamp(-1.0, 1.0))
    }
}

/// Valores del bloque como `f64`, uno por fila (en las filas nulas el valor no importa).
#[allow(clippy::cast_precision_loss)] // los enteros pasan a `f64`; ver la nota de `variance`
fn as_floats(kind: Kind, block: &Column) -> Result<Vec<f64>, Error> {
    check_block(kind, block)?;
    Ok(match (kind, block.dtype()) {
        (Kind::Float, _) => block.values::<f64>()?.to_vec(),
        (Kind::Int, DataType::Int32) => block
            .values::<i32>()?
            .iter()
            .map(|&v| f64::from(v))
            .collect(),
        (Kind::Int, _) => block.values::<i64>()?.iter().map(|&v| v as f64).collect(),
    })
}

/// Comomentos del bloque en dos pasadas, solo con las filas donde ambas son validas.
#[allow(clippy::cast_precision_loss)]
fn block_comoments(kinds: (Kind, Kind), x: &Column, y: &Column) -> Result<Comoments, Error> {
    let (all_x, all_y) = (as_floats(kinds.0, x)?, as_floats(kinds.1, y)?);
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for i in 0..x.len() {
        if !x.is_null(i)? && !y.is_null(i)? {
            xs.push(all_x[i]);
            ys.push(all_y[i]);
        }
    }
    if xs.is_empty() {
        return Ok(Comoments::default());
    }
    let n = xs.len() as f64;
    let (mean_x, mean_y) = (pairwise_sum(&xs) / n, pairwise_sum(&ys) / n);
    let dx: Vec<f64> = xs.iter().map(|v| v - mean_x).collect();
    let dy: Vec<f64> = ys.iter().map(|v| v - mean_y).collect();
    let squares = |d: &[f64]| pairwise_sum(&d.iter().map(|v| v * v).collect::<Vec<_>>());
    let products: Vec<f64> = dx.iter().zip(&dy).map(|(a, b)| a * b).collect();
    Ok(Comoments {
        n: xs.len(),
        mean_x,
        mean_y,
        m2x: squares(&dx),
        m2y: squares(&dy),
        c2: pairwise_sum(&products),
    })
}

/// Reparte las filas en bloques y fusiona los comomentos con el arbol fijo (S3).
fn comoments(
    kinds: (Kind, Kind),
    x: &Column,
    y: &Column,
    workers: usize,
) -> Result<Comoments, Error> {
    if x.len() != y.len() {
        return Err(Error::LengthMismatch(format!(
            "las columnas miden {} y {}",
            x.len(),
            y.len()
        )));
    }
    fold_blocks(
        x.len(),
        workers,
        Comoments::default,
        |offset, len| block_comoments(kinds, &x.slice(offset, len)?, &y.slice(offset, len)?),
        Comoments::combine,
    )
}

/// Covarianza de las filas donde ambas columnas son validas, con `ddof` grados de libertad
/// restados (`0` poblacional, `1` muestral; S2).
#[derive(Debug, Clone, Copy)]
pub struct Covariance {
    kinds: (Kind, Kind),
    ddof: usize,
}

impl Covariance {
    /// Covarianza entre una columna de tipo `x` y otra de tipo `y`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(x: DataType, y: DataType, ddof: usize) -> Result<Self, Error> {
        Ok(Self {
            kinds: (kind_of(x)?, kind_of(y)?),
            ddof,
        })
    }

    /// Calcula la covarianza.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si el largo difiere; [`Error::InvalidArgument`] si no hay
    /// pares validos o `ddof >= n`; [`Error::TypeMismatch`] si una columna no es del tipo
    /// con el que se creo el agregado.
    pub fn compute(&self, x: &Column, y: &Column) -> Result<f64, Error> {
        self.compute_with_workers(x, y, 1)
    }

    /// Como [`Covariance::compute`] simulando `workers` trabajadores; el resultado no
    /// depende de `workers` (S3).
    ///
    /// # Errors
    /// Los de [`Covariance::compute`]; [`Error::InvalidArgument`] si `workers` es 0.
    pub fn compute_with_workers(
        &self,
        x: &Column,
        y: &Column,
        workers: usize,
    ) -> Result<f64, Error> {
        comoments(self.kinds, x, y, workers)?.covariance(self.ddof)
    }
}

/// Correlacion de Pearson de las filas donde ambas columnas son validas.
#[derive(Debug, Clone, Copy)]
pub struct Correlation {
    kinds: (Kind, Kind),
}

impl Correlation {
    /// Correlacion entre una columna de tipo `x` y otra de tipo `y`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(x: DataType, y: DataType) -> Result<Self, Error> {
        Ok(Self {
            kinds: (kind_of(x)?, kind_of(y)?),
        })
    }

    /// Calcula la correlacion, siempre en `[-1, 1]` (o `NaN` si hay `NaN`).
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si el largo difiere; [`Error::InvalidArgument`] con menos
    /// de dos pares validos o si una columna es constante (S7).
    pub fn compute(&self, x: &Column, y: &Column) -> Result<f64, Error> {
        self.compute_with_workers(x, y, 1)
    }

    /// Como [`Correlation::compute`] simulando `workers` trabajadores (S3).
    ///
    /// # Errors
    /// Los de [`Correlation::compute`]; [`Error::InvalidArgument`] si `workers` es 0.
    pub fn compute_with_workers(
        &self,
        x: &Column,
        y: &Column,
        workers: usize,
    ) -> Result<f64, Error> {
        comoments(self.kinds, x, y, workers)?.correlation()
    }
}

/// Covarianza entre dos columnas numericas con `ddof` grados de libertad restados.
///
/// # Errors
/// Los de [`Covariance::new`] y [`Covariance::compute`].
pub fn covariance_columns(x: &Column, y: &Column, ddof: usize) -> Result<f64, Error> {
    Covariance::new(x.dtype(), y.dtype(), ddof)?.compute(x, y)
}

/// Correlacion de Pearson entre dos columnas numericas.
///
/// # Errors
/// Los de [`Correlation::new`] y [`Correlation::compute`].
pub fn correlation_columns(x: &Column, y: &Column) -> Result<f64, Error> {
    correlation_columns_with_workers(x, y, 1)
}

/// Como [`correlation_columns`], repartiendo los bloques entre `workers` hilos. El
/// resultado no depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`correlation_columns`].
pub fn correlation_columns_with_workers(
    x: &Column,
    y: &Column,
    workers: usize,
) -> Result<f64, Error> {
    Correlation::new(x.dtype(), y.dtype())?.compute_with_workers(x, y, workers)
}
