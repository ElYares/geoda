//! Ordenacion: un mergesort estable propio y su aplicacion a columnas.
//!
//! # Que es un mergesort
//! Para ordenar una lista se parte por la mitad, se ordena cada mitad (con la misma
//! receta, hasta llegar a listas de 0 o 1 elementos, que ya estan ordenadas) y se
//! **mezclan** las dos mitades ya ordenadas: se mira el primero de cada una, se toma
//! el menor y se repite. Cada nivel de la recursion mezcla `n` elementos en total y
//! hay `log2(n)` niveles, asi que cuesta O(n log n) siempre, incluso en el peor caso.
//! Es **estable**: si dos elementos son iguales, conservan su orden original.

use std::cmp::Ordering;

use geoda_core::memory::Native;
use geoda_core::{Column, DataType, Error, PrimitiveBuilder};

/// Ordena `data` de menor a mayor segun `cmp`, de forma estable, en O(n log n).
///
/// `cmp` debe ser un orden total (transitivo y consistente); con uno inconsistente
/// el resultado queda en un orden cualquiera, pero no hay panic ni memoria sin
/// inicializar.
pub fn merge_sort<T: Copy>(data: &mut [T], cmp: impl Fn(&T, &T) -> Ordering) {
    // Un solo buffer auxiliar para todo el algoritmo, en vez de uno por mezcla.
    let mut buffer = data.to_vec();
    sort_range(data, &mut buffer, &cmp);
}

/// Ordena `data` usando `buffer` (mismo largo) como espacio de mezcla.
fn sort_range<T: Copy>(data: &mut [T], buffer: &mut [T], cmp: &impl Fn(&T, &T) -> Ordering) {
    let len = data.len();
    if len <= 1 {
        return;
    }
    let mid = len / 2;
    {
        let (left, right) = data.split_at_mut(mid);
        let (buffer_left, buffer_right) = buffer.split_at_mut(mid);
        sort_range(left, buffer_left, cmp);
        sort_range(right, buffer_right, cmp);
    }
    merge(data, mid, buffer, cmp);
}

/// Mezcla `data[..mid]` y `data[mid..]` (cada mitad ya ordenada) dejando `data`
/// ordenado. Ante un empate toma de la izquierda: eso la hace estable.
fn merge<T: Copy>(data: &mut [T], mid: usize, buffer: &mut [T], cmp: &impl Fn(&T, &T) -> Ordering) {
    let (mut left, mut right) = (0, mid);
    for slot in buffer.iter_mut() {
        let take_left = if left == mid {
            false
        } else if right == data.len() {
            true
        } else {
            cmp(&data[right], &data[left]) != Ordering::Less
        };
        if take_left {
            *slot = data[left];
            left += 1;
        } else {
            *slot = data[right];
            right += 1;
        }
    }
    data.copy_from_slice(buffer);
}

/// Ordena de menor a mayor una columna `Int32`, `Int64` o `Float64`.
///
/// Devuelve una columna **nueva**; la original no cambia. Los nulos quedan al final
/// (decision F1) y conservan su cantidad. En `Float64` el orden es el total de
/// [`f64::total_cmp`] (decision F2): `-0.0` antes de `0.0` y `NaN` al final de los
/// valores validos, antes de los nulos.
///
/// # Errors
/// [`Error::Unsupported`] para `Bool` y `Utf8`.
pub fn sort_column(column: &Column) -> Result<Column, Error> {
    match column.dtype() {
        DataType::Int32 => sort_primitive::<i32>(column, i32::cmp),
        DataType::Int64 => sort_primitive::<i64>(column, i64::cmp),
        DataType::Float64 => sort_primitive::<f64>(column, f64::total_cmp),
        other => Err(Error::Unsupported(format!(
            "ordenar columnas {other:?} aun no esta soportado"
        ))),
    }
}

