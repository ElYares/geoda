//! Bitmap: un bit por valor, 1 = valido, 0 = nulo. Orden LSB primero, como Arrow.

use super::MutableBuffer;
use crate::Error;

/// Secuencia de bits sobre memoria alineada.
///
/// Invariante: los bits del ultimo byte que sobran (posiciones `>= len`) son
/// siempre 0, porque el bloque nace en ceros y `set` solo toca posiciones `< len`.
/// Por eso `count_set` puede sumar bytes completos sin enmascarar.
pub struct Bitmap {
    bits: MutableBuffer,
    len: usize, // cantidad de bits, no de bytes
}

impl Bitmap {
    /// Crea un bitmap de `len` bits, todos en 0.
    ///
    /// # Errors
    /// [`Error::Alloc`] o [`Error::InvalidArgument`] si no se puede reservar
    /// `len.div_ceil(8)` bytes.
    pub fn new(len: usize) -> Result<Self, Error> {
        Ok(Self {
            bits: MutableBuffer::zeroed(len.div_ceil(8))?,
            len,
        })
    }

    /// Cantidad de bits.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` si no tiene bits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Lee el bit `i`.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `i >= len`.
    pub fn get(&self, i: usize) -> Result<bool, Error> {
        let (byte, mascara) = self.posicion(i)?;
        Ok(self.bits.as_slice()[byte] & mascara != 0)
    }

    /// Escribe el bit `i`.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `i >= len`.
    pub fn set(&mut self, i: usize, valor: bool) -> Result<(), Error> {
        let (byte, mascara) = self.posicion(i)?;
        let b = &mut self.bits.as_mut_slice()[byte];
        if valor {
            *b |= mascara;
        } else {
            *b &= !mascara;
        }
        Ok(())
    }

    /// Cuantos bits estan en 1.
    #[must_use]
    pub fn count_set(&self) -> usize {
        self.bits
            .as_slice()
            .iter()
            .map(|b| b.count_ones() as usize)
            .sum()
    }

    /// Byte y mascara del bit `i`, o `OutOfBounds` si se sale de `len`.
    fn posicion(&self, i: usize) -> Result<(usize, u8), Error> {
        if i >= self.len {
            return Err(Error::OutOfBounds(format!(
                "bit {i} en un bitmap de {} bits",
                self.len
            )));
        }
        Ok((i / 8, 1 << (i % 8)))
    }
}

#[cfg(test)]
mod tests {
    use super::Bitmap;
    use crate::Error;

    /// xorshift64: pseudoaleatorio, determinista y sin dependencias.
    struct Xorshift(u64);

    impl Xorshift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn nace_todo_en_cero() {
        let b = Bitmap::new(21).unwrap();
        assert_eq!(b.len(), 21);
        assert_eq!(b.count_set(), 0);
        assert!((0..21).all(|i| !b.get(i).unwrap()));
    }

    #[test]
    fn vacio_es_valido() {
        let b = Bitmap::new(0).unwrap();
        assert!(b.is_empty());
        assert_eq!(b.count_set(), 0);
        assert!(b.get(0).is_err());
    }

    #[test]
    fn set_y_get_basico() {
        let mut b = Bitmap::new(21).unwrap();
        b.set(0, true).unwrap();
        b.set(8, true).unwrap();
        b.set(20, true).unwrap();
        assert!(b.get(0).unwrap() && b.get(8).unwrap() && b.get(20).unwrap());
        assert!(!b.get(1).unwrap() && !b.get(19).unwrap());
        assert_eq!(b.count_set(), 3);
    }

    #[test]
    fn set_false_apaga_sin_tocar_vecinos() {
        let mut b = Bitmap::new(8).unwrap();
        for i in 0..8 {
            b.set(i, true).unwrap();
        }
        b.set(3, false).unwrap();
        assert!(!b.get(3).unwrap());
        assert_eq!(b.count_set(), 7);
    }

    #[test]
    fn orden_lsb_primero() {
        let mut b = Bitmap::new(8).unwrap();
        b.set(0, true).unwrap();
        b.set(2, true).unwrap();
        assert_eq!(b.bits.as_slice()[0], 0b0000_0101);
    }

    #[test]
    fn fuera_de_rango_es_err() {
        let mut b = Bitmap::new(10).unwrap();
        assert!(matches!(b.get(10), Err(Error::OutOfBounds(_))));
        assert!(matches!(b.set(10, true), Err(Error::OutOfBounds(_))));
        assert!(matches!(b.get(usize::MAX), Err(Error::OutOfBounds(_))));
        // El bit 12 cae dentro del segundo byte pero fuera de `len`.
        assert!(b.set(12, true).is_err());
        assert_eq!(b.count_set(), 0);
    }

    #[test]
    fn coincide_con_vec_bool_con_semilla_fija() {
        for len in [1_usize, 7, 8, 9, 21, 64, 65, 1000] {
            let mut bitmap = Bitmap::new(len).unwrap();
            let mut referencia = vec![false; len];
            let mut rng = Xorshift(0x9E37_79B9_7F4A_7C15);
            let n = u64::try_from(len).unwrap();

            for _ in 0..2000 {
                let i = usize::try_from(rng.next() % n).unwrap();
                let valor = rng.next().is_multiple_of(2);
                bitmap.set(i, valor).unwrap();
                referencia[i] = valor;
            }

            for (i, &esperado) in referencia.iter().enumerate() {
                assert_eq!(bitmap.get(i).unwrap(), esperado, "len = {len}, i = {i}");
            }
            assert_eq!(
                bitmap.count_set(),
                referencia.iter().filter(|&&v| v).count(),
                "len = {len}"
            );
        }
    }
}
