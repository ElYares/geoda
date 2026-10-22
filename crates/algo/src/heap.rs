//! Monticulo binario y los `k` mayores de una columna.
//!
//! # Que es un monticulo
//! Un monticulo (heap) binario guarda sus elementos en un arreglo que se lee como un
//! arbol: el elemento `i` tiene por hijos a `2i + 1` y `2i + 2`, y su padre es
//! `(i - 1) / 2`. La regla es que **cada padre es menor o igual que sus hijos**, asi que
//! el menor de todos esta siempre en la raiz (posicion 0).
//!
//! - Al **insertar** se pone el valor al final y se le hace *subir* mientras sea menor que
//!   su padre.
//! - Al **extraer** el menor se saca la raiz, se pone en su lugar el ultimo elemento y se le
//!   hace *bajar* intercambiandolo con el menor de sus hijos mientras sea mayor que alguno.
//!
//! Subir o bajar recorre la altura del arbol, log2(n) pasos, asi que ambas cuestan
//! O(log n).
//!
//! # Los `k` mayores
//! Para quedarse con los `k` mayores de `n` valores sin ordenarlos todos se usa un
//! monticulo de **tamano `k` cuya raiz es el menor de los `k` candidatos**. Cada valor
//! nuevo se compara solo con esa raiz: si no la supera, no puede estar entre los `k`
//! mayores; si la supera, ocupa su lugar. Cada valor cuesta a lo sumo O(log k), asi que el
//! total es O(n log k) en lugar de O(n log n).

use std::cmp::Ordering;

use geoda_core::memory::Native;
use geoda_core::{Column, DataType, Error, PrimitiveBuilder};

/// Monticulo binario: la raiz es el **menor** segun `cmp`. Con un `cmp` invertido la raiz
/// es el mayor.
pub struct Heap<T, C: Fn(&T, &T) -> Ordering> {
    items: Vec<T>,
    cmp: C,
}

impl<T, C: Fn(&T, &T) -> Ordering> Heap<T, C> {
    /// Monticulo vacio.
    pub fn new(cmp: C) -> Self {
        Self::with_capacity(0, cmp)
    }

    /// Monticulo vacio con espacio para `capacity` elementos.
    pub fn with_capacity(capacity: usize, cmp: C) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            cmp,
        }
    }

    /// Cantidad de elementos.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` si no hay elementos.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// El menor elemento (la raiz), sin sacarlo.
    pub fn peek(&self) -> Option<&T> {
        self.items.first()
    }

    /// Inserta un elemento en O(log n).
    pub fn push(&mut self, item: T) {
        self.items.push(item);
        self.sift_up(self.items.len() - 1);
    }

    /// Saca y devuelve el menor elemento en O(log n), o `None` si esta vacio.
    pub fn pop(&mut self) -> Option<T> {
        let last = self.items.len().checked_sub(1)?;
        self.items.swap(0, last);
        let smallest = self.items.pop();
        self.sift_down(0);
        smallest
    }

    /// Reemplaza la raiz por `item` y devuelve la anterior, en O(log n). Es un `pop` y un
    /// `push` en un solo recorrido. Si esta vacio, inserta `item` y devuelve `None`.
    pub fn replace_top(&mut self, item: T) -> Option<T> {
        if self.items.is_empty() {
            self.items.push(item);
            return None;
        }
        let old = std::mem::replace(&mut self.items[0], item);
        self.sift_down(0);
        Some(old)
    }

    /// Hace subir el elemento `i` mientras sea menor que su padre.
    fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let parent = (i - 1) / 2;
            if (self.cmp)(&self.items[i], &self.items[parent]) != Ordering::Less {
                break;
            }
            self.items.swap(i, parent);
            i = parent;
        }
    }

    /// Hace bajar el elemento `i` mientras sea mayor que alguno de sus hijos.
    fn sift_down(&mut self, mut i: usize) {
        let len = self.items.len();
        loop {
            let (left, right) = (2 * i + 1, 2 * i + 2);
            let mut smallest = i;
            if left < len && (self.cmp)(&self.items[left], &self.items[smallest]) == Ordering::Less
            {
                smallest = left;
            }
            if right < len
                && (self.cmp)(&self.items[right], &self.items[smallest]) == Ordering::Less
            {
                smallest = right;
            }
            if smallest == i {
                break;
            }
            self.items.swap(i, smallest);
            i = smallest;
        }
    }
}

