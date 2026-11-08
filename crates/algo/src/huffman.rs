//! Codigo de Huffman sobre bytes.
//!
//! # La idea
//! Dar codigos **cortos** a los simbolos frecuentes y **largos** a los raros, de modo que el
//! total de bits baje. Para que se pueda decodificar sin separadores, ningun codigo puede ser
//! prefijo de otro (codigo libre de prefijos). El algoritmo voraz de Huffman lo consigue:
//!
//! 1. Cada simbolo con frecuencia mayor que cero es una hoja con su peso.
//! 2. Mientras quede mas de un arbol, se sacan los dos de menor peso (con el [`Heap`]) y se
//!    unen bajo un nodo nuevo cuyo peso es la suma.
//! 3. El camino de la raiz a una hoja da su codigo: `0` por la rama izquierda, `1` por la
//!    derecha.
//!
//! Es optimo: ningun codigo libre de prefijos por simbolo gasta menos bits en total.
//!
//! # Determinismo
//! Con pesos iguales el arbol podria salir de varias formas. Aqui se desempata por el
//! **simbolo mas pequeno** que contiene cada arbol (los conjuntos de simbolos son disjuntos,
//! asi que el orden es total). El menor de los dos va a la izquierda. Las mismas frecuencias
//! dan siempre la misma tabla.
//!
//! # Casos limite
//! - Sin simbolos (entrada vacia): no hay arbol; se codifica en 0 bits.
//! - Un solo simbolo distinto: tendria un codigo de 0 bits, que no se puede decodificar;
//!   se le da el codigo `0` de 1 bit.
//!
//! # Salida
//! [`Encoded`] empaqueta los bits de mayor a menor peso dentro de cada byte y guarda cuantos
//! son validos (`bit_len`); los bits de relleno del ultimo byte se ignoran. **No** hay formato
//! de archivo: la tabla no viaja con los datos (fuera de alcance de esta HU).
//!
//! # Limite de profundidad
//! Con frecuencias `u64` el codigo mas largo no pasa de unos 100 bits, asi que cabe en un
//! `u128`. Si no cupiera, [`Huffman::from_frequencies`] devuelve error en vez de truncar.

use geoda_core::Error;

use crate::heap::Heap;

/// Cantidad de simbolos posibles (un byte).
const SYMBOLS: usize = 256;
/// Bits que caben en un codigo.
const MAX_CODE_BITS: u8 = 128;

/// Codigo de un simbolo: los `len` bits bajos de `bits`, el primero (raiz) en el bit mas alto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code {
    /// Los bits del codigo, alineados a la derecha.
    pub bits: u128,
    /// Cuantos bits tiene (entre 1 y 128).
    pub len: u8,
}

/// Bits ya codificados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    /// Bits empaquetados, el primero en el bit mas alto del primer byte.
    pub bytes: Vec<u8>,
    /// Bits validos; `bytes.len() == bit_len.div_ceil(8)`.
    pub bit_len: usize,
}

#[derive(Debug, Clone)]
struct Node {
    /// Hijos izquierdo y derecho; `None` en una hoja.
    children: Option<(usize, usize)>,
    symbol: u8,
}

/// Tabla de codigos de Huffman y el arbol para decodificar.
#[derive(Debug, Clone)]
pub struct Huffman {
    codes: [Option<Code>; SYMBOLS],
    nodes: Vec<Node>,
    root: Option<usize>,
}

impl Huffman {
    /// Construye el codigo con la frecuencia de cada byte; los de frecuencia 0 no tienen codigo.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si algun codigo pasaria de 128 bits (no ocurre con
    /// frecuencias `u64` reales).
    pub fn from_frequencies(frequencies: &[u64; SYMBOLS]) -> Result<Self, Error> {
        let mut nodes: Vec<Node> = Vec::new();
        // (peso, simbolo menor del arbol, nodo)
        let mut heap = Heap::new(|a: &(u128, u8, usize), b: &(u128, u8, usize)| {
            a.0.cmp(&b.0).then(a.1.cmp(&b.1))
        });
        for symbol in 0..=u8::MAX {
            let weight = frequencies[usize::from(symbol)];
            if weight > 0 {
                nodes.push(Node {
                    children: None,
                    symbol,
                });
                heap.push((u128::from(weight), symbol, nodes.len() - 1));
            }
        }
        let mut codes = [None; SYMBOLS];
        // Se unen de dos en dos; el que queda solo al final es la raiz.
        let mut last = None;
        while let Some(left) = heap.pop() {
            let Some(right) = heap.pop() else {
                last = Some(left);
                break;
            };
            nodes.push(Node {
                children: Some((left.2, right.2)),
                symbol: 0,
            });
            heap.push((left.0 + right.0, left.1.min(right.1), nodes.len() - 1));
        }
        let Some((_, only_symbol, root)) = last else {
            return Ok(Self {
                codes,
                nodes,
                root: None,
            });
        };
        if nodes[root].children.is_none() {
            // Un solo simbolo: codigo `0` de 1 bit; el arbol es solo esa hoja.
            codes[usize::from(only_symbol)] = Some(Code { bits: 0, len: 1 });
            return Ok(Self {
                codes,
                nodes,
                root: Some(root),
            });
        }
        // Recorrido con pila explicita: (nodo, bits, largo).
        let mut stack = vec![(root, 0u128, 0u8)];
        while let Some((node, bits, len)) = stack.pop() {
            match nodes[node].children {
                Some((left, right)) => {
                    if len >= MAX_CODE_BITS {
                        return Err(Error::InvalidArgument(
                            "un codigo de Huffman pasaria de 128 bits".to_string(),
                        ));
                    }
                    stack.push((right, (bits << 1) | 1, len + 1));
                    stack.push((left, bits << 1, len + 1));
                }
                None => codes[usize::from(nodes[node].symbol)] = Some(Code { bits, len }),
            }
        }
        Ok(Self {
            codes,
            nodes,
            root: Some(root),
        })
    }

