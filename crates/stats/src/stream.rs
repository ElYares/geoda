//! Agregados incrementales: se alimentan con trozos sucesivos y no retienen la columna.
//!
//! # Mismo arbol que `aggregate`
//! Los trozos se rearman en bloques de [`BLOCK`] valores contando desde el inicio de la
//! serie, sin importar donde caigan los limites de cada trozo. Los estados de los bloques
//! se apilan como un contador binario: dos estados del mismo nivel se fusionan al
//! instante, de modo que quedan a lo sumo `log2(bloques) + 1` estados vivos. Al terminar,
//! la pila se fusiona de derecha a izquierda. Eso reproduce exactamente el arbol por
//! niveles de [`crate::aggregate`], asi que el resultado es identico bit a bit al de la
//! columna entera.

use geoda_core::{BoolBuilder, Column, DataType, Error, PrimitiveBuilder, Utf8Builder};

use crate::aggregate::{Aggregate, BLOCK};

/// Acumulador de un agregado sobre una serie de trozos.
///
/// Si [`Accumulator::push`] devuelve un error del propio agregado (no el de tipo), el
/// acumulador queda a medias y debe descartarse.
pub struct Accumulator<A: Aggregate> {
    agg: A,
    dtype: Option<DataType>,
    /// Filas del bloque en curso (menos de `BLOCK`), copiadas: no retienen el trozo.
    pending: Option<Column>,
    /// Estados de bloques completos con su nivel; los niveles bajan de izquierda a derecha.
    stack: Vec<(u32, A::State)>,
}

impl<A: Aggregate> Accumulator<A> {
    /// Acumulador vacio para `agg`.
    pub fn new(agg: A) -> Self {
        Self {
            agg,
            dtype: None,
            pending: None,
            stack: Vec::new(),
        }
    }

    /// Estados de bloques que se retienen ahora mismo.
    pub fn live_states(&self) -> usize {
        self.stack.len()
    }

    /// Filas del bloque en curso que aun no forman un bloque completo.
    pub fn pending_rows(&self) -> usize {
        self.pending.as_ref().map_or(0, Column::len)
    }

    /// Incorpora el siguiente trozo, que va despues de todos los anteriores.
    ///
    /// # Errors
    /// - [`Error::TypeMismatch`] si el tipo no es el del primer trozo; el acumulador no
    ///   cambia.
    /// - Los de [`Aggregate::update`].
    pub fn push(&mut self, chunk: &Column) -> Result<(), Error> {
        match self.dtype {
            Some(dtype) if dtype != chunk.dtype() => {
                return Err(Error::TypeMismatch(format!(
                    "el trozo es {:?} y los anteriores eran {dtype:?}",
                    chunk.dtype()
                )));
            }
            _ => self.dtype = Some(chunk.dtype()),
        }
        let mut offset = 0;
        if let Some(pending) = self.pending.take() {
            let take = (BLOCK - pending.len()).min(chunk.len());
            let joined = concat(Some(&pending), &chunk.slice(0, take)?)?;
            offset = take;
            if joined.len() == BLOCK {
                self.push_block(&joined)?;
            } else {
                self.pending = Some(joined);
                return Ok(());
            }
        }
        while chunk.len() - offset >= BLOCK {
            self.push_block(&chunk.slice(offset, BLOCK)?)?;
            offset += BLOCK;
        }
        if offset < chunk.len() {
            // Se copia para no mantener vivo el buffer entero del trozo.
            self.pending = Some(concat(None, &chunk.slice(offset, chunk.len() - offset)?)?);
        }
        Ok(())
    }

    /// Resultado sobre todos los trozos recibidos.
    ///
    /// # Errors
    /// Los de [`Aggregate::update`] y [`Aggregate::finalize`].
    pub fn finish(mut self) -> Result<A::Output, Error> {
        if let Some(pending) = self.pending.take() {
            self.push_block(&pending)?;
        }
        let mut states = self.stack.into_iter().rev().map(|(_, state)| state);
        let state = match states.next() {
            None => self.agg.init(),
            Some(last) => states.fold(last, |right, left| self.agg.merge(left, right)),
        };
        self.agg.finalize(state)
    }

    /// Calcula el estado de un bloque completo (o del ultimo, mas corto) y lo apila.
    fn push_block(&mut self, block: &Column) -> Result<(), Error> {
        let mut state = self.agg.init();
        self.agg.update(&mut state, block)?;
        let mut level = 0;
        while let Some(&(top, _)) = self.stack.last() {
            if top != level {
                break;
            }
            if let Some((_, left)) = self.stack.pop() {
                state = self.agg.merge(left, state);
            }
            level += 1;
        }
        self.stack.push((level, state));
        Ok(())
    }
}

/// Calcula `agg` sobre una serie de trozos (por ejemplo un [`geoda_core::file::ColumnFileReader`]),
/// sin tener la columna entera en memoria.
///
/// # Errors
/// El primer error que traiga la serie, y los de [`Accumulator::push`] y
/// [`Accumulator::finish`].
pub fn aggregate_chunks<A: Aggregate>(
    agg: A,
    chunks: impl IntoIterator<Item = Result<Column, Error>>,
) -> Result<A::Output, Error> {
    let mut acc = Accumulator::new(agg);
    for chunk in chunks {
        acc.push(&chunk?)?;
    }
    acc.finish()
}

/// Copia `head` (si hay) y `tail` en una columna nueva, en ese orden.
fn concat(head: Option<&Column>, tail: &Column) -> Result<Column, Error> {
    let parts: Vec<&Column> = head.into_iter().chain([tail]).collect();
    match tail.dtype() {
        DataType::Int32 => concat_primitive::<i32>(&parts),
        DataType::Int64 => concat_primitive::<i64>(&parts),
        DataType::Float64 => concat_primitive::<f64>(&parts),
        DataType::Bool => {
            let mut builder = BoolBuilder::new();
            for part in parts {
                for i in 0..part.len() {
                    if part.is_null(i)? {
                        builder.append_null();
                    } else {
                        builder.append(part.bool_value(i)?);
                    }
                }
            }
            builder.finish()
        }
        DataType::Utf8 => {
            let mut builder = Utf8Builder::new();
            for part in parts {
                for i in 0..part.len() {
                    if part.is_null(i)? {
                        builder.append_null();
                    } else {
                        builder.append(part.str_value(i)?)?;
                    }
                }
            }
            builder.finish()
        }
    }
}

fn concat_primitive<T: geoda_core::memory::Native>(parts: &[&Column]) -> Result<Column, Error> {
    let mut builder = PrimitiveBuilder::<T>::new();
    for part in parts {
        for (i, &value) in part.values::<T>()?.iter().enumerate() {
            if part.is_null(i)? {
                builder.append_null();
            } else {
                builder.append(value);
            }
        }
    }
    builder.finish()
}
