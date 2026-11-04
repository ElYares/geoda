//! Agregados por grupo con una columna clave (HU-024).
//!
//! # La idea
//! [`group_column`](geoda_algo::group_column) asigna cada fila a un grupo (tabla hash,
//! O(n) esperado). Despues las filas de cada grupo se juntan en una columna propia y se
//! les aplica el [`Aggregate`] de siempre con [`aggregate`]. Asi el resultado de un grupo
//! es **identico bit a bit** al de aplicar el agregado a esas filas, con las mismas reglas
//! de nulos (F1), `NaN` (S5) y errores; no hay una segunda implementacion que mantener.
//! El costo es una copia de la columna de valores repartida en los grupos.
//!
//! # Reglas
//! - Los grupos salen en el orden de la **primera aparicion** de su clave.
//! - Una clave nula es un grupo mas, con clave `None` (S8); los valores nulos de un grupo
//!   se ignoran en el agregado.
//! - Todos los `NaN` clave son un grupo; `-0.0` y `0.0` son grupos distintos.
//! - Un grupo cuyos valores son todos nulos da lo que da el agregado sin valores validos:
//!   `0` en `count` y `sum`, [`Error::InvalidArgument`] en `mean`, `min`, `max` y `var`.
//!   El error aborta la llamada entera.

use geoda_algo::{Value, group_column};
use geoda_core::memory::Native;
use geoda_core::{BoolBuilder, Column, DataType, Error, PrimitiveBuilder, Utf8Builder};

use crate::aggregate::{Aggregate, aggregate_with_workers};
use crate::basic::{Count, Max, Mean, Min, Number, Sum, kind_of};
use crate::variance::{StdDev, Variance};

/// Resultado de un agregado por grupo: `keys[g]` y `results[g]` van emparejados.
#[derive(Debug, Clone, PartialEq)]
pub struct Grouped<T> {
    /// Clave de cada grupo, en orden de primera aparicion; `None` es el grupo de nulos.
    pub keys: Vec<Option<Value>>,
    /// Resultado del agregado en cada grupo.
    pub results: Vec<T>,
}

/// Aplica `agg` a la columna `values` en cada grupo de `keys`.
///
/// # Errors
/// - [`Error::LengthMismatch`] si `keys` y `values` miden distinto.
/// - [`Error::Unsupported`] si `values` es `Bool` o `Utf8`.
/// - Los de [`aggregate`] en cualquier grupo (p. ej. [`Error::InvalidArgument`] en una
///   media sin valores validos).
pub fn group_aggregate<A: Aggregate>(
    agg: &A,
    keys: &Column,
    values: &Column,
) -> Result<Grouped<A::Output>, Error> {
    group_aggregate_with_workers(agg, keys, values, 1)
}

/// Como [`group_aggregate`], repartiendo entre `workers` hilos los bloques de cada grupo.
/// El resultado no depende de `workers`. Un grupo de un solo bloque no lanza hilos, asi que
/// con muchos grupos pequenos el efecto es nulo.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`group_aggregate`].
pub fn group_aggregate_with_workers<A: Aggregate>(
    agg: &A,
    keys: &Column,
    values: &Column,
    workers: usize,
) -> Result<Grouped<A::Output>, Error> {
    if workers == 0 {
        return Err(Error::InvalidArgument(
            "se necesita al menos un trabajador".to_string(),
        ));
    }
    if keys.len() != values.len() {
        return Err(Error::LengthMismatch(format!(
            "{} claves y {} valores",
            keys.len(),
            values.len()
        )));
    }
    kind_of(values.dtype())?;
    let grouping = group_column(keys)?;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); grouping.keys.len()];
    for (row, &group) in grouping.ids.iter().enumerate() {
        rows[group].push(row);
    }
    let mut results = Vec::with_capacity(rows.len());
    for group_rows in &rows {
        results.push(aggregate_with_workers(
            agg,
            &take(values, group_rows)?,
            workers,
        )?);
    }
    Ok(Grouped {
        keys: grouping.keys,
        results,
    })
}

/// Columna con las filas `rows` de `column`, en ese orden y conservando los nulos.
fn take(column: &Column, rows: &[usize]) -> Result<Column, Error> {
    match column.dtype() {
        DataType::Int32 => take_primitive::<i32>(column, rows),
        DataType::Int64 => take_primitive::<i64>(column, rows),
        DataType::Float64 => take_primitive::<f64>(column, rows),
        other => Err(Error::Unsupported(format!(
            "agregar por grupo una columna {other:?}"
        ))),
    }
}