    /// Cuenta los bytes de `data` y construye el codigo.
    ///
    /// # Errors
    /// Los de [`Huffman::from_frequencies`].
    pub fn from_data(data: &[u8]) -> Result<Self, Error> {
        Self::from_frequencies(&frequencies(data))
    }

    /// Codigo de `symbol`, o `None` si no aparece en las frecuencias.
    #[must_use]
    pub fn code(&self, symbol: u8) -> Option<Code> {
        self.codes[usize::from(symbol)]
    }

    /// Total de bits que ocuparian los datos con estas frecuencias (el costo que Huffman minimiza).
    #[must_use]
    pub fn weighted_length(&self, frequencies: &[u64; SYMBOLS]) -> u128 {
        self.codes
            .iter()
            .zip(frequencies)
            .filter_map(|(code, &count)| code.map(|c| u128::from(c.len) * u128::from(count)))
            .sum()
    }

    /// Codifica `data`.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si algun byte no tiene codigo en esta tabla.
    pub fn encode(&self, data: &[u8]) -> Result<Encoded, Error> {
        let mut bytes: Vec<u8> = Vec::new();
        let mut bit_len: usize = 0;
        for &symbol in data {
            let code = self.codes[usize::from(symbol)].ok_or_else(|| {
                Error::InvalidArgument(format!("el byte {symbol} no tiene codigo"))
            })?;
            for shift in (0..code.len).rev() {
                if bit_len.is_multiple_of(8) {
                    bytes.push(0);
                }
                if (code.bits >> shift) & 1 == 1 {
                    let last = bytes.len() - 1;
                    bytes[last] |= 0x80 >> (bit_len % 8);
                }
                bit_len += 1;
            }
        }
        Ok(Encoded { bytes, bit_len })
    }

    /// Decodifica los primeros `bit_len` bits de `bytes`.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si `bit_len` pasa de los bits que hay, si la secuencia
    /// termina a media palabra, o si no es posible con este codigo (bits sin simbolos, o un
    /// `1` cuando solo existe un simbolo).
    pub fn decode(&self, bytes: &[u8], bit_len: usize) -> Result<Vec<u8>, Error> {
        if bit_len > bytes.len().saturating_mul(8) {
            return Err(Error::InvalidArgument(format!(
                "bit_len {bit_len} pasa de los {} bits disponibles",
                bytes.len().saturating_mul(8)
            )));
        }
        let mut out = Vec::new();
        let Some(root) = self.root else {
            return if bit_len == 0 {
                Ok(out)
            } else {
                Err(Error::InvalidArgument(
                    "hay bits pero el codigo no tiene simbolos".to_string(),
                ))
            };
        };
        let mut node = root;
        for position in 0..bit_len {
            let bit = (bytes[position / 8] >> (7 - position % 8)) & 1;
            match self.nodes[node].children {
                Some((left, right)) => node = if bit == 0 { left } else { right },
                // Raiz que es hoja: un solo simbolo, cuyo unico codigo es `0`.
                None if bit == 0 => {}
                None => {
                    return Err(Error::InvalidArgument(
                        "bit 1 en un codigo de un solo simbolo".to_string(),
                    ));
                }
            }
            if self.nodes[node].children.is_none() {
                out.push(self.nodes[node].symbol);
                node = root;
            }
        }
        if node != root {
            return Err(Error::InvalidArgument(
                "la secuencia de bits termina a media palabra".to_string(),
            ));
        }
        Ok(out)
    }
}

