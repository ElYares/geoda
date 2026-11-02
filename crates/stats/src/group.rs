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
use geoda_core::{Column, DataType, Error, PrimitiveBuilder};

use crate::aggregate::{Aggregate, aggregate};
use crate::basic::kind_of;

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
        results.push(aggregate(agg, &take(values, group_rows)?)?);
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