fn take_primitive<T: Native>(column: &Column, rows: &[usize]) -> Result<Column, Error> {
    let values = column.values::<T>()?;
    let mut builder = PrimitiveBuilder::<T>::new();
    for &row in rows {
        if column.is_null(row)? {
            builder.append_null();
        } else {
            builder.append(values[row]);
        }
    }
    builder.finish()
}

// ------------------------------------------------- agregado elegido en tiempo de ejecucion

/// Agregado por grupo elegido por nombre: es lo que usan los bindings, que no pueden
/// instanciar un tipo generico de Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupOp {
    /// Valores no nulos.
    Count,
    /// Suma.
    Sum,
    /// Minimo.
    Min,
    /// Maximo.
    Max,
    /// Media.
    Mean,
    /// Varianza (necesita `ddof`).
    Var,
    /// Desviacion estandar (necesita `ddof`).
    Std,
}

/// Resultados de un agregado por grupo, con el tipo que corresponde a la operacion.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupValues {
    /// `Count`, y `Sum`/`Min`/`Max` sobre una columna entera.
    Int(Vec<i64>),
    /// `Mean`, `Var`, `Std`, y `Sum`/`Min`/`Max` sobre una columna `Float64`.
    Float(Vec<f64>),
}

/// Resultado de [`group_by`]: una clave y un valor por grupo.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupResult {
    /// Tipo de la columna de claves.
    pub key_dtype: DataType,
    /// Clave de cada grupo, en orden de primera aparicion; `None` es el grupo de nulos.
    pub keys: Vec<Option<Value>>,
    /// Resultado en cada grupo.
    pub values: GroupValues,
}

impl GroupResult {
    /// Columna de claves, del mismo tipo que la de entrada; el grupo de nulos sale como
    /// nulo. Un `NaN` sale como el `NaN` canonico y `-0.0` conserva su signo.
    ///
    /// # Errors
    /// [`Error::TypeMismatch`] si una clave no es del tipo de la columna (no ocurre con un
    /// resultado de [`group_by`]).
    pub fn keys_column(&self) -> Result<Column, Error> {
        let mismatch = |key: &Value| {
            Error::TypeMismatch(format!(
                "la clave {key:?} no es de tipo {:?}",
                self.key_dtype
            ))
        };
        match self.key_dtype {
            DataType::Bool => {
                let mut builder = BoolBuilder::new();
                for key in &self.keys {
                    match key {
                        None => builder.append_null(),
                        Some(Value::Bool(v)) => builder.append(*v),
                        Some(other) => return Err(mismatch(other)),
                    }
                }
                builder.finish()
            }
            DataType::Int32 => self.primitive_keys(|key| match key {
                Value::Int32(v) => Some(*v),
                _ => None,
            }),
            DataType::Int64 => self.primitive_keys(|key| match key {
                Value::Int64(v) => Some(*v),
                _ => None,
            }),
            DataType::Float64 => self.primitive_keys(|key| match key {
                Value::Float64(v) => Some(*v),
                _ => None,
            }),
            DataType::Utf8 => {
                let mut builder = Utf8Builder::new();
                for key in &self.keys {
                    match key {
                        None => builder.append_null(),
                        Some(Value::Utf8(v)) => builder.append(v)?,
                        Some(other) => return Err(mismatch(other)),
                    }
                }
                builder.finish()
            }
        }
    }

    fn primitive_keys<T: Native>(
        &self,
        pick: impl Fn(&Value) -> Option<T>,
    ) -> Result<Column, Error> {
        let mut builder = PrimitiveBuilder::<T>::new();
        for key in &self.keys {
            match key {
                None => builder.append_null(),
                Some(value) => builder.append(pick(value).ok_or_else(|| {
                    Error::TypeMismatch(format!(
                        "la clave {value:?} no es de tipo {:?}",
                        self.key_dtype
                    ))
                })?),
            }
        }
        builder.finish()
    }

    /// Columna de resultados: `Int64` o `Float64` segun [`GroupValues`].
    ///
    /// # Errors
    /// Los del constructor de la columna (no ocurren con un resultado de [`group_by`]).
    pub fn values_column(&self) -> Result<Column, Error> {
        match &self.values {
            GroupValues::Int(values) => {
                let mut builder = PrimitiveBuilder::<i64>::new();
                for &v in values {
                    builder.append(v);
                }
                builder.finish()
            }
            GroupValues::Float(values) => {
                let mut builder = PrimitiveBuilder::<f64>::new();
                for &v in values {
                    builder.append(v);
                }
                builder.finish()
            }
        }
    }
}

