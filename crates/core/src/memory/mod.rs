//! Memoria alineada a 64 bytes. Unico modulo del crate con `unsafe`.

#![allow(unsafe_code)]

mod bitmap;
mod buffer;
mod native;

pub use bitmap::Bitmap;
pub use buffer::{ALIGNMENT, Buffer, MutableBuffer};
pub use native::Native;