/// Cuantas veces aparece cada byte en `data`.
#[must_use]
pub fn frequencies(data: &[u8]) -> [u64; SYMBOLS] {
    let mut counts = [0u64; SYMBOLS];
    for &byte in data {
        counts[usize::from(byte)] += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::{Huffman, SYMBOLS, frequencies};
    use geoda_core::Error;

    fn clrs() -> [u64; SYMBOLS] {
        let mut freq = [0u64; SYMBOLS];
        for (symbol, count) in [
            (b'a', 45),
            (b'b', 13),
            (b'c', 12),
            (b'd', 16),
            (b'e', 9),
            (b'f', 5),
        ] {
            freq[usize::from(symbol)] = count;
        }
        freq
    }

    #[test]
    fn la_longitud_ponderada_del_ejemplo_clasico_es_224() {
        let freq = clrs();
        let huffman = Huffman::from_frequencies(&freq).unwrap();
        assert_eq!(huffman.weighted_length(&freq), 224);
        assert_eq!(huffman.code(b'a').unwrap().len, 1);
    }

    #[test]
    fn ningun_codigo_es_prefijo_de_otro() {
        let huffman = Huffman::from_frequencies(&clrs()).unwrap();
        let codes: Vec<_> = (0..=255u8).filter_map(|s| huffman.code(s)).collect();
        for (i, a) in codes.iter().enumerate() {
            for (j, b) in codes.iter().enumerate() {
                if i != j && a.len <= b.len {
                    assert_ne!(
                        b.bits >> (b.len - a.len),
                        a.bits,
                        "{a:?} es prefijo de {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn ida_y_vuelta() {
        let text = b"abracadabra alakazam, huffman es un algoritmo voraz".repeat(7);
        let huffman = Huffman::from_data(&text).unwrap();
        let encoded = huffman.encode(&text).unwrap();
        assert!(encoded.bit_len < text.len() * 8);
        assert_eq!(encoded.bytes.len(), encoded.bit_len.div_ceil(8));
        assert_eq!(
            huffman.decode(&encoded.bytes, encoded.bit_len).unwrap(),
            text
        );
    }

    #[test]
    fn todos_los_bytes_ida_y_vuelta() {
        let data: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
        let huffman = Huffman::from_data(&data).unwrap();
        let encoded = huffman.encode(&data).unwrap();
        assert_eq!(
            huffman.decode(&encoded.bytes, encoded.bit_len).unwrap(),
            data
        );
    }

    #[test]
    fn un_solo_simbolo() {
        let data = b"zzzzzz";
        let huffman = Huffman::from_data(data).unwrap();
        assert_eq!(huffman.code(b'z').unwrap().len, 1);
        let encoded = huffman.encode(data).unwrap();
        assert_eq!(encoded.bit_len, 6);
        assert_eq!(
            huffman.decode(&encoded.bytes, encoded.bit_len).unwrap(),
            data
        );
        assert!(matches!(
            huffman.decode(&[0b0100_0000], 2),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn entrada_vacia() {
        let huffman = Huffman::from_data(b"").unwrap();
        let encoded = huffman.encode(b"").unwrap();
        assert_eq!((encoded.bit_len, encoded.bytes.len()), (0, 0));
        assert_eq!(huffman.decode(&[], 0).unwrap(), Vec::<u8>::new());
        assert!(matches!(
            huffman.decode(&[0], 3),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn bits_truncados_o_invalidos_son_error() {
        let huffman = Huffman::from_data(b"aaaabbbcc").unwrap();
        let encoded = huffman.encode(b"abc").unwrap();
        // Quitar el ultimo bit deja una palabra a medias (o una distinta): nunca panico.
        let _ = huffman.decode(&encoded.bytes, encoded.bit_len - 1);
        assert!(matches!(
            huffman.decode(&encoded.bytes, encoded.bytes.len() * 8 + 1),
            Err(Error::InvalidArgument(_))
        ));
        // Un simbolo que no esta en la tabla no se puede codificar.
        assert!(matches!(
            huffman.encode(b"z"),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn decodificar_bits_al_azar_no_entra_en_panico() {
        let huffman = Huffman::from_data(b"mississippi river").unwrap();
        let mut state: u64 = 7;
        for len in 0..64usize {
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    u8::try_from(state >> 56).unwrap()
                })
                .collect();
            let _ = huffman.decode(&bytes, bytes.len() * 8);
            let _ = huffman.decode(&bytes, bytes.len() * 8 - bytes.len().min(3));
        }
    }

    #[test]
    fn las_mismas_frecuencias_dan_la_misma_tabla() {
        let freq = frequencies(b"el rapido zorro marron salta sobre el perro perezoso");
        let first = Huffman::from_frequencies(&freq).unwrap();
        for _ in 0..5 {
            let again = Huffman::from_frequencies(&freq).unwrap();
            for symbol in 0..=255u8 {
                assert_eq!(first.code(symbol), again.code(symbol));
            }
        }
    }

    #[test]
    fn frecuencias_de_fibonacci_dan_el_codigo_mas_profundo() {
        let mut freq = [0u64; SYMBOLS];
        let (mut a, mut b) = (1u64, 1u64);
        for slot in freq.iter_mut().take(60) {
            *slot = a;
            (a, b) = (b, a + b);
        }
        let huffman = Huffman::from_frequencies(&freq).unwrap();
        assert_eq!(huffman.code(0).unwrap().len, 59);
    }
}
