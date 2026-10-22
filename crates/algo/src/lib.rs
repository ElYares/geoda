//! Algoritmos base de geoda (fase 1). Todo en Rust seguro: ningun `unsafe`.
//!
//! Los algoritmos se escriben a mano (el proyecto es «desde cero»); la biblioteca
//! estandar solo se usa como oraculo en los tests.

#![forbid(unsafe_code)]

pub mod percentile;
pub mod search;
pub mod select;
pub mod sort;

pub use percentile::{median_column, percentile_column, percentiles_column};
pub use search::{lower_bound, lower_bound_column, upper_bound, upper_bound_column};
pub use select::{Method, Scalar, median_of_medians, quickselect, select_column};
pub use sort::{merge_sort, sort_column};
