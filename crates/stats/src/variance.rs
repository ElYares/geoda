//! Varianza y desviacion estandar estables, por bloques (HU-021).
//!
//! # El problema de la formula directa
//! La varianza se puede escribir como `E[x²] - E[x]²`, pero con valores grandes y
//! cercanos entre si (`1e9 + 4`, `1e9 + 7`...) los dos terminos valen casi lo mismo y
//! su resta cancela casi todas las cifras. Aqui se guarda otra cosa: la media y `m2`, la
//! suma de los cuadrados de las **desviaciones respecto a la media**, que nunca resta
//! numeros grandes parecidos.
//!
//! # Como se calcula y como se fusiona
//! - **Dentro de un bloque** (a lo sumo `BLOCK` valores, ya en memoria) se hacen dos
//!   pasadas: la media, y luego `m2` con las desviaciones respecto a esa media. Es mas
//!   exacto que actualizar valor por valor (Welford): con la media unas 10 000 veces
//!   mayor que la desviacion, Welford dejo un error relativo de 1e-12 en las pruebas, y
//!   las dos pasadas dejan el de la propia aritmetica.
//! - **Entre bloques** se junta con la formula de Chan:
//!   `media = ma + delta * nb / n`, `m2 = m2a + m2b + delta² * na * nb / n`, con
//!   `delta = mb - ma`. Es lo que hace fusionable al agregado (decision S3) y solo se
//!   aplica unas pocas veces (una por nivel del arbol), asi que no acumula error.
//!
//! La varianza es `m2 / (n - ddof)`. Con `ddof = 0` es la poblacional y con `ddof = 1` la
//! muestral (decision S2: el parametro es obligatorio en Rust).
//!
//! Las columnas enteras se convierten a `f64`: por encima de 2^53 pierden precision.

use geoda_core::{Column, DataType, Error};

use crate::aggregate::{Aggregate, aggregate_with_workers};
use crate::basic::{Kind, check_block, for_each_int, kind_of, pairwise_sum, valid_floats};

/// Cuenta, media y suma de cuadrados de desviaciones de un tramo de la columna.
#[derive(Debug, Clone, Copy, Default)]
pub struct Moments {
    n: usize,
    mean: f64,
    m2: f64,
}

impl Moments {
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
        let delta = right.mean - left.mean;
        Self {
            n,
            mean: left.mean + delta * nb / nt,
            m2: left.m2 + right.m2 + delta * delta * na * nb / nt,
        }
    }

    /// Varianza con `ddof` grados de libertad restados.
    #[allow(clippy::cast_precision_loss)]
    fn variance(&self, ddof: usize) -> Result<f64, Error> {
        if self.n == 0 {
            return Err(Error::InvalidArgument(
                "la varianza necesita al menos un valor valido".to_string(),
            ));
        }
        if ddof >= self.n {
            return Err(Error::InvalidArgument(format!(
                "ddof = {ddof} exige mas de {ddof} valores validos, hay {}",
                self.n
            )));
        }
        Ok(self.m2 / (self.n - ddof) as f64)
    }
}

/// Momentos del bloque en dos pasadas, para fusionarlos despues con el estado.
#[allow(clippy::cast_precision_loss)] // los enteros pasan a `f64`; ver la nota del modulo
fn block_moments(kind: Kind, block: &Column) -> Result<Moments, Error> {
    check_block(kind, block)?;
    let values = match kind {
        Kind::Int => {
            let mut values = Vec::with_capacity(block.len() - block.null_count());
            for_each_int(block, |v| values.push(v as f64))?;
            values
        }
        Kind::Float => valid_floats(block)?,
    };
    if values.is_empty() {
        return Ok(Moments::default());
    }
    let mean = pairwise_sum(&values) / values.len() as f64;
    let squares: Vec<f64> = values.iter().map(|v| (v - mean) * (v - mean)).collect();
    Ok(Moments {
        n: values.len(),
        mean,
        m2: pairwise_sum(&squares),
    })
}

/// Varianza de los valores no nulos con `ddof` grados de libertad restados.
#[derive(Debug, Clone, Copy)]
pub struct Variance {
    kind: Kind,
    ddof: usize,
}

impl Variance {
    /// Agregado de varianza para una columna de tipo `dtype`. `ddof = 0` es la varianza
    /// poblacional y `ddof = 1` la muestral.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(dtype: DataType, ddof: usize) -> Result<Self, Error> {
        Ok(Self {
            kind: kind_of(dtype)?,
            ddof,
        })
    }
}

impl Aggregate for Variance {
    type State = Moments;
    type Output = f64;

    fn init(&self) -> Moments {
        Moments::default()
    }

    fn update(&self, state: &mut Moments, block: &Column) -> Result<(), Error> {
        *state = Moments::combine(*state, block_moments(self.kind, block)?);
        Ok(())
    }

    fn merge(&self, left: Moments, right: Moments) -> Moments {
        Moments::combine(left, right)
    }

    fn finalize(&self, state: Moments) -> Result<f64, Error> {
        state.variance(self.ddof)
    }
}

/// Desviacion estandar: la raiz cuadrada de la varianza con el mismo `ddof`.
#[derive(Debug, Clone, Copy)]
pub struct StdDev {
    variance: Variance,
}

impl StdDev {
    /// Agregado de desviacion estandar para una columna de tipo `dtype`.
    ///
    /// # Errors
    /// [`Error::Unsupported`] para `Bool` y `Utf8`.
    pub fn new(dtype: DataType, ddof: usize) -> Result<Self, Error> {
        Ok(Self {
            variance: Variance::new(dtype, ddof)?,
        })
    }
}

impl Aggregate for StdDev {
    type State = Moments;
    type Output = f64;

    fn init(&self) -> Moments {
        self.variance.init()
    }

    fn update(&self, state: &mut Moments, block: &Column) -> Result<(), Error> {
        self.variance.update(state, block)
    }

    fn merge(&self, left: Moments, right: Moments) -> Moments {
        self.variance.merge(left, right)
    }

    fn finalize(&self, state: Moments) -> Result<f64, Error> {
        self.variance.finalize(state).map(f64::sqrt)
    }
}

/// Varianza de una columna numerica con `ddof` grados de libertad restados.
///
/// # Errors
/// - [`Error::Unsupported`] para `Bool` y `Utf8`.
/// - [`Error::InvalidArgument`] si no hay valores validos o `ddof >= n`.
pub fn variance_column(column: &Column, ddof: usize) -> Result<f64, Error> {
    variance_column_with_workers(column, ddof, 1)
}

/// Como [`variance_column`], repartiendo los bloques entre `workers` hilos. El resultado no
/// depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`variance_column`].
pub fn variance_column_with_workers(
    column: &Column,
    ddof: usize,
    workers: usize,
) -> Result<f64, Error> {
    aggregate_with_workers(&Variance::new(column.dtype(), ddof)?, column, workers)
}

/// Desviacion estandar de una columna numerica con `ddof` grados de libertad restados.
///
/// # Errors
/// Los mismos que [`variance_column`].
pub fn std_dev_column(column: &Column, ddof: usize) -> Result<f64, Error> {
    std_dev_column_with_workers(column, ddof, 1)
}

/// Como [`std_dev_column`], repartiendo los bloques entre `workers` hilos. El resultado no
/// depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`std_dev_column`].
pub fn std_dev_column_with_workers(
    column: &Column,
    ddof: usize,
    workers: usize,
) -> Result<f64, Error> {
    aggregate_with_workers(&StdDev::new(column.dtype(), ddof)?, column, workers)
}
