//! Deduplicacion por similitud: agrupa las filas de una columna de texto que son casi iguales.
//!
//! # El resultado
//! Dos filas estan **cerca** si su [distancia de edicion](crate::edit) es a lo sumo
//! `max_distance`. Un grupo es una **componente conexa** de la relacion «estar cerca»: si `a`
//! esta cerca de `b` y `b` de `c`, los tres van juntos aunque `a` y `c` no lo esten. Es la
//! regla mas simple de explicar y no depende del orden de las filas. Las consecuencias:
//! una cadena puede arrastrar a otra muy distinta por una cadena intermedia, y con un umbral
//! alto todo termina en un solo grupo.
//!
//! Los grupos se numeran desde 0 en el orden en que aparece su primera fila. Una fila nula no
//! pertenece a ningun grupo (`None`) y no cuenta en el total.
//!
//! # Como evita comparar todos contra todos
//! 1. Las cadenas **identicas** se juntan primero (ordenando): despues solo se comparan las
//!    distintas.
//! 2. Con `max_distance` 1 o 2 se usa un indice de **borrados** (la idea de SymSpell): si dos
//!    cadenas estan a distancia `k`, borrando a lo sumo `k` caracteres de cada una se llega a
//!    una cadena en comun (una sustitucion es un borrado de cada lado; una insercion, uno de un
//!    solo lado). Se guarda el hash de cada variante con borrados de cada cadena, se ordena, y
//!    solo se comparan de verdad las cadenas que comparten una variante. Un choque de hash solo
//!    agrega un candidato mas, que la comparacion exacta descarta: el resultado es exacto.
//! 3. Con `max_distance` mayor que 2 el indice crece demasiado: se ordenan por longitud y solo
//!    se comparan las que difieren en a lo sumo `max_distance` caracteres (la diferencia de
//!    longitudes ya es una cota inferior de la distancia). Es exacto pero mas lento.
//!
//! Con `max_distance` 0 solo se agrupan las identicas, sin indice.
//!
//! # Peor caso
//! Con muchas cadenas cortisimas (de uno o dos caracteres) casi todas comparten alguna
//! variante, y la comparacion dentro de cada coincidencia es cuadratica. Los nombres,
//! direcciones y otro texto real no caen ahi.

use std::cmp::Ordering;

use geoda_core::{Column, DataType, Error};

use crate::components::UnionFind;
use crate::edit::edit_distance_within;
use crate::sort::merge_sort;

/// Tope de `max_distance` con indice de borrados; mas alla se compara por ventana de longitud.
pub const MAX_INDEXED_DISTANCE: usize = 2;

/// Resultado de [`dedupe_similar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dedupe {
    /// Grupo de cada fila (desde 0, por primera aparicion); `None` en las filas nulas.
    pub ids: Vec<Option<usize>>,
    /// Cuantos grupos hay.
    pub count: usize,
}

/// Agrupa las filas de `rows` (`None` = nulo) cuya distancia de edicion es a lo sumo
/// `max_distance`, con la regla de componentes conexas de la documentacion del modulo.
///
/// # Errors
/// [`Error::OutOfBounds`] si hay mas de `u32::MAX` cadenas distintas; [`Error::Alloc`] si no hay
/// memoria para el union-find.
pub fn dedupe_similar(rows: &[Option<&str>], max_distance: usize) -> Result<Dedupe, Error> {
    // 1. Cadenas distintas: ordenar las filas no nulas y cortar donde cambia el texto.
    let mut order: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].is_some()).collect();
    merge_sort(&mut order, |&a, &b| rows[a].cmp(&rows[b]));
    let mut texts: Vec<&str> = Vec::new();
    let mut distinct_of_row: Vec<Option<usize>> = vec![None; rows.len()];
    for &row in &order {
        let text = rows[row].unwrap_or_default();
        if texts.last() != Some(&text) {
            texts.push(text);
        }
        distinct_of_row[row] = Some(texts.len() - 1);
    }

    // 2. Unir las distintas que estan cerca.
    let mut sets = UnionFind::new(texts.len())?;
    if max_distance > 0 && texts.len() > 1 {
        let chars: Vec<Vec<char>> = texts.iter().map(|t| t.chars().collect()).collect();
        if max_distance <= MAX_INDEXED_DISTANCE {
            join_by_deletions(&texts, &chars, max_distance, &mut sets)?;
        } else {
            join_by_length_window(&texts, &chars, max_distance, &mut sets)?;
        }
    }

    // 3. Numerar los grupos por la primera fila en que aparecen.
    let mut group_of_root: Vec<Option<usize>> = vec![None; texts.len()];
    let mut count = 0;
    let mut ids = Vec::with_capacity(rows.len());
    for distinct in distinct_of_row {
        ids.push(match distinct {
            None => None,
            Some(d) => {
                let root = sets.find(d)?;
                Some(*group_of_root[root].get_or_insert_with(|| {
                    count += 1;
                    count - 1
                }))
            }
        });
    }
    Ok(Dedupe { ids, count })
}

