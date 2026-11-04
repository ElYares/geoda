//! Estadistica de geoda (fase 2). Todo en Rust seguro: ningun `unsafe`.
//!
//! # La idea
//! Un agregado (suma, media, varianza...) se calcula por **bloques**: cada bloque
//! produce un estado parcial y los estados se **fusionan**. Como la suma de flotantes
//! no es asociativa, el resultado solo es reproducible si los bloques y el orden de
//! fusion no dependen de cuantos trabajadores haya (decision S3). Ver [`aggregate`].

#![forbid(unsafe_code)]

pub mod aggregate;
pub mod basic;
pub mod covariance;
pub mod group;
pub mod histogram;
pub mod stream;
pub mod variance;

pub use aggregate::{
    Aggregate, BLOCK, aggregate, aggregate_with_workers, fold_blocks, for_each_valid,
};
pub use basic::{
    Count, Max, Mean, Min, Number, Sum, count_column, max_column, mean_column, min_column,
    sum_column,
};
pub use covariance::{Correlation, Covariance, correlation_columns, covariance_columns};
pub use geoda_algo::{Method, median_column, percentile_column};
pub use group::{GroupOp, GroupResult, GroupValues, Grouped, group_aggregate, group_by};
pub use histogram::{Histogram, HistogramData, MAX_BINS, histogram_column};
pub use stream::Accumulator;
pub use variance::{StdDev, Variance, std_dev_column, variance_column};
