//! Busqueda binaria en datos ya ordenados.
//!
//! # La idea
//! Si la lista esta ordenada, para saber donde cae un valor no hace falta recorrerla:
//! se mira el elemento del medio. Si es menor que el buscado, todo lo anterior tambien
//! lo es y se descarta la mitad izquierda; si no, se descarta la derecha. Cada paso
//! parte la zona de busqueda por la mitad, asi que bastan log2(n) pasos.
//!
//! Se devuelven **posiciones de insercion**, no un «encontrado / no encontrado»:
//!
//! - [`lower_bound`]: la primera posicion cuyo valor es `>=` el buscado.
//! - [`upper_bound`]: la primera posicion cuyo valor es `>` el buscado.
//!
//! La diferencia entre ambas es cuantas veces aparece el valor. Si no aparece, valen
//! lo mismo y apuntan a donde habria que insertarlo para mantener el orden. Sobre
//! `[1, 3, 3, 5]` y el valor 3, valen 1 y 3.
//!
//! **Precondicion:** los datos estan ordenados de menor a mayor segun el mismo orden
//! que se pasa en `cmp`. Comprobarlo costaria O(n) y anularia el beneficio, asi que
//! con datos desordenados el resultado es una posicion cualquiera (sin panic).

use std::cmp::Ordering;

use geoda_core::memory::Native;
use geoda_core::{Column, DataType, Error};

use crate::select::Scalar;

