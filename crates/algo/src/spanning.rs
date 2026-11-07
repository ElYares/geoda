//! Arbol de expansion minimo (Kruskal) sobre una lista de aristas no dirigidas con peso.
//!
//! # La idea
//! Se ordenan las aristas por peso (con el `merge_sort` propio, que es **estable**) y se
//! recorren de menor a mayor: una arista entra si une dos componentes distintas, lo que
//! decide el [`UnionFind`]; si los dos extremos ya estan conectados, cerraria un ciclo y se
//! descarta. Al terminar quedan `n - c` aristas, con `c` componentes: un arbol si el grafo es
//! conexo, un bosque si no.
//!
//! # Por que recibe la lista y no un `WeightedGraph`
//! El CSR guarda cada arista no dirigida dos veces y la agrupa por nodo: pierde la posicion
//! que tenia en la lista de entrada, y el desempate por esa posicion es parte del contrato.
//!
//! # Empates, bucles y paralelas
//! Con el mismo peso gana la arista que aparece antes en la lista (el orden es estable), asi
//! que repetir la corrida da el mismo arbol. `-0.0` y `0.0` empatan. Un bucle `(u, u)` nunca
//! une dos componentes, asi que nunca entra; de varias paralelas entra la de menor peso.
//! Los pesos negativos son validos (Kruskal no los necesita no negativos).

use std::cmp::Ordering;

use geoda_core::Error;

use crate::components::UnionFind;
use crate::sort::merge_sort;

/// Resultado de [`kruskal`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpanningForest {
    /// Posiciones, en la lista de entrada, de las aristas elegidas, en el orden en que entraron
    /// (de menor a mayor peso).
    pub edges: Vec<usize>,
    /// Suma de los pesos de las aristas elegidas, en ese mismo orden.
    pub total_weight: f64,
    /// Componentes conexas del grafo; `edges.len() == n - components`.
    pub components: usize,
}

