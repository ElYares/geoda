//! Estadistica de geoda (fase 2). Todo en Rust seguro: ningun `unsafe`.
//!
//! # La idea
//! Un agregado (suma, media, varianza...) se calcula por **bloques**: cada bloque
//! produce un estado parcial y los estados se **fusionan**. Como la suma de flotantes
//! no es asociativa, el resultado solo es reproducible si los bloques y el orden de
//! fusion no dependen de cuantos trabajadores haya (decision S3). Ver [`aggregate`].

#![forbid(unsafe_code)]

pub mod aggregate;

pub use aggregate::{Aggregate, BLOCK, aggregate, aggregate_with_workers, for_each_valid};
pub use geoda_algo::{Method, median_column, percentile_column};