/// Ordena una columna primitiva con el orden `cmp`: valores validos ordenados y,
/// despues, los nulos.
fn sort_primitive<T: Native>(
    column: &Column,
    cmp: impl Fn(&T, &T) -> Ordering,
) -> Result<Column, Error> {
    let values = column.values::<T>()?;
    let mut valid = Vec::with_capacity(values.len() - column.null_count());
    for (i, &value) in values.iter().enumerate() {
        if !column.is_null(i)? {
            valid.push(value);
        }
    }
    merge_sort(&mut valid, cmp);

    let mut builder = PrimitiveBuilder::<T>::new();
    for value in valid {
        builder.append(value);
    }
    for _ in 0..column.null_count() {
        builder.append_null();
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use geoda_core::{BoolBuilder, Utf8Builder};

    use super::*;

    /// Generador pseudoaleatorio con semilla fija (xorshift64): los tests son
    /// reproducibles y no hace falta ninguna dependencia.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        /// Entero en `0..bound`.
        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }
    }

    fn int64_column(values: &[Option<i64>]) -> Column {
        let mut builder = PrimitiveBuilder::<i64>::new();
        for value in values {
            match value {
                Some(v) => builder.append(*v),
                None => builder.append_null(),
            }
        }
        builder.finish().unwrap()
    }

    /// Los valores de una columna `Int64`, con `None` en los nulos.
    fn int64_values(column: &Column) -> Vec<Option<i64>> {
        let values = column.values::<i64>().unwrap();
        (0..column.len())
            .map(|i| (!column.is_null(i).unwrap()).then_some(values[i]))
            .collect()
    }

    #[test]
    fn merge_sort_handles_trivial_and_degenerate_inputs() {
        let cases: [&[i32]; 7] = [
            &[],
            &[7],
            &[2, 1],
            &[1, 2, 3, 4],
            &[4, 3, 2, 1],
            &[5, 5, 5, 5],
            &[3, 1, 2, 3, 1, 2],
        ];
        for case in cases {
            let mut data = case.to_vec();
            merge_sort(&mut data, i32::cmp);
            let mut oracle = case.to_vec();
            oracle.sort_unstable();
            assert_eq!(data, oracle, "entrada {case:?}");
        }
    }

    #[test]
    fn merge_sort_is_stable() {
        // Se ordena solo por la clave; los elementos con la misma clave deben
        // conservar el orden en que venian (el segundo campo es su posicion original).
        let mut rng = Rng(7);
        let mut data: Vec<(u64, usize)> = (0..200).map(|i| (rng.below(5), i)).collect();
        merge_sort(&mut data, |a, b| a.0.cmp(&b.0));
        for pair in data.windows(2) {
            assert!(pair[0].0 <= pair[1].0, "fuera de orden: {pair:?}");
            if pair[0].0 == pair[1].0 {
                assert!(pair[0].1 < pair[1].1, "no es estable: {pair:?}");
            }
        }
    }

    #[test]
    fn merge_sort_matches_the_oracle_on_1000_seeded_inputs() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(65)).unwrap();
            // Rango chico para que haya muchos repetidos.
            let range = [3, 50, 1_000_000][case % 3];
            let data: Vec<i64> = (0..len)
                .map(|_| i64::try_from(rng.below(range)).unwrap() - 1)
                .collect();
            let mut ours = data.clone();
            merge_sort(&mut ours, i64::cmp);
            let mut oracle = data.clone();
            oracle.sort_unstable();
            assert_eq!(ours, oracle, "caso {case}: {data:?}");
        }
    }

    #[test]
    fn columns_match_the_oracle_on_1000_seeded_inputs() {
        let mut rng = Rng(2026);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(40)).unwrap();
            // Vacias, de un elemento, con repetidos, con nulos: sale de la semilla.
            let values: Vec<Option<i64>> = (0..len)
                .map(|_| (rng.below(4) != 0).then(|| i64::try_from(rng.below(10)).unwrap() - 3))
                .collect();
            let sorted = sort_column(&int64_column(&values)).unwrap();

            let mut valid: Vec<i64> = values.iter().flatten().copied().collect();
            valid.sort_unstable();
            let nulls = values.len() - valid.len();
            let expected: Vec<Option<i64>> = valid
                .into_iter()
                .map(Some)
                .chain(std::iter::repeat_n(None, nulls))
                .collect();
            assert_eq!(int64_values(&sorted), expected, "caso {case}: {values:?}");
            assert_eq!(sorted.null_count(), nulls);
        }
    }

    #[test]
    fn sorts_an_int64_column_without_touching_the_original() {
        let original = int64_column(&[Some(3), Some(1), Some(2)]);
        let sorted = sort_column(&original).unwrap();
        assert_eq!(int64_values(&sorted), [Some(1), Some(2), Some(3)]);
        assert_eq!(int64_values(&original), [Some(3), Some(1), Some(2)]);
    }

    #[test]
    fn nulls_go_last_and_keep_their_count() {
        let column = int64_column(&[None, Some(5), Some(-1), None, Some(2)]);
        let sorted = sort_column(&column).unwrap();
        assert_eq!(sorted.null_count(), 2);
        assert_eq!(
            int64_values(&sorted),
            [Some(-1), Some(2), Some(5), None, None]
        );
    }

    #[test]
    fn empty_and_all_null_columns_are_fine() {
        let empty = sort_column(&int64_column(&[])).unwrap();
        assert_eq!(empty.len(), 0);
        let nulls = sort_column(&int64_column(&[None, None])).unwrap();
        assert_eq!((nulls.len(), nulls.null_count()), (2, 2));
    }

    #[test]
    fn float64_follows_the_total_order() {
        let mut builder = PrimitiveBuilder::<f64>::new();
        for value in [f64::NAN, 0.0, -0.0, 1.5, f64::NEG_INFINITY, f64::INFINITY] {
            builder.append(value);
        }
        builder.append_null();
        let sorted = sort_column(&builder.finish().unwrap()).unwrap();
        let values = sorted.values::<f64>().unwrap();
        // -inf, -0.0, 0.0, 1.5, +inf, NaN y por ultimo el nulo.
        assert_eq!(values[0].to_bits(), f64::NEG_INFINITY.to_bits());
        assert_eq!(values[1].to_bits(), (-0.0_f64).to_bits());
        assert_eq!(values[2].to_bits(), 0.0_f64.to_bits());
        assert_eq!(values[3].to_bits(), 1.5_f64.to_bits());
        assert_eq!(values[4].to_bits(), f64::INFINITY.to_bits());
        assert!(values[5].is_nan());
        assert!(sorted.is_null(6).unwrap());
        assert_eq!(sorted.null_count(), 1);
    }

    #[test]
    fn float64_matches_the_oracle_on_1000_seeded_inputs() {
        let specials = [f64::NAN, 0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, 1.0];
        let mut rng = Rng(42);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(40)).unwrap();
            let data: Vec<f64> = (0..len)
                .map(|_| {
                    if rng.below(3) == 0 {
                        specials[usize::try_from(rng.below(6)).unwrap()]
                    } else {
                        f64::from(i32::try_from(rng.below(1000)).unwrap()) / 8.0 - 60.0
                    }
                })
                .collect();
            let mut ours = data.clone();
            merge_sort(&mut ours, f64::total_cmp);
            let mut oracle = data.clone();
            oracle.sort_unstable_by(f64::total_cmp);
            let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&ours), bits(&oracle), "caso {case}");
        }
    }

    #[test]
    fn sorts_a_sliced_column() {
        let column = int64_column(&[Some(9), Some(4), None, Some(1), Some(7)]);
        let window = column.slice(1, 3).unwrap(); // [4, NULL, 1]
        let sorted = sort_column(&window).unwrap();
        assert_eq!(int64_values(&sorted), [Some(1), Some(4), None]);
    }

    #[test]
    fn int32_columns_are_supported() {
        let mut builder = PrimitiveBuilder::<i32>::new();
        for value in [30, -5, 12] {
            builder.append(value);
        }
        let sorted = sort_column(&builder.finish().unwrap()).unwrap();
        assert_eq!(sorted.values::<i32>().unwrap(), [-5, 12, 30]);
    }

    #[test]
    fn bool_and_utf8_are_unsupported() {
        let mut bools = BoolBuilder::new();
        bools.append(true);
        assert!(matches!(
            sort_column(&bools.finish().unwrap()),
            Err(Error::Unsupported(_))
        ));
        let mut strings = Utf8Builder::new();
        strings.append("a").unwrap();
        assert!(matches!(
            sort_column(&strings.finish().unwrap()),
            Err(Error::Unsupported(_))
        ));
    }
}