/// Los `k` mayores de `data` segun `cmp`, de mayor a menor, en O(n log k).
///
/// Si `k >= data.len()` devuelve todos los elementos ordenados; si `k == 0`, un vector
/// vacio. Los empates no se pierden: dos valores iguales ocupan dos lugares.
pub fn top_k<T: Copy>(data: &[T], k: usize, cmp: impl Fn(&T, &T) -> Ordering) -> Vec<T> {
    // La capacidad nunca es `k` a secas: un `k` enorme no debe reservar memoria.
    let mut heap = Heap::with_capacity(k.min(data.len()), &cmp);
    if k > 0 {
        for &value in data {
            if heap.len() < k {
                heap.push(value);
            } else if heap
                .peek()
                .is_some_and(|min| cmp(&value, min) == Ordering::Greater)
            {
                heap.replace_top(value);
            }
        }
    }
    // Se extrae de menor a mayor; al reves es de mayor a menor.
    let mut result = Vec::with_capacity(heap.len());
    while let Some(value) = heap.pop() {
        result.push(value);
    }
    result.reverse();
    result
}

/// Los `k` mayores de una columna `Int32`, `Int64` o `Float64`, como columna **nueva** del
/// mismo tipo, de mayor a menor y sin nulos. La original no cambia.
///
/// Los nulos se ignoran (decision F1): `k` cuenta sobre los valores validos, y si hay menos
/// de `k` se devuelven todos. En `Float64` el orden es el total de [`f64::total_cmp`]
/// (decision F2), donde `NaN` es el mayor.
///
/// # Errors
/// [`Error::Unsupported`] para `Bool` y `Utf8`.
pub fn top_k_column(column: &Column, k: usize) -> Result<Column, Error> {
    match column.dtype() {
        DataType::Int32 => top_k_primitive::<i32>(column, k, i32::cmp),
        DataType::Int64 => top_k_primitive::<i64>(column, k, i64::cmp),
        DataType::Float64 => top_k_primitive::<f64>(column, k, f64::total_cmp),
        other => Err(Error::Unsupported(format!(
            "los k mayores de columnas {other:?} aun no esta soportado"
        ))),
    }
}

