//! Pruebas del grafo CSR (HU-033).

use geoda_algo::{Graph, WeightedGraph};
use geoda_core::Error;

fn neighbors(g: &Graph, u: usize) -> Vec<u32> {
    g.neighbors(u).unwrap().to_vec()
}

#[test]
fn directed_graph_lists_the_targets_of_each_node() {
    let g = Graph::from_edges(4, &[(0, 1), (0, 2), (2, 3), (1, 2)], true).unwrap();
    assert_eq!((g.node_count(), g.edge_count()), (4, 4));
    assert!(g.is_directed());
    assert_eq!(neighbors(&g, 0), [1, 2]);
    assert_eq!(neighbors(&g, 1), [2]);
    assert_eq!(neighbors(&g, 2), [3]);
    assert_eq!(neighbors(&g, 3), Vec::<u32>::new());
}

#[test]
fn undirected_graph_stores_each_edge_both_ways() {
    let g = Graph::from_edges(4, &[(0, 1), (0, 2), (2, 3)], false).unwrap();
    assert_eq!(g.edge_count(), 3);
    assert!(!g.is_directed());
    assert_eq!(neighbors(&g, 0), [1, 2]);
    assert_eq!(neighbors(&g, 1), [0]);
    assert_eq!(neighbors(&g, 2), [0, 3]);
    assert_eq!(neighbors(&g, 3), [2]);
    assert_eq!(
        (0..4).map(|u| g.degree(u).unwrap()).collect::<Vec<_>>(),
        [2, 1, 2, 1]
    );
}

#[test]
fn weights_are_aligned_with_the_neighbors() {
    let g = WeightedGraph::from_edges(3, &[(0, 1, 1.5), (0, 2, -2.0), (2, 1, 7.0)], true).unwrap();
    assert_eq!(g.graph().neighbors(0).unwrap(), [1, 2]);
    assert_eq!(g.weights(0).unwrap(), [1.5, -2.0]);
    assert_eq!(g.weights(2).unwrap(), [7.0]);
    assert_eq!(g.weights(1).unwrap().len(), 0);
    // En un no dirigido el peso viaja con los dos sentidos.
    let g = WeightedGraph::from_edges(3, &[(0, 1, 4.0), (1, 2, 9.0)], false).unwrap();
    assert_eq!(g.graph().neighbors(1).unwrap(), [0, 2]);
    assert_eq!(g.weights(1).unwrap(), [4.0, 9.0]);
    assert_eq!(g.weights(2).unwrap(), [9.0]);
}

#[test]
fn a_node_out_of_range_is_an_error_not_a_panic() {
    for directed in [true, false] {
        assert!(matches!(
            Graph::from_edges(3, &[(0, 3)], directed),
            Err(Error::OutOfBounds(_))
        ));
        assert!(matches!(
            Graph::from_edges(3, &[(5, 0)], directed),
            Err(Error::OutOfBounds(_))
        ));
        assert!(matches!(
            WeightedGraph::from_edges(3, &[(0, 1, 1.0), (1, 9, 1.0)], directed),
            Err(Error::OutOfBounds(_))
        ));
    }
    let g = Graph::from_edges(3, &[(0, 1)], true).unwrap();
    assert!(matches!(g.neighbors(3), Err(Error::OutOfBounds(_))));
    assert!(matches!(g.degree(3), Err(Error::OutOfBounds(_))));
    let w = WeightedGraph::from_edges(3, &[(0, 1, 1.0)], true).unwrap();
    assert!(matches!(w.weights(3), Err(Error::OutOfBounds(_))));
}

#[test]
fn empty_graphs_are_valid() {
    let g = Graph::from_edges(0, &[], true).unwrap();
    assert_eq!((g.node_count(), g.edge_count()), (0, 0));
    assert!(matches!(g.neighbors(0), Err(Error::OutOfBounds(_))));
    let g = Graph::from_edges(5, &[], false).unwrap();
    assert_eq!((g.node_count(), g.edge_count()), (5, 0));
    assert!((0..5).all(|u| g.degree(u).unwrap() == 0));
}

#[test]
fn repeated_edges_and_loops_are_kept() {
    let g = Graph::from_edges(2, &[(0, 1), (0, 1), (1, 1)], true).unwrap();
    assert_eq!(neighbors(&g, 0), [1, 1]);
    assert_eq!(neighbors(&g, 1), [1]);
    // En un no dirigido el bucle aparece dos veces y suma 2 al grado.
    let g = Graph::from_edges(2, &[(0, 1), (1, 1)], false).unwrap();
    assert_eq!(neighbors(&g, 1), [0, 1, 1]);
    assert_eq!(g.degree(1).unwrap(), 3);
    assert_eq!(g.edge_count(), 2);
}

#[test]
fn neighbors_follow_the_order_of_the_edge_list() {
    let g = Graph::from_edges(4, &[(0, 3), (0, 1), (0, 2), (0, 1)], true).unwrap();
    assert_eq!(neighbors(&g, 0), [3, 1, 2, 1]);
    let again = Graph::from_edges(4, &[(0, 3), (0, 1), (0, 2), (0, 1)], true).unwrap();
    assert_eq!(g, again);
}

#[test]
fn more_nodes_than_u32_is_an_error_without_allocating() {
    let n = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
    for directed in [true, false] {
        assert!(matches!(
            Graph::from_edges(n, &[], directed),
            Err(Error::OutOfBounds(_))
        ));
        assert!(matches!(
            WeightedGraph::from_edges(n, &[], directed),
            Err(Error::OutOfBounds(_))
        ));
    }
}

/// Generador con semilla (xorshift64), para no depender de una biblioteca.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n as u64).unwrap()
    }
}

/// Las listas de adyacencia ingenuas son el oraculo del CSR.
#[test]
fn csr_matches_naive_adjacency_lists_on_random_graphs() {
    let mut rng = Rng(0x5EED_0033);
    for round in 0..200 {
        let n = 1 + rng.below(30);
        let m = rng.below(120);
        let edges: Vec<(usize, usize)> = (0..m).map(|_| (rng.below(n), rng.below(n))).collect();
        for directed in [true, false] {
            let mut naive: Vec<Vec<u32>> = vec![Vec::new(); n];
            for &(u, v) in &edges {
                naive[u].push(u32::try_from(v).unwrap());
                if !directed {
                    naive[v].push(u32::try_from(u).unwrap());
                }
            }
            let g = Graph::from_edges(n, &edges, directed).unwrap();
            for (u, expected) in naive.iter().enumerate() {
                assert_eq!(neighbors(&g, u), *expected, "ronda {round}, nodo {u}");
                assert_eq!(g.degree(u).unwrap(), expected.len());
            }
            let total: usize = (0..n).map(|u| g.degree(u).unwrap()).sum();
            assert_eq!(total, if directed { m } else { 2 * m });
        }
    }
}
