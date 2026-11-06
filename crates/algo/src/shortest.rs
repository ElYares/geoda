//! Caminos minimos desde un nodo con pesos no negativos (Dijkstra).
//!
//! # La idea
//! Se mantiene la mejor distancia conocida a cada nodo y un monticulo ([`Heap`]) con los
//! candidatos `(distancia, nodo)`. Se extrae el menor: como ningun peso es negativo, esa
//! distancia ya no puede mejorar, y se relajan sus aristas (si `dist[u] + w < dist[v]`, `v`
//! mejora y entra al monticulo).
//!
//! # Sin decrease-key
//! El `Heap` no sabe bajar la prioridad de un elemento: al mejorar un nodo se inserta otra
//! entrada y la vieja se descarta al extraerla (su distancia es mayor que la guardada). El
//! monticulo llega a tener como mucho una entrada por arista: O(m log m).
//!
//! # Aristas repetidas, bucles y determinismo
//! Con aristas paralelas gana la de menor peso (la relajacion se queda con la mejor). Un
//! bucle nunca mejora nada. Con `f64` la suma no es asociativa, pero el orden de relajacion
//! es fijo (el del CSR y el desempate por nodo), asi que repetir la corrida da los mismos
//! bits. No se promete igualdad con otra implementacion.

use std::cmp::Ordering;

use geoda_core::Error;

use crate::graph::WeightedGraph;
use crate::heap::Heap;

/// Resultado de [`dijkstra`].
#[derive(Debug, Clone, PartialEq)]
pub struct ShortestPaths {
    /// Nodo de origen.
    pub source: u32,
    /// Distancia minima desde el origen; `+inf` si el nodo no es alcanzable.
    pub distance: Vec<f64>,
    /// Nodo anterior en un camino minimo; `None` en el origen y en los no alcanzables.
    pub parent: Vec<Option<u32>>,
}