/// Bosque de expansion minimo de un grafo no dirigido de `n` nodos.
///
/// # Errors
/// [`Error::OutOfBounds`] si una arista nombra un nodo `>= n`; [`Error::InvalidArgument`] si
/// algun peso es `NaN`; [`Error::Alloc`] si no hay memoria.
pub fn kruskal(n: usize, edges: &[(usize, usize, f64)]) -> Result<SpanningForest, Error> {
    for (position, &(u, v, weight)) in edges.iter().enumerate() {
        if u >= n || v >= n {
            return Err(Error::OutOfBounds(format!(
                "la arista {position} ({u}, {v}) nombra un nodo en un grafo de {n} nodos"
            )));
        }
        if weight.is_nan() {
            return Err(Error::InvalidArgument(format!(
                "Kruskal no admite pesos NaN: la arista {position} ({u}, {v}) pesa NaN"
            )));
        }
    }

    let mut order: Vec<(f64, usize)> = Vec::new();
    order.try_reserve_exact(edges.len()).map_err(|_| {
        Error::Alloc(format!(
            "no hay memoria para ordenar {} aristas",
            edges.len()
        ))
    })?;
    order.extend(edges.iter().enumerate().map(|(i, &(_, _, w))| (w, i)));
    // Sin NaN, `partial_cmp` es total y trata -0.0 y 0.0 como iguales; la estabilidad desempata.
    merge_sort(&mut order, |a, b| {
        a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal)
    });

    let mut sets = UnionFind::new(n)?;
    let mut chosen = Vec::new();
    let mut total_weight = 0.0;
    for &(weight, position) in &order {
        let (u, v, _) = edges[position];
        if sets.union(u, v)? {
            chosen.push(position);
            total_weight += weight;
        }
    }
    Ok(SpanningForest {
        edges: chosen,
        total_weight,
        components: sets.set_count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_a_connected_graph_with_minimum_weight() {
        // Cuadrado 0-1-2-3 con una diagonal cara: el arbol deja fuera la arista de peso 4.
        let edges = [
            (0, 1, 1.0),
            (1, 2, 2.0),
            (2, 3, 3.0),
            (3, 0, 4.0),
            (0, 2, 10.0),
        ];
        let forest = kruskal(4, &edges).unwrap();
        assert_eq!(forest.edges, [0, 1, 2]);
        assert_eq!(forest.total_weight, 6.0);
        assert_eq!(forest.components, 1);
    }

    #[test]
    fn disconnected_graph_gives_a_forest() {
        // Componentes {0,1,2}, {3,4} y {5}.
        let edges = [(0, 1, 5.0), (1, 2, 1.0), (0, 2, 2.0), (3, 4, 7.0)];
        let forest = kruskal(6, &edges).unwrap();
        assert_eq!(forest.edges, [1, 2, 3]);
        assert_eq!(forest.edges.len(), 6 - forest.components);
        assert_eq!(forest.components, 3);
        assert_eq!(forest.total_weight, 10.0);
    }

    #[test]
    fn ties_go_to_the_earlier_edge_in_the_list() {
        // Triangulo con tres pesos iguales: entran las dos primeras.
        let edges = [(0, 1, 1.0), (1, 2, 1.0), (0, 2, 1.0)];
        assert_eq!(kruskal(3, &edges).unwrap().edges, [0, 1]);
        // Invertida la lista, cambia el arbol y sigue siendo determinista.
        let reversed = [(0, 2, 1.0), (1, 2, 1.0), (0, 1, 1.0)];
        assert_eq!(kruskal(3, &reversed).unwrap().edges, [0, 1]);
        // -0.0 y 0.0 empatan: gana la posicion.
        let zeros = [(0, 1, 0.0), (0, 1, -0.0)];
        assert_eq!(kruskal(2, &zeros).unwrap().edges, [0]);
    }

    #[test]
    fn rejects_nan_and_out_of_range_nodes() {
        assert!(matches!(
            kruskal(2, &[(0, 1, 1.0), (0, 1, f64::NAN)]),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            kruskal(2, &[(0, 2, 1.0)]),
            Err(Error::OutOfBounds(_))
        ));
    }

    #[test]
    fn loops_never_enter_and_the_lightest_parallel_wins() {
        let edges = [(0, 0, -5.0), (0, 1, 9.0), (0, 1, 3.0), (1, 0, 4.0)];
        let forest = kruskal(2, &edges).unwrap();
        assert_eq!(forest.edges, [2]);
        assert_eq!(forest.total_weight, 3.0);
    }

    #[test]
    fn negative_infinite_and_empty_inputs() {
        let forest = kruskal(3, &[(0, 1, -2.0), (1, 2, f64::INFINITY)]).unwrap();
        assert_eq!(forest.edges, [0, 1]);
        assert_eq!(forest.total_weight, f64::INFINITY);

        let empty = kruskal(0, &[]).unwrap();
        assert_eq!((empty.edges.len(), empty.components), (0, 0));
        let isolated = kruskal(4, &[]).unwrap();
        assert_eq!((isolated.edges.len(), isolated.components), (0, 4));
    }

    /// Etiqueta de componente de cada nodo, fusionando etiquetas a la antigua.
    fn labels(n: usize, edges: &[(usize, usize, f64)], subset: u32) -> Vec<usize> {
        let mut label: Vec<usize> = (0..n).collect();
        for (i, &(u, v, _)) in edges.iter().enumerate() {
            if subset >> i & 1 == 1 {
                let (from, to) = (label[u], label[v]);
                for l in &mut label {
                    if *l == from {
                        *l = to;
                    }
                }
            }
        }
        label
    }

    /// Peso minimo de un bosque maximal, probando todos los subconjuntos de aristas: uno es
    /// un bosque maximal si no tiene ciclos y tiene `n - c` aristas.
    fn brute_force_weight(n: usize, edges: &[(usize, usize, f64)]) -> (f64, usize) {
        let mut distinct = labels(n, edges, (1_u32 << edges.len()) - 1);
        distinct.sort_unstable();
        distinct.dedup();
        let components = if edges.is_empty() { n } else { distinct.len() };
        let mut best = f64::INFINITY;
        for subset in 0..(1_u32 << edges.len()) {
            if subset.count_ones() as usize != n - components {
                continue;
            }
            // Sin ciclos: cada arista elegida debe unir etiquetas distintas al agregarla.
            let mut label: Vec<usize> = (0..n).collect();
            let mut acyclic = true;
            let mut weight = 0.0;
            for (i, &(u, v, w)) in edges.iter().enumerate() {
                if subset >> i & 1 == 0 {
                    continue;
                }
                if label[u] == label[v] {
                    acyclic = false;
                    break;
                }
                let (from, to) = (label[u], label[v]);
                for l in &mut label {
                    if *l == from {
                        *l = to;
                    }
                }
                weight += w;
            }
            if acyclic && weight < best {
                best = weight;
            }
        }
        (best, components)
    }

    #[test]
    fn matches_brute_force_on_random_small_graphs() {
        let mut state = 0x2026_u64;
        let mut next = move |bound: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            usize::try_from(state % bound).unwrap()
        };
        for round in 0..300 {
            let n = 1 + next(7);
            let count = next(11);
            let edges: Vec<(usize, usize, f64)> = (0..count)
                .map(|_| {
                    let weight = f64::from(u32::try_from(next(12)).unwrap()) - 3.0;
                    (next(n as u64), next(n as u64), weight)
                })
                .collect();
            let forest = kruskal(n, &edges).unwrap();
            let (best, components) = brute_force_weight(n, &edges);
            assert_eq!(forest.components, components, "ronda {round}: {edges:?}");
            assert_eq!(forest.edges.len(), n - components, "ronda {round}");
            assert_eq!(
                forest.total_weight.to_bits(),
                best.to_bits(),
                "ronda {round}: {edges:?}"
            );
        }
    }
}
