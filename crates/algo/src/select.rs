//! Seleccion del k-esimo menor sin ordenar todo.
//!
//! # La idea
//! La mediana es «el valor que quedaria en medio si ordenaras». Pero para saber cual es
//! no hace falta ordenar todo: basta con ir descartando la mitad que no puede contenerlo.
//! Se elige un **pivote**, se reparte la lista en tres zonas (menores que el pivote,
//! iguales y mayores) y, segun donde caiga la posicion `k` que buscamos, se sigue solo
//! por una de las zonas. Cada vuelta descarta una parte de la lista, asi que el trabajo
//! total es una suma que decrece (n + n/2 + n/4 + ...) y vale O(n) en lugar de
//! O(n log n).
//!
//! Los dos algoritmos de este modulo se distinguen solo por **como eligen el pivote**:
//!
//! - [`quickselect`]: pivote = mediana de tres valores (primero, central, ultimo). Es muy
//!   simple y rapido; es lineal en promedio, pero existen entradas fabricadas a mano que
//!   lo hacen cuadratico (no basta con que la lista este ordenada o invertida).
//! - [`median_of_medians`]: pivote = mediana de las medianas de grupos de 5. Garantiza que
//!   cada vuelta descarta al menos ~30 % de la lista, asi que es lineal **siempre**, a
//!   cambio de mas trabajo por vuelta.
//!
//! La particion en tres zonas hace que los valores repetidos no degraden a ninguno.

use std::cmp::Ordering;

use geoda_core::memory::Native;
use geoda_core::{Column, DataType, Error};

/// Algoritmo de seleccion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// Pivote de mediana de tres; lineal en promedio.
    #[default]
    Quickselect,
    /// Pivote de mediana de medianas; lineal en el peor caso.
    MedianOfMedians,
}

/// Un valor suelto de una columna primitiva.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scalar {
    /// Valor `Int32`.
    Int32(i32),
    /// Valor `Int64`.
    Int64(i64),
    /// Valor `Float64`.
    Float64(f64),
}

fn out_of_bounds(k: usize, len: usize) -> Error {
    Error::OutOfBounds(format!("k = {k} sobre {len} valores"))
}

/// Reparte `data[lo..hi]` en tres zonas respecto de `pivot` y devuelve `(lt, gt)`:
/// `data[lo..lt]` son menores, `data[lt..gt]` iguales y `data[gt..hi]` mayores.
fn partition3<T: Copy>(
    data: &mut [T],
    (lo, hi): (usize, usize),
    pivot: &T,
    cmp: &impl Fn(&T, &T) -> Ordering,
) -> (usize, usize) {
    let (mut lt, mut i, mut gt) = (lo, lo, hi);
    while i < gt {
        match cmp(&data[i], pivot) {
            Ordering::Less => {
                data.swap(lt, i);
                lt += 1;
                i += 1;
            }
            Ordering::Equal => i += 1,
            Ordering::Greater => {
                gt -= 1;
                data.swap(i, gt);
            }
        }
    }
    (lt, gt)
}

/// Mediana de tres valores segun `cmp`.
fn median_of_three<T: Copy>(a: T, b: T, c: T, cmp: &impl Fn(&T, &T) -> Ordering) -> T {
    let (lo, hi) = if cmp(&a, &b) == Ordering::Greater {
        (b, a)
    } else {
        (a, b)
    };
    if cmp(&c, &lo) == Ordering::Less {
        lo
    } else if cmp(&c, &hi) == Ordering::Greater {
        hi
    } else {
        c
    }
}

