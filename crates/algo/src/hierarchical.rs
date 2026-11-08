//! Clustering jerarquico de enlace simple (*single-linkage*) sobre el arbol de expansion minimo.
//!
//! # La idea
//! Se empieza con cada fila en su propio grupo y se une siempre el par de grupos mas cercano,
//! donde la distancia entre dos grupos es la **menor** distancia entre una fila de uno y una del
//! otro (enlace simple). Resulta que ese proceso es exactamente el algoritmo de Kruskal sobre el
//! grafo completo de distancias: las aristas del arbol de expansion minimo, de menor a mayor, son
//! las uniones, y su peso es la altura a la que ocurren.
//!
//! # El dendrograma
//! Sigue la convencion de la matriz de enlaces de `scipy`: las filas originales son los grupos
//! `0..n` y la union numero `s` crea el grupo `n + s`. Cada union anota `left`, `right` (con
//! `left < right`), `height` (la distancia euclidiana) y `size`. Las alturas no decrecen.
//!
//! # Empates y determinismo (M8)
//! Las aristas se generan en el orden `(i, j)` con `i < j` y el `merge_sort` de Kruskal es
//! estable: con alturas iguales gana la arista que aparece antes. La misma entrada da siempre el
//! mismo dendrograma.
//!
//! # Tope de tamano (M5)
//! Se construyen las `n (n - 1) / 2` aristas (a 24 bytes cada una): con [`MAX_POINTS`] = 5000 son
//! 12.5 millones, unos 300 MB. Mas puntos son `InvalidArgument`, con el tope en el mensaje.

use geoda_core::{Column, Error};

use crate::components::UnionFind;
use crate::matrix::{Dataset, squared_distance};
use crate::spanning::kruskal;

/// Maximo de filas que acepta [`single_linkage`].
pub const MAX_POINTS: usize = 5000;

/// Resultado de [`single_linkage`]: `n - 1` uniones (o ninguna si `n <= 1`).
#[derive(Debug, Clone, PartialEq)]
pub struct Dendrogram {
    /// Cantidad de filas (hojas).
    pub n: usize,
    /// Primer grupo de cada union.
    pub left: Vec<usize>,
    /// Segundo grupo de cada union (`left < right`).
    pub right: Vec<usize>,
    /// Distancia a la que se unen (no decrece).
    pub height: Vec<f64>,
    /// Filas del grupo resultante.
    pub size: Vec<usize>,
}

/// Dendrograma de enlace simple de una lista de columnas (una por variable).
///
/// # Errors
/// Los de [`Dataset::from_columns`] y los de [`single_linkage`].
pub fn single_linkage_columns(columns: &[&Column]) -> Result<Dendrogram, Error> {
    single_linkage(&Dataset::from_columns(columns)?)
}

/// Dendrograma de enlace simple de una matriz, con distancia euclidiana.
///
/// # Errors
/// [`Error::InvalidArgument`] si hay mas de [`MAX_POINTS`] filas.
pub fn single_linkage(data: &Dataset) -> Result<Dendrogram, Error> {
    let n = data.rows();
    if n > MAX_POINTS {
        return Err(Error::InvalidArgument(format!(
            "el clustering jerarquico acepta hasta {MAX_POINTS} filas, no {n}"
        )));
    }
    let mut dendrogram = Dendrogram {
        n,
        left: Vec::new(),
        right: Vec::new(),
        height: Vec::new(),
        size: Vec::new(),
    };
    if n < 2 {
        return Ok(dendrogram);
    }
    let mut edges = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in i + 1..n {
            edges.push((i, j, squared_distance(data.row(i), data.row(j)).sqrt()));
        }
    }
    let forest = kruskal(n, &edges)?;
    let mut sets = UnionFind::new(n)?;
    // Para cada raiz del union-find: el id de grupo del dendrograma y cuantas filas tiene.
    let mut group_of_root: Vec<usize> = (0..n).collect();
    let mut size_of_root = vec![1usize; n];
    for (step, &edge) in forest.edges.iter().enumerate() {
        let (u, v, height) = edges[edge];
        let (root_u, root_v) = (sets.find(u)?, sets.find(v)?);
        let (group_u, group_v) = (group_of_root[root_u], group_of_root[root_v]);
        let size = size_of_root[root_u] + size_of_root[root_v];
        sets.union(root_u, root_v)?;
        let root = sets.find(root_u)?;
        group_of_root[root] = n + step;
        size_of_root[root] = size;
        dendrogram.left.push(group_u.min(group_v));
        dendrogram.right.push(group_u.max(group_v));
        dendrogram.height.push(height);
        dendrogram.size.push(size);
    }
    Ok(dendrogram)
}