/// Primera posicion `i` tal que `keeps_going(&data[i])` es falso, suponiendo que es
/// verdadero en un prefijo de `data` y falso despues.
fn first_false<T>(data: &[T], keeps_going: impl Fn(&T) -> bool) -> usize {
    let (mut lo, mut hi) = (0, data.len());
    // Invariante: lo anterior a `lo` cumple el predicado y desde `hi` no.
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if keeps_going(&data[mid]) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Primera posicion de `data` (ordenado segun `cmp`) cuyo valor no es menor que `target`.
/// Devuelve `data.len()` si todos son menores y `0` si `data` esta vacio.
pub fn lower_bound<T>(data: &[T], target: &T, cmp: impl Fn(&T, &T) -> Ordering) -> usize {
    first_false(data, |x| cmp(x, target) == Ordering::Less)
}

/// Primera posicion de `data` (ordenado segun `cmp`) cuyo valor es mayor que `target`.
/// Devuelve `data.len()` si ninguno lo es y `0` si `data` esta vacio.
pub fn upper_bound<T>(data: &[T], target: &T, cmp: impl Fn(&T, &T) -> Ordering) -> usize {
    first_false(data, |x| cmp(x, target) != Ordering::Greater)
}

/// Que cota se pide.
#[derive(Clone, Copy)]
enum Bound {
    Lower,
    Upper,
}

/// [`lower_bound`] sobre una columna `Int32`, `Int64` o `Float64` ordenada ascendente
/// con los nulos al final (como la deja [`crate::sort_column`], decision F1).
///
/// Los nulos no participan. En `Float64` el orden es el total de [`f64::total_cmp`]
/// (decision F2). La posicion es sobre la columna completa; como los nulos van al
/// final, nunca apunta mas alla del tramo valido.
///
/// # Errors
/// - [`Error::TypeMismatch`] si `target` no es del tipo de la columna.
/// - [`Error::InvalidArgument`] si los nulos no estan al final (solo se comprueba la
///   frontera entre validos y nulos; el orden del tramo valido es precondicion).
/// - [`Error::Unsupported`] para `Bool` y `Utf8`.
pub fn lower_bound_column(column: &Column, target: Scalar) -> Result<usize, Error> {
    column_bound(column, target, Bound::Lower)
}

/// [`upper_bound`] sobre una columna; mismas reglas y errores que [`lower_bound_column`].
///
/// # Errors
/// Los mismos que [`lower_bound_column`].
pub fn upper_bound_column(column: &Column, target: Scalar) -> Result<usize, Error> {
    column_bound(column, target, Bound::Upper)
}

fn column_bound(column: &Column, target: Scalar, which: Bound) -> Result<usize, Error> {
    match (column.dtype(), target) {
        (DataType::Int32, Scalar::Int32(t)) => primitive_bound(column, t, which, i32::cmp),
        (DataType::Int64, Scalar::Int64(t)) => primitive_bound(column, t, which, i64::cmp),
        (DataType::Float64, Scalar::Float64(t)) => {
            primitive_bound(column, t, which, f64::total_cmp)
        }
        (DataType::Bool | DataType::Utf8, _) => Err(Error::Unsupported(format!(
            "buscar en columnas {:?} aun no esta soportado",
            column.dtype()
        ))),
        (dtype, target) => Err(Error::TypeMismatch(format!(
            "se busca {target:?} en una columna {dtype:?}"
        ))),
    }
}

fn primitive_bound<T: Native>(
    column: &Column,
    target: T,
    which: Bound,
    cmp: impl Fn(&T, &T) -> Ordering,
) -> Result<usize, Error> {
    let values = column.values::<T>()?;
    let valid = values.len() - column.null_count();
    // Frontera: el ultimo valido y el primer nulo (si existen) deben estar donde toca.
    let misplaced = (valid > 0 && column.is_null(valid - 1)?)
        || (valid < values.len() && !column.is_null(valid)?);
    if misplaced {
        return Err(Error::InvalidArgument(
            "los nulos deben estar al final de la columna".to_string(),
        ));
    }
    let data = &values[..valid];
    Ok(match which {
        Bound::Lower => lower_bound(data, &target, cmp),
        Bound::Upper => upper_bound(data, &target, cmp),
    })
}

#[cfg(test)]
mod tests {
    use geoda_core::{BoolBuilder, PrimitiveBuilder};

    use super::*;
    use crate::sort::sort_column;

    /// Generador pseudoaleatorio con semilla fija (xorshift64), igual que en `select`.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

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

    #[test]
    fn documented_example_with_repeated_values() {
        let data = [1_i64, 3, 3, 5];
        assert_eq!(lower_bound(&data, &3, i64::cmp), 1);
        assert_eq!(upper_bound(&data, &3, i64::cmp), 3);
        // Ausente: las dos cotas coinciden, en el hueco donde se insertaria.
        assert_eq!(lower_bound(&data, &4, i64::cmp), 3);
        assert_eq!(upper_bound(&data, &4, i64::cmp), 3);
    }

    #[test]
    fn values_below_all_or_above_all_and_empty_input() {
        let data = [10_i64, 20, 30];
        for bound in [lower_bound::<i64>, upper_bound::<i64>] {
            assert_eq!(bound(&data, &5, i64::cmp), 0);
            assert_eq!(bound(&data, &99, i64::cmp), 3);
            assert_eq!(bound(&[], &7, i64::cmp), 0);
        }
    }

    #[test]
    fn matches_partition_point_on_1000_seeded_inputs() {
        let mut rng = Rng(0x5EA2_C014);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(40)).unwrap();
            let range = [4, 1_000][case % 2];
            let mut data: Vec<i64> = (0..len)
                .map(|_| i64::try_from(rng.below(range)).unwrap())
                .collect();
            data.sort_unstable();
            // Tambien objetivos fuera del rango de los datos.
            let target = i64::try_from(rng.below(range + 2)).unwrap() - 1;
            assert_eq!(
                lower_bound(&data, &target, i64::cmp),
                data.partition_point(|&x| x < target),
                "lower, caso {case}, objetivo {target}: {data:?}"
            );
            assert_eq!(
                upper_bound(&data, &target, i64::cmp),
                data.partition_point(|&x| x <= target),
                "upper, caso {case}, objetivo {target}: {data:?}"
            );
        }
    }

    #[test]
    fn nulls_at_the_end_do_not_take_part() {
        let column = sort_column(&int64_column(&[
            Some(5),
            None,
            Some(1),
            Some(3),
            None,
            Some(3),
        ]))
        .unwrap(); // [1, 3, 3, 5, NULL, NULL]
        assert_eq!(lower_bound_column(&column, Scalar::Int64(3)).unwrap(), 1);
        assert_eq!(upper_bound_column(&column, Scalar::Int64(3)).unwrap(), 3);
        // Mayor que todo: la posicion es el final del tramo valido, no de la columna.
        assert_eq!(lower_bound_column(&column, Scalar::Int64(99)).unwrap(), 4);
        assert_eq!(upper_bound_column(&column, Scalar::Int64(99)).unwrap(), 4);
    }

    #[test]
    fn empty_and_all_null_columns() {
        for column in [int64_column(&[]), int64_column(&[None, None])] {
            assert_eq!(lower_bound_column(&column, Scalar::Int64(1)).unwrap(), 0);
            assert_eq!(upper_bound_column(&column, Scalar::Int64(1)).unwrap(), 0);
        }
    }

    #[test]
    fn float64_uses_the_total_order() {
        let mut builder = PrimitiveBuilder::<f64>::new();
        for value in [f64::NAN, 2.5, -0.0, 0.0, f64::NEG_INFINITY] {
            builder.append(value);
        }
        // Orden total: -inf, -0.0, 0.0, 2.5, NaN.
        let column = sort_column(&builder.finish().unwrap()).unwrap();
        let lower = |t: f64| lower_bound_column(&column, Scalar::Float64(t)).unwrap();
        let upper = |t: f64| upper_bound_column(&column, Scalar::Float64(t)).unwrap();
        assert_eq!((lower(-0.0), upper(-0.0)), (1, 2));
        assert_eq!((lower(0.0), upper(0.0)), (2, 3));
        assert_eq!((lower(f64::NAN), upper(f64::NAN)), (4, 5));
        assert_eq!(lower(f64::INFINITY), 4);
    }

    #[test]
    fn int32_and_wrong_target_type() {
        let mut ints = PrimitiveBuilder::<i32>::new();
        for value in [1, 4, 4, 9] {
            ints.append(value);
        }
        let ints = ints.finish().unwrap();
        assert_eq!(lower_bound_column(&ints, Scalar::Int32(4)).unwrap(), 1);
        assert!(matches!(
            lower_bound_column(&ints, Scalar::Int64(4)),
            Err(Error::TypeMismatch(_))
        ));
        assert!(matches!(
            upper_bound_column(&ints, Scalar::Float64(4.0)),
            Err(Error::TypeMismatch(_))
        ));
    }

    #[test]
    fn nulls_not_at_the_end_are_rejected() {
        for values in [vec![None, Some(1), Some(2)], vec![Some(1), None, Some(2)]] {
            assert!(matches!(
                lower_bound_column(&int64_column(&values), Scalar::Int64(1)),
                Err(Error::InvalidArgument(_))
            ));
        }
    }

    #[test]
    fn bool_is_unsupported_and_sliced_columns_work() {
        let mut bools = BoolBuilder::new();
        bools.append(true);
        assert!(matches!(
            lower_bound_column(&bools.finish().unwrap(), Scalar::Int64(1)),
            Err(Error::Unsupported(_))
        ));

        let column = int64_column(&[Some(0), Some(2), Some(4), Some(6), None]);
        let window = column.slice(1, 4).unwrap(); // [2, 4, 6, NULL]
        assert_eq!(lower_bound_column(&window, Scalar::Int64(4)).unwrap(), 1);
        assert_eq!(upper_bound_column(&window, Scalar::Int64(6)).unwrap(), 3);
    }
}