/// k-esimo menor (desde 0) de `data` con quickselect. Reordena `data` en el proceso.
///
/// Al volver, `data[..k]` son menores o iguales que el resultado y `data[k + 1..]` mayores
/// o iguales. Los percentiles dependen de esto para seleccionar varias posiciones seguidas
/// sobre el mismo tramo.
///
/// # Errors
/// [`Error::OutOfBounds`] si `k >= data.len()` (incluye el caso de `data` vacio).
pub fn quickselect<T: Copy>(
    data: &mut [T],
    k: usize,
    cmp: impl Fn(&T, &T) -> Ordering,
) -> Result<T, Error> {
    if k >= data.len() {
        return Err(out_of_bounds(k, data.len()));
    }
    let (mut lo, mut hi) = (0, data.len());
    // Invariante: el k-esimo menor de `data` esta en `data[lo..hi]`.
    while hi - lo > 1 {
        let pivot = median_of_three(data[lo], data[lo + (hi - lo) / 2], data[hi - 1], &cmp);
        let (lt, gt) = partition3(data, (lo, hi), &pivot, &cmp);
        if k < lt {
            hi = lt;
        } else if k >= gt {
            lo = gt;
        } else {
            return Ok(data[k]);
        }
    }
    Ok(data[k])
}

/// Ordena a mano un tramo corto por insercion: aqui son a lo sumo 5 elementos.
fn insertion_sort<T: Copy>(data: &mut [T], cmp: &impl Fn(&T, &T) -> Ordering) {
    for i in 1..data.len() {
        let mut j = i;
        while j > 0 && cmp(&data[j - 1], &data[j]) == Ordering::Greater {
            data.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// Pivote de la mediana de medianas: agrupa de 5 en 5, toma la mediana de cada grupo
/// (ordenandolo, es barato) y devuelve la mediana de esas medianas, hallada
/// recursivamente con el mismo algoritmo. Reordena `data`.
fn pivot_of_medians<T: Copy>(data: &mut [T], cmp: &impl Fn(&T, &T) -> Ordering) -> T {
    const GROUP: usize = 5;
    let groups = data.len().div_ceil(GROUP);
    for g in 0..groups {
        let start = g * GROUP;
        let end = (start + GROUP).min(data.len());
        insertion_sort(&mut data[start..end], cmp);
        // La mediana del grupo pasa al frente; `g <= start`, asi que nunca pisa un
        // grupo que aun no se ha procesado.
        data.swap(g, start + (end - start) / 2);
    }
    select_by_medians(&mut data[..groups], groups / 2, cmp)
}

/// Nucleo de la mediana de medianas. `k < data.len()` ya esta comprobado.
fn select_by_medians<T: Copy>(data: &mut [T], k: usize, cmp: &impl Fn(&T, &T) -> Ordering) -> T {
    let (mut lo, mut hi) = (0, data.len());
    while hi - lo > 5 {
        let pivot = pivot_of_medians(&mut data[lo..hi], cmp);
        let (lt, gt) = partition3(data, (lo, hi), &pivot, cmp);
        if k < lt {
            hi = lt;
        } else if k >= gt {
            lo = gt;
        } else {
            return data[k];
        }
    }
    insertion_sort(&mut data[lo..hi], cmp);
    data[k]
}

/// k-esimo menor (desde 0) de `data` con mediana de medianas: lineal en el peor caso.
/// Reordena `data` en el proceso.
///
/// Al volver, `data[..k]` son menores o iguales que el resultado y `data[k + 1..]` mayores
/// o iguales. Los percentiles dependen de esto para seleccionar varias posiciones seguidas
/// sobre el mismo tramo.
///
/// # Errors
/// [`Error::OutOfBounds`] si `k >= data.len()` (incluye el caso de `data` vacio).
pub fn median_of_medians<T: Copy>(
    data: &mut [T],
    k: usize,
    cmp: impl Fn(&T, &T) -> Ordering,
) -> Result<T, Error> {
    if k >= data.len() {
        return Err(out_of_bounds(k, data.len()));
    }
    Ok(select_by_medians(data, k, &cmp))
}

/// k-esimo menor (desde 0) de una columna `Int32`, `Int64` o `Float64`.
///
/// Los nulos se ignoran (decision F1): `k` cuenta solo sobre los valores validos. En
/// `Float64` el orden es el total de [`f64::total_cmp`] (decision F2). La columna no
/// cambia.
///
/// # Errors
/// - [`Error::OutOfBounds`] si `k` no es menor que la cantidad de valores validos.
/// - [`Error::Unsupported`] para `Bool` y `Utf8`.
pub fn select_column(column: &Column, k: usize, method: Method) -> Result<Scalar, Error> {
    match column.dtype() {
        DataType::Int32 => select_primitive::<i32>(column, k, method, i32::cmp).map(Scalar::Int32),
        DataType::Int64 => select_primitive::<i64>(column, k, method, i64::cmp).map(Scalar::Int64),
        DataType::Float64 => {
            select_primitive::<f64>(column, k, method, f64::total_cmp).map(Scalar::Float64)
        }
        other => Err(Error::Unsupported(format!(
            "seleccionar en columnas {other:?} aun no esta soportado"
        ))),
    }
}

/// Valores no nulos de una columna primitiva, en su orden original.
pub(crate) fn valid_values<T: Native>(column: &Column) -> Result<Vec<T>, Error> {
    let values = column.values::<T>()?;
    let mut valid = Vec::with_capacity(values.len() - column.null_count());
    for (i, &value) in values.iter().enumerate() {
        if !column.is_null(i)? {
            valid.push(value);
        }
    }
    Ok(valid)
}

/// Despacha al algoritmo de `method`.
pub(crate) fn select_with<T: Copy>(
    data: &mut [T],
    k: usize,
    method: Method,
    cmp: impl Fn(&T, &T) -> Ordering,
) -> Result<T, Error> {
    match method {
        Method::Quickselect => quickselect(data, k, cmp),
        Method::MedianOfMedians => median_of_medians(data, k, cmp),
    }
}

fn select_primitive<T: Native>(
    column: &Column,
    k: usize,
    method: Method,
    cmp: impl Fn(&T, &T) -> Ordering,
) -> Result<T, Error> {
    let mut valid = valid_values::<T>(column)?;
    select_with(&mut valid, k, method, cmp)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use geoda_core::{BoolBuilder, PrimitiveBuilder};

    use super::*;

    /// Generador pseudoaleatorio con semilla fija (xorshift64), igual que en `sort`.
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

    const METHODS: [Method; 2] = [Method::Quickselect, Method::MedianOfMedians];

    fn select_slice(method: Method, data: &mut [i64], k: usize) -> Result<i64, Error> {
        match method {
            Method::Quickselect => quickselect(data, k, i64::cmp),
            Method::MedianOfMedians => median_of_medians(data, k, i64::cmp),
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
    fn both_methods_match_the_sorted_oracle_on_1000_seeded_inputs() {
        let mut rng = Rng(0xC0FF_EE11);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(80)).unwrap() + 1;
            // Rango chico para muchos repetidos; el segundo para casi todos distintos.
            let range = [4, 1_000_000][case % 2];
            let data: Vec<i64> = (0..len)
                .map(|_| i64::try_from(rng.below(range)).unwrap() - 2)
                .collect();
            let mut sorted = data.clone();
            sorted.sort_unstable();
            let k = usize::try_from(rng.below(len as u64)).unwrap();
            for method in METHODS {
                let mut work = data.clone();
                assert_eq!(
                    select_slice(method, &mut work, k).unwrap(),
                    sorted[k],
                    "{method:?}, caso {case}, k = {k}: {data:?}"
                );
            }
        }
    }

    #[test]
    fn every_k_of_a_small_input_is_correct() {
        let data = [5_i64, 3, 3, 9, -1, 5, 5, 0, 12, 3, 7];
        let mut sorted = data;
        sorted.sort_unstable();
        for method in METHODS {
            for (k, expected) in sorted.iter().enumerate() {
                let mut work = data;
                assert_eq!(select_slice(method, &mut work, k).unwrap(), *expected);
            }
        }
    }

    #[test]
    fn single_element_and_out_of_range_k() {
        for method in METHODS {
            assert_eq!(select_slice(method, &mut [42], 0).unwrap(), 42);
            assert!(matches!(
                select_slice(method, &mut [42], 1),
                Err(Error::OutOfBounds(_))
            ));
            assert!(matches!(
                select_slice(method, &mut [], 0),
                Err(Error::OutOfBounds(_))
            ));
        }
    }

    /// Cuantas comparaciones hace `method` para hallar la mediana de `data`.
    fn comparisons(method: Method, data: &[i64]) -> usize {
        let count = Cell::new(0_usize);
        let cmp = |a: &i64, b: &i64| {
            count.set(count.get() + 1);
            a.cmp(b)
        };
        let mut work = data.to_vec();
        let k = work.len() / 2;
        match method {
            Method::Quickselect => quickselect(&mut work, k, cmp).unwrap(),
            Method::MedianOfMedians => median_of_medians(&mut work, k, cmp).unwrap(),
        };
        count.get()
    }

    #[test]
    fn adversarial_inputs_stay_linear() {
        let n = 10_000_usize;
        let n_i64 = i64::try_from(n).unwrap();
        let mut rng = Rng(99);
        let inputs: [(&str, Vec<i64>); 5] = [
            ("todo igual", vec![7; n]),
            ("ya ordenada", (0..n_i64).collect()),
            ("orden inverso", (0..n_i64).rev().collect()),
            (
                "pocos distintos",
                (0..n)
                    .map(|_| i64::try_from(rng.below(3)).unwrap())
                    .collect(),
            ),
            (
                "aleatoria",
                (0..n)
                    .map(|_| i64::try_from(rng.below(1_000_000)).unwrap())
                    .collect(),
            ),
        ];
        for (name, data) in &inputs {
            for method in METHODS {
                let used = comparisons(method, data);
                // Medido: quickselect usa de 1 a 3 n comparaciones y la mediana de
                // medianas de 2 a 8 n. Un algoritmo cuadratico haria ~n^2 / 2 = 50 000 000.
                // 15 n deja margen a los lineales y atrapa un pivote degradado.
                assert!(
                    used <= 15 * n,
                    "{method:?} sobre «{name}»: {used} comparaciones para n = {n}"
                );
            }
        }
    }

    /// Cuantos valores de `data` son menores o iguales, y cuantos mayores o iguales, que `pivot`.
    fn pivot_ranks(data: &[i64], pivot: i64) -> (usize, usize) {
        (
            data.iter().filter(|&&v| v <= pivot).count(),
            data.iter().filter(|&&v| v >= pivot).count(),
        )
    }

    #[test]
    fn median_of_medians_pivot_is_always_central() {
        // La garantia del peor caso lineal: el pivote deja al menos ~30 % de los
        // valores a cada lado, pase lo que pase con la entrada. Construir una entrada que
        // haga cuadratico a quickselect con mediana de tres es delicado, asi que la
        // garantia se comprueba directamente sobre la calidad del pivote.
        let n = 1000_usize;
        let n_i64 = i64::try_from(n).unwrap();
        let mut rng = Rng(5);
        // Cada grupo de 5 es [chico, grande, grande, grande, grande]: elegir mal la
        // mediana de cada grupo (por ejemplo tomar su primer elemento) da un pivote chico.
        let small_first: Vec<i64> = (0..n_i64)
            .map(|i| if i % 5 == 0 { i } else { 1_000_000 + i })
            .collect();
        let inputs: [(&str, Vec<i64>); 4] = [
            ("un chico y cuatro grandes por grupo", small_first),
            ("ordenada", (0..n_i64).collect()),
            ("inversa", (0..n_i64).rev().collect()),
            (
                "aleatoria",
                (0..n)
                    .map(|_| i64::try_from(rng.below(1_000_000)).unwrap())
                    .collect(),
            ),
        ];
        for (name, data) in &inputs {
            let mut work = data.clone();
            let pivot = pivot_of_medians(&mut work, &i64::cmp);
            let (at_most, at_least) = pivot_ranks(data, pivot);
            // 3n/10 menos un margen pequeno por el redondeo de los grupos.
            let floor = 3 * n / 10 - 5;
            assert!(
                at_most >= floor && at_least >= floor,
                "«{name}»: pivote {pivot} con {at_most} <= y {at_least} >= (minimo {floor})"
            );
        }
    }

    #[test]
    fn nulls_are_ignored_and_k_counts_only_valid_values() {
        let column = int64_column(&[None, Some(30), Some(10), None, Some(20)]);
        for method in METHODS {
            assert_eq!(
                select_column(&column, 0, method).unwrap(),
                Scalar::Int64(10)
            );
            assert_eq!(
                select_column(&column, 2, method).unwrap(),
                Scalar::Int64(30)
            );
            // Solo hay 3 validos: k = 3 ya esta fuera de rango aunque la columna mida 5.
            assert!(matches!(
                select_column(&column, 3, method),
                Err(Error::OutOfBounds(_))
            ));
        }
    }

    #[test]
    fn float64_uses_the_total_order_with_nan() {
        let mut builder = PrimitiveBuilder::<f64>::new();
        for value in [f64::NAN, 2.5, -0.0, 0.0, f64::NEG_INFINITY] {
            builder.append(value);
        }
        let column = builder.finish().unwrap();
        // Orden total: -inf, -0.0, 0.0, 2.5, NaN.
        let expected_bits = [
            f64::NEG_INFINITY.to_bits(),
            (-0.0_f64).to_bits(),
            0.0_f64.to_bits(),
            2.5_f64.to_bits(),
            f64::NAN.to_bits(),
        ];
        for method in METHODS {
            for (k, bits) in expected_bits.iter().enumerate() {
                let Scalar::Float64(got) = select_column(&column, k, method).unwrap() else {
                    panic!("se esperaba un Float64");
                };
                if k == 4 {
                    assert!(got.is_nan(), "{method:?}, k = 4");
                } else {
                    assert_eq!(got.to_bits(), *bits, "{method:?}, k = {k}");
                }
            }
        }
    }

    #[test]
    fn int32_empty_and_unsupported_columns() {
        let mut ints = PrimitiveBuilder::<i32>::new();
        for value in [9, -4, 6] {
            ints.append(value);
        }
        let ints = ints.finish().unwrap();
        assert_eq!(
            select_column(&ints, 1, Method::default()).unwrap(),
            Scalar::Int32(6)
        );

        let empty = int64_column(&[]);
        assert!(matches!(
            select_column(&empty, 0, Method::default()),
            Err(Error::OutOfBounds(_))
        ));
        let all_null = int64_column(&[None, None]);
        assert!(matches!(
            select_column(&all_null, 0, Method::default()),
            Err(Error::OutOfBounds(_))
        ));

        let mut bools = BoolBuilder::new();
        bools.append(true);
        assert!(matches!(
            select_column(&bools.finish().unwrap(), 0, Method::default()),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn sliced_columns_and_the_original_is_untouched() {
        let column = int64_column(&[Some(9), Some(4), None, Some(1), Some(7)]);
        let window = column.slice(1, 3).unwrap(); // [4, NULL, 1]
        assert_eq!(
            select_column(&window, 0, Method::default()).unwrap(),
            Scalar::Int64(1)
        );
        assert_eq!(
            select_column(&window, 1, Method::default()).unwrap(),
            Scalar::Int64(4)
        );
        assert_eq!(column.values::<i64>().unwrap()[0], 9);
    }
}