/// Como [`dedupe_similar`] sobre una columna `Utf8`.
///
/// # Errors
/// [`Error::TypeMismatch`] si la columna no es `Utf8`; los de [`dedupe_similar`].
pub fn dedupe_column(column: &Column, max_distance: usize) -> Result<Dedupe, Error> {
    if column.dtype() != DataType::Utf8 {
        return Err(Error::TypeMismatch(format!(
            "la deduplicacion pide Utf8, no {:?}",
            column.dtype()
        )));
    }
    let rows = (0..column.len())
        .map(|i| {
            if column.is_null(i)? {
                Ok(None)
            } else {
                column.str_value(i).map(Some)
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    dedupe_similar(&rows, max_distance)
}

/// Une los pares que comparten una variante con borrados y estan realmente cerca.
fn join_by_deletions(
    texts: &[&str],
    chars: &[Vec<char>],
    max_distance: usize,
    sets: &mut UnionFind,
) -> Result<(), Error> {
    // (hash de la variante, cadena); el id cabe en u32 porque `UnionFind::new` ya lo comprobo.
    let mut entries: Vec<(u64, u32)> = Vec::new();
    for (id, text) in chars.iter().enumerate() {
        let id = u32::try_from(id).map_err(|_| Error::OutOfBounds("demasiadas cadenas".into()))?;
        entries.push((hash_without(text, usize::MAX, usize::MAX), id));
        for first in 0..text.len() {
            entries.push((hash_without(text, first, usize::MAX), id));
            if max_distance >= 2 {
                for second in first + 1..text.len() {
                    entries.push((hash_without(text, first, second), id));
                }
            }
        }
    }
    let by_hash_then_id = |a: &(u64, u32), b: &(u64, u32)| -> Ordering { a.cmp(b) };
    merge_sort(&mut entries, by_hash_then_id);
    // Una cadena puede producir la misma variante varias veces (borrar una letra repetida).
    entries.dedup();

    let mut start = 0;
    while start < entries.len() {
        let mut end = start + 1;
        while end < entries.len() && entries[end].0 == entries[start].0 {
            end += 1;
        }
        let run = &entries[start..end];
        for (i, &(_, a)) in run.iter().enumerate() {
            for &(_, b) in &run[i + 1..] {
                let (a, b) = (a as usize, b as usize);
                if sets.find(a)? != sets.find(b)?
                    && edit_distance_within(texts[a], texts[b], max_distance).is_some()
                {
                    sets.union(a, b)?;
                }
            }
        }
        start = end;
    }
    Ok(())
}

/// Une los pares cuya longitud difiere a lo sumo `max_distance` y estan realmente cerca.
fn join_by_length_window(
    texts: &[&str],
    chars: &[Vec<char>],
    max_distance: usize,
    sets: &mut UnionFind,
) -> Result<(), Error> {
    let mut by_length: Vec<usize> = (0..texts.len()).collect();
    merge_sort(&mut by_length, |&a, &b| chars[a].len().cmp(&chars[b].len()));
    for (position, &a) in by_length.iter().enumerate() {
        for &b in &by_length[position + 1..] {
            if chars[b].len() - chars[a].len() > max_distance {
                break;
            }
            if sets.find(a)? != sets.find(b)?
                && edit_distance_within(texts[a], texts[b], max_distance).is_some()
            {
                sets.union(a, b)?;
            }
        }
    }
    Ok(())
}

/// Hash de `text` sin los caracteres en las posiciones `skip_a` y `skip_b` (`usize::MAX` = ninguna).
fn hash_without(text: &[char], skip_a: usize, skip_b: usize) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, &ch) in text.iter().enumerate() {
        if i != skip_a && i != skip_b {
            hash ^= u64::from(u32::from(ch));
            hash = hash.wrapping_mul(PRIME);
        }
    }
    crate::hash::mix64(hash)
}

#[cfg(test)]
mod tests {
    use super::{Dedupe, dedupe_column, dedupe_similar};
    use crate::edit::edit_distance;
    use geoda_core::{Error, PrimitiveBuilder, Utf8Builder};

    fn some<'a>(values: &[&'a str]) -> Vec<Option<&'a str>> {
        values.iter().map(|v| Some(*v)).collect()
    }

    fn root(parent: &mut [usize], x: usize) -> usize {
        let mut x = x;
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    /// Oraculo: todos contra todos con union-find ingenuo.
    fn brute(rows: &[Option<&str>], max_distance: usize) -> Dedupe {
        let n = rows.len();
        let mut parent: Vec<usize> = (0..n).collect();
        for a in 0..n {
            for b in a + 1..n {
                if let (Some(x), Some(y)) = (rows[a], rows[b])
                    && edit_distance(x, y) <= max_distance
                {
                    let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        let mut group_of_root = vec![None; n];
        let mut count = 0;
        let mut ids = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            ids.push(row.map(|_| {
                let r = root(&mut parent, i);
                *group_of_root[r].get_or_insert_with(|| {
                    count += 1;
                    count - 1
                })
            }));
        }
        Dedupe { ids, count }
    }

    #[test]
    fn ejemplo_de_la_historia() {
        let rows = some(&["Juan Perez", "Juan Pérez", "Ana"]);
        let result = dedupe_similar(&rows, 1).unwrap();
        assert_eq!(result.ids, [Some(0), Some(0), Some(1)]);
        assert_eq!(result.count, 2);
    }

    #[test]
    fn umbral_cero_agrupa_solo_identicas() {
        let rows = some(&["a", "b", "a", "ab", "b"]);
        let result = dedupe_similar(&rows, 0).unwrap();
        assert_eq!(result.ids, [Some(0), Some(1), Some(0), Some(2), Some(1)]);
        assert_eq!(result.count, 3);
    }

    #[test]
    fn los_nulos_no_forman_grupo() {
        let rows = vec![Some("casa"), None, Some("cosa"), None];
        let result = dedupe_similar(&rows, 1).unwrap();
        assert_eq!(result.ids, [Some(0), None, Some(0), None]);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn la_relacion_no_transitiva_se_cierra_por_componentes() {
        // aaa~aab y aab~abb, pero aaa y abb estan a 2.
        let rows = some(&["aaa", "abb", "aab"]);
        for k in 1..=2 {
            let result = dedupe_similar(&rows, k).unwrap();
            assert_eq!(result.count, 1, "k = {k}");
        }
        assert_eq!(
            dedupe_similar(&rows, 1).unwrap().ids,
            [Some(0), Some(0), Some(0)]
        );
    }

    #[test]
    fn el_resultado_no_depende_de_la_corrida() {
        let rows = some(&["perro", "pero", "gato", "gata", "pera", "pez"]);
        let first = dedupe_similar(&rows, 1).unwrap();
        for _ in 0..5 {
            assert_eq!(dedupe_similar(&rows, 1).unwrap(), first);
        }
    }

    #[test]
    fn coincide_con_todos_contra_todos() {
        let mut state: u64 = 0xDED0;
        let mut next = move |bound: u64| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) % bound
        };
        for _ in 0..60 {
            let owned: Vec<Option<String>> = (0..30)
                .map(|_| {
                    if next(10) == 0 {
                        None
                    } else {
                        let len = next(7);
                        Some(
                            (0..len)
                                .map(|_| char::from(b'a' + u8::try_from(next(3)).unwrap()))
                                .collect(),
                        )
                    }
                })
                .collect();
            let rows: Vec<Option<&str>> = owned.iter().map(Option::as_deref).collect();
            for k in 0..=4 {
                assert_eq!(
                    dedupe_similar(&rows, k).unwrap(),
                    brute(&rows, k),
                    "k = {k}, filas {rows:?}"
                );
            }
        }
    }

    #[test]
    fn por_caracteres_unicode() {
        let rows = some(&["ñandú", "nandu", "ñandu"]);
        assert_eq!(dedupe_similar(&rows, 1).unwrap().count, 1);
        assert_eq!(dedupe_similar(&rows, 0).unwrap().count, 3);
    }

    #[test]
    fn columna_utf8_y_otros_tipos() {
        let mut builder = Utf8Builder::new();
        builder.append("casa").unwrap();
        builder.append_null();
        builder.append("cosa").unwrap();
        let column = builder.finish().unwrap();
        let result = dedupe_column(&column, 1).unwrap();
        assert_eq!(result.ids, [Some(0), None, Some(0)]);
        let mut ints = PrimitiveBuilder::<i64>::new();
        ints.append(1);
        assert!(matches!(
            dedupe_column(&ints.finish().unwrap(), 1),
            Err(Error::TypeMismatch(_))
        ));
    }

    #[test]
    fn vacia() {
        let result = dedupe_similar(&[], 1).unwrap();
        assert_eq!((result.ids.len(), result.count), (0, 0));
    }
}
