//! Marco de agregados con estado fusionable y troceo determinista (decision S3).
//!
//! # El orden de fusion
//! La columna se parte en bloques de [`BLOCK`] valores. Los limites de los bloques
//! dependen solo de la longitud de la columna, nunca del numero de trabajadores. Los
//! estados de los bloques se fusionan por niveles, en pares vecinos por indice de bloque:
//!
//! ```text
//! [s0 s1 s2 s3 s4]  ->  [m(s0,s1) m(s2,s3) s4]  ->  [m(m01,m23) s4]  ->  [m(m0123,s4)]
//! ```
//!
//! Quien haya calculado cada bloque da igual: el arbol es siempre el mismo, asi que el
//! resultado es identico bit a bit con 1 o con N trabajadores.

use geoda_core::memory::Native;
use geoda_core::{Column, Error};

/// Valores por bloque. Es un punto de partida (se ajusta con `bench/`): cambiarlo cambia
/// los ultimos bits de los agregados de flotantes, porque cambia el arbol de fusion.
pub const BLOCK: usize = 1024;

/// Agregado calculable por bloques con estado fusionable.
///
/// `merge` debe ser el equivalente de haber hecho `update` con los datos de ambos
/// estados; el marco garantiza el orden (`left` son los bloques anteriores a `right`).
pub trait Aggregate {
    /// Estado parcial de un tramo de la columna.
    type State;
    /// Resultado final.
    type Output;

    /// Estado de un tramo vacio. Es el neutro de `merge`.
    fn init(&self) -> Self::State;

    /// Incorpora un bloque de la columna al estado. Los nulos se ignoran (decision F1).
    ///
    /// # Errors
    /// [`Error::Unsupported`] si el tipo de la columna no lo maneja el agregado.
    fn update(&self, state: &mut Self::State, block: &Column) -> Result<(), Error>;

    /// Junta dos estados; `left` cubre bloques anteriores a los de `right`.
    fn merge(&self, left: Self::State, right: Self::State) -> Self::State;

    /// Resultado a partir del estado completo.
    ///
    /// # Errors
    /// Depende del agregado (p. ej. una media sin valores validos).
    fn finalize(&self, state: Self::State) -> Result<Self::Output, Error>;
}

/// Llama a `f` con cada valor no nulo de `block`, en orden.
///
/// # Errors
/// [`Error::TypeMismatch`] si `T` no es el tipo de la columna.
pub fn for_each_valid<T: Native>(block: &Column, mut f: impl FnMut(T)) -> Result<(), Error> {
    let values = block.values::<T>()?;
    for (i, &value) in values.iter().enumerate() {
        if !block.is_null(i)? {
            f(value);
        }
    }
    Ok(())
}

/// Calcula `agg` sobre la columna, bloque por bloque y fusionando con el arbol fijo.
///
/// # Errors
/// Los de [`Aggregate::update`] y [`Aggregate::finalize`].
pub fn aggregate<A: Aggregate>(agg: &A, column: &Column) -> Result<A::Output, Error> {
    aggregate_with_workers(agg, column, 1)
}

/// Como [`aggregate`], pero simulando `workers` trabajadores que se reparten los bloques
/// (el `w`-esimo toma los bloques `w`, `w + workers`, ...) en un solo hilo. El resultado
/// no depende de `workers`; la prueba existe para que la fase 3 pueda paralelizar sin
/// cambiar los numeros.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de [`aggregate`].
pub fn aggregate_with_workers<A: Aggregate>(
    agg: &A,
    column: &Column,
    workers: usize,
) -> Result<A::Output, Error> {
    let state = fold_blocks(
        column.len(),
        workers,
        || agg.init(),
        |offset, len| {
            let block = column.slice(offset, len)?;
            let mut state = agg.init();
            agg.update(&mut state, &block)?;
            Ok(state)
        },
        |left, right| agg.merge(left, right),
    )?;
    agg.finalize(state)
}

/// Parte `len` filas en bloques de [`BLOCK`], calcula el estado de cada uno con `compute`
/// (que recibe `offset` y largo del bloque) y los fusiona con el arbol fijo. Es el
/// armazon de [`aggregate_with_workers`], expuesto para agregados que miran mas de una
/// columna (covarianza). `workers` se reparte los bloques como en esa funcion.
///
/// # Errors
/// [`Error::InvalidArgument`] si `workers` es 0; ademas los de `compute`.
pub fn fold_blocks<S>(
    len: usize,
    workers: usize,
    init: impl Fn() -> S,
    compute: impl Fn(usize, usize) -> Result<S, Error>,
    merge: impl Fn(S, S) -> S,
) -> Result<S, Error> {
    if workers == 0 {
        return Err(Error::InvalidArgument(
            "se necesita al menos un trabajador".to_string(),
        ));
    }
    let blocks = len.div_ceil(BLOCK);
    // Cada trabajador deja su estado en la posicion del bloque, no en la suya.
    let mut states: Vec<Option<S>> = (0..blocks).map(|_| None).collect();
    for worker in 0..workers {
        for index in (worker..blocks).step_by(workers) {
            let offset = index * BLOCK;
            states[index] = Some(compute(offset, BLOCK.min(len - offset))?);
        }
    }
    let ordered: Vec<S> = states.into_iter().flatten().collect();
    Ok(merge_tree(ordered, &init, &merge))
}

/// Fusiona por niveles, en pares vecinos; un estado impar pasa tal cual al nivel siguiente.
fn merge_tree<S>(mut level: Vec<S>, init: &impl Fn() -> S, merge: &impl Fn(S, S) -> S) -> S {
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut pending = level.into_iter();
        while let Some(left) = pending.next() {
            next.push(match pending.next() {
                Some(right) => merge(left, right),
                None => left,
            });
        }
        level = next;
    }
    level.pop().unwrap_or_else(init)
}
