//! Distancia de edicion (Levenshtein) entre dos cadenas.
//!
//! # La idea
//! La distancia es el minimo de operaciones para convertir una cadena en otra, donde una
//! operacion es insertar, borrar o sustituir **un caracter**, cada una con costo 1. Se
//! resuelve con programacion dinamica: `d[i][j]` es la distancia entre los primeros `i`
//! caracteres de `a` y los primeros `j` de `b`, y sale de tres vecinas:
//!
//! ```text
//! d[i][j] = min( d[i-1][j] + 1,                 // borrar a[i]
//!                d[i][j-1] + 1,                 // insertar b[j]
//!                d[i-1][j-1] + (a[i] != b[j]) ) // sustituir (o nada si son iguales)
//! ```
//!
//! # Memoria
//! Cada fila solo mira la anterior, asi que bastan **dos filas** del largo de la cadena mas
//! corta: O(min(n, m)) de espacio y O(n * m) de tiempo. A cambio no se puede reconstruir el
//! alineamiento (la lista de operaciones), que queda fuera de alcance.
//!
//! # Unidad
//! Se cuenta por `char` (valor escalar Unicode), no por byte ni por grafema: `"añb"` y
//! `"anb"` estan a 1. Un caracter compuesto con acento combinante son dos `char`.
//!
//! # Cota
//! `max_distance` permite cortar antes: si la diferencia de longitudes ya supera el tope,
//! o todas las celdas de una fila lo superan, se devuelve `None` sin terminar la tabla.

/// Distancia de edicion entre `a` y `b`, contada en caracteres Unicode.
///
/// Simetrica: `edit_distance(a, b) == edit_distance(b, a)`.
#[must_use]
pub fn edit_distance(a: &str, b: &str) -> usize {
    // Sin tope siempre hay resultado; el `unwrap_or` solo evita un `panic` inalcanzable.
    distance(a, b, usize::MAX).unwrap_or(usize::MAX)
}

/// Como [`edit_distance`], pero devuelve `None` si la distancia es mayor que `max_distance`.
///
/// Corta antes de terminar la tabla, asi que es mas rapida cuando se buscan solo cadenas
/// cercanas (deduplicacion).
#[must_use]
pub fn edit_distance_within(a: &str, b: &str, max_distance: usize) -> Option<usize> {
    distance(a, b, max_distance)
}

fn distance(a: &str, b: &str, max_distance: usize) -> Option<usize> {
    let mut long: Vec<char> = a.chars().collect();
    let mut short: Vec<char> = b.chars().collect();
    // La fila mide el largo de la mas corta.
    if short.len() > long.len() {
        std::mem::swap(&mut long, &mut short);
    }
    // La diferencia de longitudes es cota inferior: hacen falta al menos esos borrados.
    if long.len() - short.len() > max_distance {
        return None;
    }
    let mut previous: Vec<usize> = (0..=short.len()).collect();
    let mut current = vec![0; short.len() + 1];
    for (i, &ch_long) in long.iter().enumerate() {
        current[0] = i + 1;
        let mut row_min = current[0];
        for (j, &ch_short) in short.iter().enumerate() {
            let substitute = previous[j] + usize::from(ch_long != ch_short);
            let delete = previous[j + 1] + 1;
            let insert = current[j] + 1;
            current[j + 1] = substitute.min(delete).min(insert);
            row_min = row_min.min(current[j + 1]);
        }
        // Los valores de una fila nunca bajan en las siguientes: si toda la fila pasa el
        // tope, la distancia final tambien.
        if row_min > max_distance {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let result = previous[short.len()];
    (result <= max_distance).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::{edit_distance, edit_distance_within};

    #[test]
    fn caso_clasico() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn iguales_y_vacias() {
        assert_eq!(edit_distance("hola", "hola"), 0);
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("", "abcde"), 5);
        assert_eq!(edit_distance("abcde", ""), 5);
    }

    #[test]
    fn cuenta_caracteres_no_bytes() {
        assert_eq!(edit_distance("añb", "anb"), 1);
        assert_eq!(edit_distance("日本語", "日本"), 1);
    }

    #[test]
    fn es_simetrica() {
        let words = ["", "a", "casa", "cosas", "perro", "pera", "ñandú"];
        for a in words {
            for b in words {
                assert_eq!(edit_distance(a, b), edit_distance(b, a), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn cumple_desigualdad_triangular() {
        let words = ["", "a", "casa", "cosas", "perro", "pera"];
        for a in words {
            for b in words {
                for c in words {
                    assert!(edit_distance(a, c) <= edit_distance(a, b) + edit_distance(b, c));
                }
            }
        }
    }

    #[test]
    fn el_tope_corta_sin_cambiar_el_resultado() {
        let words = [
            "", "a", "casa", "cosas", "perro", "pera", "kitten", "sitting",
        ];
        for a in words {
            for b in words {
                let exact = edit_distance(a, b);
                for max in 0..8 {
                    let expected = (exact <= max).then_some(exact);
                    assert_eq!(
                        edit_distance_within(a, b, max),
                        expected,
                        "{a:?} {b:?} {max}"
                    );
                }
            }
        }
    }
}