fn top_k_primitive<T: Native>(
    column: &Column,
    k: usize,
    cmp: impl Fn(&T, &T) -> Ordering,
) -> Result<Column, Error> {
    let valid = crate::select::valid_values::<T>(column)?;
    let mut builder = PrimitiveBuilder::<T>::new();
    for value in top_k(&valid, k, cmp) {
        builder.append(value);
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use geoda_core::BoolBuilder;

    use super::*;

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

    fn int64_values(column: &Column) -> Vec<i64> {
        column.values::<i64>().unwrap().to_vec()
    }

    /// La regla del monticulo: ningun hijo es menor que su padre.
    fn is_heap<C: Fn(&i64, &i64) -> Ordering>(heap: &Heap<i64, C>) -> bool {
        (1..heap.items.len()).all(|i| heap.items[i] >= heap.items[(i - 1) / 2])
    }

    #[test]
    fn pop_returns_values_in_ascending_order_and_len_drops_by_one() {
        let mut heap = Heap::new(i64::cmp);
        assert!(heap.is_empty());
        assert_eq!(heap.pop(), None);
        assert_eq!(heap.peek(), None);
        for value in [5, 3, 8, 1, 9, 3, 7] {
            heap.push(value);
            assert!(is_heap(&heap));
        }
        assert_eq!(heap.len(), 7);
        assert_eq!(heap.peek(), Some(&1));
        let mut out = Vec::new();
        while let Some(value) = heap.pop() {
            assert!(is_heap(&heap));
            assert_eq!(heap.len(), 7 - out.len() - 1);
            out.push(value);
        }
        assert_eq!(out, [1, 3, 3, 5, 7, 8, 9]);
    }

    #[test]
    fn a_reversed_comparator_makes_a_max_heap() {
        let mut heap = Heap::new(|a: &i64, b: &i64| b.cmp(a));
        for value in [5, 3, 8, 1] {
            heap.push(value);
        }
        assert_eq!(heap.pop(), Some(8));
        assert_eq!(heap.pop(), Some(5));
    }

    #[test]
    fn replace_top_swaps_the_root_and_keeps_the_rule() {
        let mut heap = Heap::new(i64::cmp);
        assert_eq!(heap.replace_top(4), None);
        for value in [6, 2, 9] {
            heap.push(value);
        }
        assert_eq!(heap.replace_top(7), Some(2));
        assert!(is_heap(&heap));
        assert_eq!(heap.len(), 4);
        assert_eq!(heap.peek(), Some(&4));
    }

    #[test]
    fn top_k_of_a_small_input() {
        let data = [4_i64, 9, 1, 7, 9, 3];
        assert_eq!(top_k(&data, 3, i64::cmp), [9, 9, 7]);
        assert_eq!(top_k(&data, 0, i64::cmp), Vec::<i64>::new());
        assert_eq!(top_k(&data, 6, i64::cmp), [9, 9, 7, 4, 3, 1]);
        assert_eq!(top_k(&data, usize::MAX, i64::cmp), [9, 9, 7, 4, 3, 1]);
        assert_eq!(top_k(&[], 3, i64::cmp), Vec::<i64>::new());
    }

    #[test]
    fn matches_sort_and_truncate_on_1000_seeded_inputs() {
        let mut rng = Rng(0x70B_0012);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(60)).unwrap();
            // Rango chico para muchos empates; el segundo para casi todos distintos.
            let range = [4, 1_000_000][case % 2];
            let data: Vec<i64> = (0..len)
                .map(|_| i64::try_from(rng.below(range)).unwrap() - 2)
                .collect();
            let k = usize::try_from(rng.below(70)).unwrap();
            let mut oracle = data.clone();
            oracle.sort_unstable_by(|a, b| b.cmp(a));
            oracle.truncate(k);
            assert_eq!(
                top_k(&data, k, i64::cmp),
                oracle,
                "caso {case}, k = {k}: {data:?}"
            );
        }
    }

    #[test]
    fn columns_ignore_nulls_and_keep_ties() {
        let column = int64_column(&[Some(5), None, Some(9), Some(5), None, Some(1), Some(9)]);
        let top = top_k_column(&column, 4).unwrap();
        assert_eq!(top.null_count(), 0);
        assert_eq!(int64_values(&top), [9, 9, 5, 5]);
        // k mayor que los validos: todos los validos, y los nulos no cuentan.
        assert_eq!(
            int64_values(&top_k_column(&column, 99).unwrap()),
            [9, 9, 5, 5, 1]
        );
        assert_eq!(top_k_column(&column, 0).unwrap().len(), 0);
        assert_eq!(
            top_k_column(&int64_column(&[None, None]), 3).unwrap().len(),
            0
        );
        // La original no cambia.
        assert_eq!(column.len(), 7);
        assert_eq!(column.null_count(), 2);
    }

    #[test]
    fn float64_uses_the_total_order_where_nan_is_the_largest() {
        let mut builder = PrimitiveBuilder::<f64>::new();
        for value in [2.5, f64::NAN, -0.0, 0.0, f64::NEG_INFINITY] {
            builder.append(value);
        }
        let top = top_k_column(&builder.finish().unwrap(), 3).unwrap();
        let values = top.values::<f64>().unwrap();
        assert!(values[0].is_nan());
        assert_eq!(values[1].to_bits(), 2.5_f64.to_bits());
        assert_eq!(values[2].to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn int32_sliced_and_unsupported_columns() {
        let mut ints = PrimitiveBuilder::<i32>::new();
        for value in [9, -4, 6, 2] {
            ints.append(value);
        }
        let ints = ints.finish().unwrap();
        let top = top_k_column(&ints, 2).unwrap();
        assert_eq!(top.values::<i32>().unwrap(), [9, 6]);

        let column = int64_column(&[Some(1), Some(8), None, Some(3), Some(7)]);
        let window = column.slice(1, 3).unwrap(); // [8, NULL, 3]
        assert_eq!(int64_values(&top_k_column(&window, 5).unwrap()), [8, 3]);

        let mut bools = BoolBuilder::new();
        bools.append(true);
        assert!(matches!(
            top_k_column(&bools.finish().unwrap(), 1),
            Err(Error::Unsupported(_))
        ));
    }
}