/// Distancias minimas desde `source` a todos los nodos.
///
/// # Errors
/// [`Error::OutOfBounds`] si `source >= node_count()`; [`Error::InvalidArgument`] si alguna
/// arista del grafo tiene un peso negativo o `NaN` (se revisan todas antes de correr, no solo
/// las alcanzables); [`Error::Alloc`] si no hay memoria.
pub fn dijkstra(graph: &WeightedGraph, source: usize) -> Result<ShortestPaths, Error> {
    let topology = graph.graph();
    let n = topology.node_count();
    let source = u32::try_from(source)
        .ok()
        .filter(|&s| (s as usize) < n)
        .ok_or_else(|| Error::OutOfBounds(format!("origen {source} en un grafo de {n} nodos")))?;

    for u in 0..n {
        if let Some(&bad) = graph.weights(u)?.iter().find(|w| w.is_nan() || **w < 0.0) {
            return Err(Error::InvalidArgument(format!(
                "Dijkstra no admite pesos negativos ni NaN: la arista que sale del nodo {u} pesa {bad}"
            )));
        }
    }

    let mut distance = vec![f64::INFINITY; n];
    let mut parent: Vec<Option<u32>> = vec![None; n];
    distance[source as usize] = 0.0;

    // Menor distancia primero; a igualdad, menor nodo (orden total y determinista).
    let mut heap =
        Heap::new(|a: &(f64, u32), b: &(f64, u32)| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    heap.push((0.0, source));
    while let Some((dist, u)) = heap.pop() {
        // Entrada obsoleta: el nodo ya se mejoro despues de insertarla.
        if dist.total_cmp(&distance[u as usize]) == Ordering::Greater {
            continue;
        }
        let targets = topology.neighbors(u as usize)?;
        let weights = graph.weights(u as usize)?;
        for (&v, &weight) in targets.iter().zip(weights) {
            let candidate = dist + weight;
            if candidate < distance[v as usize] {
                distance[v as usize] = candidate;
                parent[v as usize] = Some(u);
                heap.push((candidate, v));
            }
        }
    }
    Ok(ShortestPaths {
        source,
        distance,
        parent,
    })
}

/// Camino minimo `origen..=destino` como lista de nodos, o `None` si `target` no es
/// alcanzable (o no existe en el resultado).
///
/// Recibe el resultado completo y no solo `parent` porque con `parent` solo no se distingue
/// el origen de un nodo inalcanzable (los dos tienen `None`).
#[must_use]
pub fn reconstruct_path(paths: &ShortestPaths, target: usize) -> Option<Vec<u32>> {
    if !paths.distance.get(target)?.is_finite() {
        return None;
    }
    let mut path = vec![u32::try_from(target).ok()?];
    while let Some(previous) = paths.parent[*path.last()? as usize] {
        path.push(previous);
    }
    path.reverse();
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, edges: &[(usize, usize, f64)], directed: bool) -> WeightedGraph {
        WeightedGraph::from_edges(n, edges, directed).unwrap()
    }

    #[test]
    fn distances_parents_and_unreachable() {
        // 0 -> 1 (4), 0 -> 2 (1), 2 -> 1 (2), 1 -> 3 (1); el 4 queda aislado.
        let g = graph(
            5,
            &[(0, 1, 4.0), (0, 2, 1.0), (2, 1, 2.0), (1, 3, 1.0)],
            true,
        );
        let paths = dijkstra(&g, 0).unwrap();
        assert_eq!(paths.distance[..4], [0.0, 3.0, 1.0, 4.0]);
        assert_eq!(paths.distance[4], f64::INFINITY);
        assert_eq!(paths.parent, [None, Some(2), Some(0), Some(1), None]);
        assert_eq!(reconstruct_path(&paths, 3), Some(vec![0, 2, 1, 3]));
        assert_eq!(reconstruct_path(&paths, 0), Some(vec![0]));
        assert_eq!(reconstruct_path(&paths, 4), None);
        assert_eq!(reconstruct_path(&paths, 99), None);
    }

    #[test]
    fn rejects_negative_and_nan_weights_even_if_unreachable() {
        for bad in [-1.0, f64::NAN, f64::NEG_INFINITY] {
            let g = graph(3, &[(0, 1, 1.0), (2, 2, bad)], true);
            assert!(matches!(dijkstra(&g, 0), Err(Error::InvalidArgument(_))));
        }
        // -0.0 no es negativo.
        let g = graph(2, &[(0, 1, -0.0)], true);
        assert_eq!(dijkstra(&g, 0).unwrap().distance[1], 0.0);
    }

    #[test]
    fn source_out_of_bounds() {
        let g = graph(2, &[], true);
        assert!(matches!(dijkstra(&g, 2), Err(Error::OutOfBounds(_))));
        assert!(matches!(
            dijkstra(&g, usize::MAX),
            Err(Error::OutOfBounds(_))
        ));
    }

    #[test]
    fn parallel_edges_use_the_lightest_and_loops_are_ignored() {
        let g = graph(
            2,
            &[(0, 1, 5.0), (0, 1, 2.0), (0, 1, 7.0), (1, 1, 0.0)],
            true,
        );
        assert_eq!(dijkstra(&g, 0).unwrap().distance, [0.0, 2.0]);
    }

    #[test]
    fn undirected_goes_both_ways() {
        let g = graph(3, &[(0, 1, 2.0), (2, 1, 3.0)], false);
        let paths = dijkstra(&g, 0).unwrap();
        assert_eq!(paths.distance, [0.0, 2.0, 5.0]);
        assert_eq!(reconstruct_path(&paths, 2), Some(vec![0, 1, 2]));
    }

    #[test]
    fn empty_graph_has_no_valid_source_and_single_node_works() {
        assert!(dijkstra(&graph(0, &[], true), 0).is_err());
        let paths = dijkstra(&graph(1, &[], true), 0).unwrap();
        assert_eq!(paths.distance, [0.0]);
    }

    /// Bellman-Ford como oraculo: relaja todas las aristas n - 1 veces.
    fn bellman_ford(n: usize, edges: &[(usize, usize, f64)], source: usize) -> Vec<f64> {
        let mut distance = vec![f64::INFINITY; n];
        distance[source] = 0.0;
        for _ in 0..n {
            for &(u, v, w) in edges {
                if distance[u] + w < distance[v] {
                    distance[v] = distance[u] + w;
                }
            }
        }
        distance
    }

    #[test]
    fn matches_bellman_ford_on_random_graphs_with_integer_weights() {
        let mut state = 0x2026_u64;
        let mut next = move |bound: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            usize::try_from(state % bound).unwrap()
        };
        for round in 0..200 {
            let n = 1 + next(12);
            let count = next(40);
            let edges: Vec<(usize, usize, f64)> = (0..count)
                .map(|_| {
                    (
                        next(n as u64),
                        next(n as u64),
                        f64::from(u32::try_from(next(20)).unwrap()),
                    )
                })
                .collect();
            let directed = round % 2 == 0;
            let source = next(n as u64);
            let mut both = edges.clone();
            if !directed {
                both.extend(edges.iter().map(|&(u, v, w)| (v, u, w)));
            }
            let expected = bellman_ford(n, &both, source);
            let got = dijkstra(&graph(n, &edges, directed), source).unwrap();
            let same = got
                .distance
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.to_bits() == b.to_bits());
            assert!(same, "ronda {round}: {:?} != {expected:?}", got.distance);
        }
    }
}
