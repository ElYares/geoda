//! Memoria alineada a 64 bytes. Unico modulo del crate con `unsafe`.

#![allow(unsafe_code)]

mod bitmap;
mod buffer;
mod native;

pub use bitmap::Bitmap;
pub(crate) use bitmap::{count_set_bits, get_bit};
pub use buffer::{ALIGNMENT, Buffer, MutableBuffer};
pub use native::Native;