impl Dendrogram {
    /// Corta el dendrograma en `k` grupos: aplica las primeras `n - k` uniones. Las etiquetas
    /// van de `0` a `k - 1`, numeradas por la primera fila en que aparece cada grupo.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si `k` es 0 o mayor que `n`.
    pub fn cut(&self, k: usize) -> Result<Vec<usize>, Error> {
        cut_merges(self.n, &self.left, &self.right, k)
    }
}

/// Corta en `k` grupos un dendrograma dado solo por sus uniones (`left` y `right`, como las de
/// [`Dendrogram`]): sirve para cortar uno que se guardo fuera de Rust.
///
/// # Errors
/// [`Error::InvalidArgument`] si `k` es 0 o mayor que `n`, si hay mas o menos de `n - 1` uniones,
/// o si una union menciona un grupo que todavia no existe (`>= n + paso`).
pub fn cut_merges(
    n: usize,
    left: &[usize],
    right: &[usize],
    k: usize,
) -> Result<Vec<usize>, Error> {
    if k == 0 || k > n {
        return Err(Error::InvalidArgument(format!(
            "k = {k} debe estar entre 1 y el numero de filas ({n})"
        )));
    }
    let merges = n.saturating_sub(1);
    if left.len() != merges || right.len() != merges {
        return Err(Error::InvalidArgument(format!(
            "un dendrograma de {n} filas tiene {merges} uniones, no {} y {}",
            left.len(),
            right.len()
        )));
    }
    let mut sets = UnionFind::new(n)?;
    // Una fila cualquiera de cada grupo del dendrograma, para poder unir por fila.
    let mut leaf_of_group: Vec<usize> = (0..n).collect();
    for step in 0..merges {
        let (a, b) = (left[step], right[step]);
        if a >= n + step || b >= n + step {
            return Err(Error::InvalidArgument(format!(
                "la union {step} usa el grupo {} que todavia no existe",
                a.max(b)
            )));
        }
        let (leaf_a, leaf_b) = (leaf_of_group[a], leaf_of_group[b]);
        if step < n - k {
            sets.union(leaf_a, leaf_b)?;
        }
        leaf_of_group.push(leaf_a);
    }
    let mut label_of_root: Vec<Option<usize>> = vec![None; n];
    let mut next = 0;
    let mut labels = Vec::with_capacity(n);
    for row in 0..n {
        let root = sets.find(row)?;
        labels.push(*label_of_root[root].get_or_insert_with(|| {
            next += 1;
            next - 1
        }));
    }
    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::{MAX_POINTS, single_linkage, single_linkage_columns};
    use crate::matrix::{Dataset, squared_distance};
    use geoda_core::{Column, Error, PrimitiveBuilder};

    fn floats(values: &[f64]) -> Column {
        let mut b = PrimitiveBuilder::<f64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    #[test]
    fn el_ejemplo_de_la_historia() {
        let x = floats(&[0.0, 1.0, 5.0, 6.0]);
        let d = single_linkage_columns(&[&x]).unwrap();
        assert_eq!(d.left, [0, 2, 4]);
        assert_eq!(d.right, [1, 3, 5]);
        assert_eq!(d.height, [1.0, 1.0, 4.0]);
        assert_eq!(d.size, [2, 2, 4]);
        assert_eq!(d.cut(2).unwrap(), [0, 0, 1, 1]);
        assert_eq!(d.cut(4).unwrap(), [0, 1, 2, 3]);
        assert_eq!(d.cut(1).unwrap(), [0, 0, 0, 0]);
    }

    #[test]
    fn cortar_por_uniones_valida_la_entrada() {
        use super::cut_merges;
        assert_eq!(
            cut_merges(4, &[0, 2, 4], &[1, 3, 5], 2).unwrap(),
            [0, 0, 1, 1]
        );
        assert!(matches!(
            cut_merges(4, &[0, 2], &[1, 3], 2),
            Err(Error::InvalidArgument(_))
        ));
        // La union 0 menciona el grupo 9, que no existe todavia.
        assert!(matches!(
            cut_merges(4, &[0, 2, 4], &[9, 3, 5], 2),
            Err(Error::InvalidArgument(_))
        ));
        assert_eq!(cut_merges(1, &[], &[], 1).unwrap(), [0]);
    }

    #[test]
    fn una_fila_y_ninguna() {
        let one = single_linkage_columns(&[&floats(&[3.0])]).unwrap();
        assert_eq!(one.left.len(), 0);
        assert_eq!(one.cut(1).unwrap(), [0]);
        let none = single_linkage_columns(&[&floats(&[])]).unwrap();
        assert_eq!(none.n, 0);
        assert!(matches!(none.cut(1), Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn cortes_invalidos() {
        let d = single_linkage_columns(&[&floats(&[0.0, 1.0, 5.0])]).unwrap();
        assert!(matches!(d.cut(0), Err(Error::InvalidArgument(_))));
        assert!(matches!(d.cut(4), Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn mas_filas_que_el_tope() {
        let big = floats(&vec![0.0; MAX_POINTS + 1]);
        let err = single_linkage_columns(&[&big]).unwrap_err();
        assert!(
            matches!(&err, Error::InvalidArgument(m) if m.contains("5000")),
            "{err}"
        );
    }

    #[test]
    fn las_alturas_no_decrecen_y_el_empate_es_estable() {
        let x = floats(&[0.0, 1.0, 2.0, 3.0, 4.0]);
        let d = single_linkage_columns(&[&x]).unwrap();
        assert!(d.height.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(d.size.last(), Some(&5));
        assert_eq!(single_linkage_columns(&[&x]).unwrap(), d);
    }

    /// Oraculo: enlace simple ingenuo, O(n^3). Devuelve las alturas de union.
    fn naive_heights(data: &Dataset) -> Vec<f64> {
        let mut clusters: Vec<Vec<usize>> = (0..data.rows()).map(|i| vec![i]).collect();
        let mut heights = Vec::new();
        while clusters.len() > 1 {
            let mut best = (0, 1, f64::INFINITY);
            for a in 0..clusters.len() {
                for b in a + 1..clusters.len() {
                    for &i in &clusters[a] {
                        for &j in &clusters[b] {
                            let d = squared_distance(data.row(i), data.row(j)).sqrt();
                            if d < best.2 {
                                best = (a, b, d);
                            }
                        }
                    }
                }
            }
            let merged = clusters.remove(best.1);
            clusters[best.0].extend(merged);
            heights.push(best.2);
        }
        heights
    }

    #[test]
    fn coincide_con_el_enlace_simple_ingenuo() {
        let mut state: u64 = 0x51;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 40).unwrap()) / 100.0
        };
        for n in [2, 3, 7, 25] {
            let x = floats(&(0..n).map(|_| next()).collect::<Vec<_>>());
            let y = floats(&(0..n).map(|_| next()).collect::<Vec<_>>());
            let data = Dataset::from_columns(&[&x, &y]).unwrap();
            let dendrogram = single_linkage(&data).unwrap();
            let heights = naive_heights(&data);
            assert_eq!(dendrogram.height.len(), heights.len());
            for (got, want) in dendrogram.height.iter().zip(&heights) {
                assert!((got - want).abs() < 1e-9, "n = {n}: {got} contra {want}");
            }
            // Un corte en k grupos tiene k etiquetas distintas.
            for k in 1..=n {
                let labels = dendrogram.cut(k).unwrap();
                let distinct = labels.iter().max().map_or(0, |m| m + 1);
                assert_eq!(distinct, k);
            }
        }
    }
}
