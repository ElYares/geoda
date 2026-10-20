//! Algoritmos base de geoda (fase 1). Todo en Rust seguro: ningun `unsafe`.
//!
//! Los algoritmos se escriben a mano (el proyecto es «desde cero»); la biblioteca
//! estandar solo se usa como oraculo en los tests.

#![forbid(unsafe_code)]

pub mod sort;

pub use sort::{merge_sort, sort_column};
