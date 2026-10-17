//! `MutableBuffer` (escribible, dueno unico) y `Buffer` (solo lectura, compartible).

use std::alloc::{self, Layout};
use std::num::NonZeroUsize;
use std::ptr::NonNull;
use std::sync::Arc;

use super::Native;
use crate::Error;

/// Alineacion en bytes de todo bloque: una linea de cache y un registro AVX-512.
pub const ALIGNMENT: usize = 64;

/// Bloque de memoria alineado a [`ALIGNMENT`]. Lo libera al soltarse.
///
/// Es el unico dueno de la reserva; `MutableBuffer` y `Buffer` solo lo envuelven.
struct Allocation {
    ptr: NonNull<u8>,
    len: usize,
}

// SAFETY: `Allocation` es dueno unico de su bloque y no guarda estado compartido
// ni mutable por si misma; mover o compartir el puntero entre hilos no crea
// accesos concurrentes. Quien escribe lo hace con `&mut`, y quien lee con `&`.
unsafe impl Send for Allocation {}
// SAFETY: ver `Send`; desde `&Allocation` solo se pueden hacer lecturas.
unsafe impl Sync for Allocation {}

impl Allocation {
    /// Reserva `len` bytes en cero. Con `len == 0` no pide memoria al alocador.
    fn zeroed(len: usize) -> Result<Self, Error> {
        if len == 0 {
            // Puntero "colgante" pero alineado: nunca se lee ni se libera.
            return Ok(Self {
                ptr: NonNull::without_provenance(NonZeroUsize::new(ALIGNMENT).expect("64 != 0")),
                len,
            });
        }
        let layout = Layout::from_size_align(len, ALIGNMENT).map_err(|e| {
            Error::InvalidArgument(format!("tamano {len} no se puede reservar: {e}"))
        })?;
        // SAFETY: `layout` tiene tamano distinto de cero (comprobado arriba).
        let raw = unsafe { alloc::alloc_zeroed(layout) };
        let ptr = NonNull::new(raw)
            .ok_or_else(|| Error::Alloc(format!("sin memoria para {len} bytes")))?;
        Ok(Self { ptr, len })
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        if self.len == 0 {
            return;
        }
        // SAFETY: con `len > 0`, `ptr` salio de `alloc_zeroed` con este mismo
        // tamano y alineacion, y `drop` corre una sola vez.
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.len, ALIGNMENT);
            alloc::dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

/// Bloque de bytes escribible, alineado a 64 y con dueno unico.
pub struct MutableBuffer {
    data: Allocation,
}

impl MutableBuffer {
    /// Reserva `len` bytes inicializados a cero.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si `len` excede el maximo de `Layout`, o
    /// [`Error::Alloc`] si el sistema no tiene memoria.
    pub fn zeroed(len: usize) -> Result<Self, Error> {
        Ok(Self {
            data: Allocation::zeroed(len)?,
        })
    }

    /// Cantidad de bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len
    }

    /// `true` si no tiene bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.len == 0
    }

    /// Bytes de solo lectura.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `ptr` es valido para `len` bytes inicializados (cero al reservar)
        // y el prestamo `&self` impide una escritura simultanea.
        unsafe { std::slice::from_raw_parts(self.data.ptr.as_ptr(), self.data.len) }
    }

    /// Bytes escribibles.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: igual que `as_slice`, y `&mut self` garantiza acceso exclusivo.
        unsafe { std::slice::from_raw_parts_mut(self.data.ptr.as_ptr(), self.data.len) }
    }

    /// Congela el bloque: ya no se puede escribir, pero se puede compartir.
    #[must_use]
    pub fn into_buffer(self) -> Buffer {
        let len = self.data.len;
        Buffer {
            data: Arc::new(self.data),
            offset: 0,
            len,
        }
    }
}

/// Ventana de solo lectura sobre un bloque compartido. Clonar o cortar no copia bytes.
#[derive(Clone)]
pub struct Buffer {
    data: Arc<Allocation>,
    offset: usize,
    len: usize,
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl Buffer {
    /// Cantidad de bytes de la ventana.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` si la ventana no tiene bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Puntero al primer byte de la ventana: base del bloque mas `offset`.
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        // SAFETY: `offset <= data.len` (lo asegura `slice`), asi que el resultado
        // queda dentro del bloque o un byte despues de su final.
        unsafe { self.data.ptr.as_ptr().add(self.offset) }
    }