/// Resultado entero de un `Number` (`Sum`/`Min`/`Max` sobre una columna entera).
fn int_of(number: Number) -> Result<i64, Error> {
    match number {
        Number::Int(v) => Ok(v),
        Number::Float(_) => Err(Error::TypeMismatch(
            "se esperaba un resultado entero".to_string(),
        )),
    }
}

/// Resultado flotante de un `Number` (`Sum`/`Min`/`Max` sobre una columna `Float64`).
fn float_of(number: Number) -> Result<f64, Error> {
    match number {
        Number::Float(v) => Ok(v),
        Number::Int(_) => Err(Error::TypeMismatch(
            "se esperaba un resultado flotante".to_string(),
        )),
    }
}

/// Agrupa por `keys` y calcula `op` sobre `values` en cada grupo. `ddof` solo cuenta en
/// `Var` y `Std`.
///
/// `Sum`, `Min` y `Max` dan enteros si `values` es `Int32` o `Int64` y flotantes si es
/// `Float64`; `Count` da enteros y el resto flotantes.
///
/// # Errors
/// Los de [`group_aggregate`] y de crear el agregado (p. ej. [`Error::Unsupported`] si
/// `values` es `Bool` o `Utf8`).
pub fn group_by(
    op: GroupOp,
    ddof: usize,
    keys: &Column,
    values: &Column,
) -> Result<GroupResult, Error> {
    group_by_with_workers(op, ddof, keys, values, 1)
}

/// Como [`group_by`], repartiendo entre `workers` hilos los bloques de cada grupo. El
/// resultado no depende de `workers`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`group_by`].
pub fn group_by_with_workers(
    op: GroupOp,
    ddof: usize,
    keys: &Column,
    values: &Column,
    workers: usize,
) -> Result<GroupResult, Error> {
    let dtype = values.dtype();
    let int_values = matches!(dtype, DataType::Int32 | DataType::Int64);
    let (group_keys, group_values) = match op {
        GroupOp::Count => {
            let got = group_aggregate_with_workers(&Count::new(dtype)?, keys, values, workers)?;
            let counts = got
                .results
                .iter()
                .map(|&n| {
                    i64::try_from(n).map_err(|_| Error::OutOfBounds(format!("{n} no cabe en i64")))
                })
                .collect::<Result<Vec<_>, _>>()?;
            (got.keys, GroupValues::Int(counts))
        }
        GroupOp::Sum => number_values(
            group_aggregate_with_workers(&Sum::new(dtype)?, keys, values, workers)?,
            int_values,
        )?,
        GroupOp::Min => number_values(
            group_aggregate_with_workers(&Min::new(dtype)?, keys, values, workers)?,
            int_values,
        )?,
        GroupOp::Max => number_values(
            group_aggregate_with_workers(&Max::new(dtype)?, keys, values, workers)?,
            int_values,
        )?,
        GroupOp::Mean => {
            let got = group_aggregate_with_workers(&Mean::new(dtype)?, keys, values, workers)?;
            (got.keys, GroupValues::Float(got.results))
        }
        GroupOp::Var => {
            let got =
                group_aggregate_with_workers(&Variance::new(dtype, ddof)?, keys, values, workers)?;
            (got.keys, GroupValues::Float(got.results))
        }
        GroupOp::Std => {
            let got =
                group_aggregate_with_workers(&StdDev::new(dtype, ddof)?, keys, values, workers)?;
            (got.keys, GroupValues::Float(got.results))
        }
    };
    Ok(GroupResult {
        key_dtype: keys.dtype(),
        keys: group_keys,
        values: group_values,
    })
}

fn number_values(
    got: Grouped<Number>,
    int_values: bool,
) -> Result<(Vec<Option<Value>>, GroupValues), Error> {
    let values = if int_values {
        GroupValues::Int(
            got.results
                .into_iter()
                .map(int_of)
                .collect::<Result<_, _>>()?,
        )
    } else {
        GroupValues::Float(
            got.results
                .into_iter()
                .map(float_of)
                .collect::<Result<_, _>>()?,
        )
    };
    Ok((got.keys, values))
}
