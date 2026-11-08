//! Subsecuencia creciente mas larga (LIS, *longest increasing subsequence*).
//!
//! # La idea
//! Se busca el mayor numero de elementos que, tomados en el orden en que aparecen (sin
//! tener que ser vecinos), crecen estrictamente. Sobre `[10, 9, 2, 5, 3, 7, 101, 18]` una
//! LIS es `[2, 5, 7, 101]`: longitud 4.
//!
//! El truco es mantener `tails`, donde `tails[k]` es el **menor final posible** de una
//! subsecuencia creciente de longitud `k + 1` entre lo visto hasta ahora. `tails` siempre esta
//! ordenado, asi que cada elemento nuevo se coloca con una busqueda binaria
//! ([`lower_bound`](crate::search::lower_bound)): si es mayor que todos, alarga la LIS; si no,
//! baja el final de la primera longitud que no lo supera. Son O(n log n) en total, frente a
//! O(n²) de la programacion dinamica directa.
//!
//! # Que subsecuencia se devuelve
//! Cada elemento recuerda su predecesor (el final de longitud `k - 1` en el momento de
//! colocarlo) y la LIS se reconstruye hacia atras desde el ultimo elemento de `tails`. Con
//! varias LIS de la misma longitud el resultado es siempre el mismo (el algoritmo no tiene
//! azar), pero no es necesariamente la de indices mas chicos: es la que construye este
//! procedimiento.
//!
//! # Estricta y sin `NaN`
//! La subsecuencia es **estrictamente** creciente: los repetidos no se encadenan. Un valor
//! que no se compara consigo mismo (`NaN`) no tiene lugar en un orden, asi que se rechaza con
//! [`Error::InvalidArgument`] en vez de dar un resultado arbitrario.

use std::cmp::Ordering;

use geoda_core::Error;

use crate::search::lower_bound;

/// Resultado de [`lis`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lis {
    /// Largo de la subsecuencia.
    pub length: usize,
    /// Posiciones, en la entrada, de los elementos de la subsecuencia, de menor a mayor.
    pub indices: Vec<usize>,
}

/// Subsecuencia estrictamente creciente mas larga de `data`.
///
/// # Errors
/// [`Error::InvalidArgument`] si algun elemento no es comparable consigo mismo (`NaN`).
pub fn lis<T: Copy + PartialOrd>(data: &[T]) -> Result<Lis, Error> {
    if let Some(position) = data.iter().position(|x| x.partial_cmp(x).is_none()) {
        return Err(Error::InvalidArgument(format!(
            "el elemento {position} no es comparable (NaN)"
        )));
    }
    // `NaN` ya esta descartado, asi que `partial_cmp` siempre responde.
    let cmp = |a: &T, b: &T| a.partial_cmp(b).unwrap_or(Ordering::Equal);
    // tails[k]: menor final de una subsecuencia de longitud k + 1; tail_at[k]: su posicion.
    let mut tails: Vec<T> = Vec::new();
    let mut tail_at: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = Vec::with_capacity(data.len());
    for (i, value) in data.iter().enumerate() {
        let slot = lower_bound(&tails, value, cmp);
        previous.push(slot.checked_sub(1).map(|k| tail_at[k]));
        if slot == tails.len() {
            tails.push(*value);
            tail_at.push(i);
        } else {
            tails[slot] = *value;
            tail_at[slot] = i;
        }
    }
    let mut indices = Vec::with_capacity(tails.len());
    let mut cursor = tail_at.last().copied();
    while let Some(i) = cursor {
        indices.push(i);
        cursor = previous[i];
    }
    indices.reverse();
    Ok(Lis {
        length: indices.len(),
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::lis;
    use geoda_core::Error;

    /// Longitud por programacion dinamica O(n²): el oraculo del test.
    fn brute_length(data: &[i64]) -> usize {
        let mut best = vec![1usize; data.len()];
        for i in 0..data.len() {
            for j in 0..i {
                if data[j] < data[i] {
                    best[i] = best[i].max(best[j] + 1);
                }
            }
        }
        best.into_iter().max().unwrap_or(0)
    }

    #[test]
    fn caso_clasico() {
        let result = lis(&[10, 9, 2, 5, 3, 7, 101, 18]).unwrap();
        assert_eq!(result.length, 4);
        assert_eq!(result.indices.len(), 4);
    }

    #[test]
    fn vacia_y_decreciente() {
        let empty: [i64; 0] = [];
        assert_eq!(lis(&empty).unwrap().length, 0);
        assert_eq!(lis(&[5, 4, 3, 2, 1]).unwrap().length, 1);
    }

    #[test]
    fn los_repetidos_no_se_encadenan() {
        assert_eq!(lis(&[2, 2, 2, 2]).unwrap().length, 1);
        assert_eq!(lis(&[1, 2, 2, 3]).unwrap().length, 3);
    }

    #[test]
    fn el_desempate_es_estable() {
        // Dos LIS de longitud 2 con distintos indices; la corrida siempre da la misma.
        let data = [1, 3, 2, 4];
        let first = lis(&data).unwrap();
        for _ in 0..5 {
            assert_eq!(lis(&data).unwrap(), first);
        }
        assert_eq!(first.length, 3);
    }

    #[test]
    fn nan_es_invalid_argument() {
        assert!(matches!(
            lis(&[1.0, f64::NAN, 3.0]),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn flotantes_y_cero_negativo() {
        let result = lis(&[-0.0, 0.0, 1.5]).unwrap();
        // -0.0 == 0.0: no son estrictamente crecientes entre si.
        assert_eq!(result.length, 2);
    }

    #[test]
    fn coincide_con_la_programacion_dinamica_y_los_indices_son_validos() {
        let mut state: u64 = 0x5EED;
        for len in 0..120 {
            let data: Vec<i64> = (0..len)
                .map(|_| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    i64::try_from((state >> 33) % 30).unwrap()
                })
                .collect();
            let result = lis(&data).unwrap();
            assert_eq!(result.length, brute_length(&data), "datos {data:?}");
            assert!(result.indices.windows(2).all(|w| w[0] < w[1]));
            assert!(
                result.indices.windows(2).all(|w| data[w[0]] < data[w[1]]),
                "no es creciente: {data:?}"
            );
        }
    }
}