    /// Bytes de la ventana.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `offset + len <= data.len` (invariante de `slice`); el bloque
        // vive mientras exista este `Arc` y nadie lo muta tras `into_buffer`.
        unsafe { std::slice::from_raw_parts(self.as_ptr(), self.len) }
    }

    /// Copia `bytes` a un bloque nuevo alineado a 64 y lo congela.
    ///
    /// # Errors
    /// Los de [`MutableBuffer::zeroed`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut m = MutableBuffer::zeroed(bytes.len())?;
        m.as_mut_slice().copy_from_slice(bytes);
        Ok(m.into_buffer())
    }

    /// Reinterpreta la ventana como `[T]`, sin copiar.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si el largo no es multiplo de `T::WIDTH` o si
    /// el puntero (por un `slice` con offset) no esta alineado para `T`.
    pub fn typed<T: Native>(&self) -> Result<&[T], Error> {
        if !self.len.is_multiple_of(T::WIDTH) {
            return Err(Error::InvalidArgument(format!(
                "{} bytes no son multiplo de {}",
                self.len,
                T::WIDTH
            )));
        }
        if !(self.as_ptr() as usize).is_multiple_of(std::mem::align_of::<T>()) {
            return Err(Error::InvalidArgument(format!(
                "buffer desalineado para un tipo de {} bytes",
                T::WIDTH
            )));
        }
        // SAFETY: el puntero esta alineado para `T` y `len` es multiplo de su
        // ancho (comprobado arriba), asi que `len / WIDTH` valores caben en la
        // ventana. `Native` esta sellado a tipos donde todo patron de bits es un
        // valor valido, y la memoria vive y no se muta mientras exista `&self`.
        Ok(unsafe { std::slice::from_raw_parts(self.as_ptr().cast::<T>(), self.len / T::WIDTH) })
    }

    /// Sub-ventana `[offset, offset + len)` sobre el mismo bloque, sin copiar.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si la ventana se sale de este `Buffer`.
    pub fn slice(&self, offset: usize, len: usize) -> Result<Self, Error> {
        offset
            .checked_add(len)
            .filter(|&end| end <= self.len)
            .ok_or_else(|| {
                Error::OutOfBounds(format!(
                    "slice({offset}, {len}) sobre un buffer de {} bytes",
                    self.len
                ))
            })?;
        Ok(Self {
            data: Arc::clone(&self.data),
            offset: self.offset + offset,
            len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ALIGNMENT, Buffer, MutableBuffer};
    use crate::Error;

    fn buffer_con(bytes: &[u8]) -> Buffer {
        let mut m = MutableBuffer::zeroed(bytes.len()).unwrap();
        m.as_mut_slice().copy_from_slice(bytes);
        m.into_buffer()
    }

    #[test]
    fn puntero_alineado_a_64_para_varios_tamanos() {
        for len in [0, 1, 7, 63, 64, 65, 1000, 4096] {
            let b = MutableBuffer::zeroed(len).unwrap().into_buffer();
            assert_eq!(b.as_ptr() as usize % ALIGNMENT, 0, "len = {len}");
        }
    }

    #[test]
    fn nace_en_ceros() {
        let m = MutableBuffer::zeroed(100).unwrap();
        assert!(m.as_slice().iter().all(|&b| b == 0));
    }

    #[test]
    fn lo_escrito_se_lee_tras_congelar() {
        let b = buffer_con(&[1, 2, 3, 4]);
        assert_eq!(b.as_slice(), &[1, 2, 3, 4]);
    }

    #[test]
    fn slice_no_copia_y_suma_el_offset() {
        let b = buffer_con(&[10, 11, 12, 13, 14]);
        let s = b.slice(2, 3).unwrap();
        assert_eq!(s.as_slice(), &[12, 13, 14]);
        assert_eq!(s.as_ptr() as usize, b.as_ptr() as usize + 2);
    }

    #[test]
    fn slice_de_slice_acumula_offset() {
        let b = buffer_con(&[0, 1, 2, 3, 4, 5]);
        let s = b.slice(1, 4).unwrap().slice(2, 2).unwrap();
        assert_eq!(s.as_slice(), &[3, 4]);
    }

    #[test]
    fn slice_fuera_de_rango_es_err() {
        let b = buffer_con(&[0; 8]);
        assert!(matches!(b.slice(9, 0), Err(Error::OutOfBounds(_))));
        assert!(matches!(b.slice(4, 5), Err(Error::OutOfBounds(_))));
        assert!(matches!(b.slice(usize::MAX, 2), Err(Error::OutOfBounds(_))));
        assert!(b.slice(8, 0).is_ok());
    }

    #[test]
    fn tamano_imposible_es_err() {
        assert!(MutableBuffer::zeroed(usize::MAX).is_err());
    }

    #[test]
    fn el_bloque_sobrevive_mientras_haya_una_ventana() {
        let ventana = {
            let b = buffer_con(&[7; 16]);
            b.slice(4, 8).unwrap()
        };
        assert_eq!(ventana.as_slice(), &[7; 8]);
    }

    #[test]
    fn buffer_se_puede_enviar_a_otro_hilo() {
        let b = buffer_con(&[1, 2, 3]);
        let suma =
            std::thread::spawn(move || b.as_slice().iter().map(|&x| u32::from(x)).sum::<u32>())
                .join()
                .unwrap();
        assert_eq!(suma, 6);
    }

    #[test]
    fn from_bytes_copia_y_alinea() {
        let b = Buffer::from_bytes(&[1, 2, 3]).unwrap();
        assert_eq!(b.as_slice(), &[1, 2, 3]);
        assert_eq!(b.as_ptr() as usize % ALIGNMENT, 0);
    }

    #[test]
    fn typed_lee_los_bytes_como_numeros() {
        let bytes: Vec<u8> = [7_i32, -1, 300]
            .iter()
            .flat_map(|x| x.to_ne_bytes())
            .collect();
        let b = Buffer::from_bytes(&bytes).unwrap();
        assert_eq!(b.typed::<i32>().unwrap(), &[7, -1, 300]);
        assert_eq!(
            Buffer::from_bytes(&[]).unwrap().typed::<f64>().unwrap(),
            &[] as &[f64]
        );
    }

    #[test]
    fn typed_con_largo_no_multiplo_es_err() {
        let b = Buffer::from_bytes(&[0; 6]).unwrap();
        assert!(matches!(b.typed::<i32>(), Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn typed_desalineado_es_err() {
        // Base alineada a 64, pero un slice en el byte 2 no lo esta para i32.
        let b = Buffer::from_bytes(&[0; 16]).unwrap().slice(2, 8).unwrap();
        assert!(matches!(b.typed::<i32>(), Err(Error::InvalidArgument(_))));
        // Un slice en un multiplo del ancho si vale.
        let b = Buffer::from_bytes(&[0; 16]).unwrap().slice(4, 8).unwrap();
        assert_eq!(b.typed::<i32>().unwrap().len(), 2);
    }
}
