//! Pruebas de BFS y DFS (HU-034).

use std::collections::VecDeque;

use geoda_algo::{Graph, bfs, dfs};
use geoda_core::Error;

fn graph(n: usize, edges: &[(usize, usize)], directed: bool) -> Graph {
    Graph::from_edges(n, edges, directed).unwrap()
}

#[test]
fn bfs_gives_order_distance_and_parent() {
    //   0 -> 1 -> 3
    //   0 -> 2 -> 3
    let g = graph(5, &[(0, 1), (0, 2), (1, 3), (2, 3)], true);
    let r = bfs(&g, 0).unwrap();
    assert_eq!(r.order, [0, 1, 2, 3]);
    assert_eq!(r.distance, [Some(0), Some(1), Some(1), Some(2), None]);
    assert_eq!(r.parent, [None, Some(0), Some(0), Some(1), None]);
}

#[test]
fn dfs_gives_entry_exit_and_parent() {
    let g = graph(5, &[(0, 1), (0, 2), (1, 3), (2, 3)], true);
    let r = dfs(&g, 0).unwrap();
    // Entra a 0, baja por 1 hasta 3, sale de 3, de 1; luego 2 (3 ya esta visitado).
    assert_eq!(r.entry_order, [0, 1, 3, 2]);
    assert_eq!(r.exit_order, [3, 1, 2, 0]);
    assert_eq!(r.parent, [None, Some(0), Some(0), Some(1), None]);
}

#[test]
fn a_source_out_of_range_is_an_error() {
    let g = graph(3, &[(0, 1)], true);
    assert!(matches!(bfs(&g, 3), Err(Error::OutOfBounds(_))));
    assert!(matches!(dfs(&g, 3), Err(Error::OutOfBounds(_))));
    assert!(matches!(bfs(&g, usize::MAX), Err(Error::OutOfBounds(_))));
    let empty = graph(0, &[], true);
    assert!(matches!(bfs(&empty, 0), Err(Error::OutOfBounds(_))));
}

#[test]
fn only_reachable_nodes_are_visited() {
    // Dos componentes: {0, 1} y {2, 3}.
    let g = graph(4, &[(0, 1), (2, 3)], false);
    let b = bfs(&g, 0).unwrap();
    assert_eq!(b.order, [0, 1]);
    assert_eq!(b.distance, [Some(0), Some(1), None, None]);
    let d = dfs(&g, 2).unwrap();
    assert_eq!(d.entry_order, [2, 3]);
    assert_eq!(d.exit_order, [3, 2]);
    assert_eq!(d.parent, [None, None, None, Some(2)]);
}

#[test]
fn direction_matters_only_in_directed_graphs() {
    let edges = [(0, 1), (1, 2)];
    let directed = graph(3, &edges, true);
    assert_eq!(bfs(&directed, 2).unwrap().order, [2]);
    let undirected = graph(3, &edges, false);
    let r = bfs(&undirected, 2).unwrap();
    assert_eq!(r.order, [2, 1, 0]);
    assert_eq!(r.distance, [Some(2), Some(1), Some(0)]);
}

#[test]
fn loops_and_repeated_edges_do_not_change_the_result() {
    let plain = graph(4, &[(0, 1), (1, 2), (2, 3)], true);
    let noisy = graph(
        4,
        &[
            (0, 0),
            (0, 1),
            (0, 1),
            (1, 1),
            (1, 2),
            (1, 2),
            (2, 3),
            (3, 3),
        ],
        true,
    );
    assert_eq!(bfs(&plain, 0).unwrap(), bfs(&noisy, 0).unwrap());
    assert_eq!(dfs(&plain, 0).unwrap(), dfs(&noisy, 0).unwrap());
}

#[test]
fn repeating_a_traversal_gives_the_same_result() {
    let g = graph(6, &[(0, 3), (0, 1), (3, 4), (1, 4), (4, 5), (5, 0)], true);
    assert_eq!(bfs(&g, 0).unwrap(), bfs(&g, 0).unwrap());
    assert_eq!(dfs(&g, 0).unwrap(), dfs(&g, 0).unwrap());
}

/// Una cadena de un millon de nodos: con recursion la pila no aguantaria.
#[test]
fn a_chain_of_a_million_nodes_does_not_overflow_the_stack() {
    let n = 1_000_000;
    let edges: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, i + 1)).collect();
    let g = graph(n, &edges, true);
    let d = dfs(&g, 0).unwrap();
    assert_eq!(d.entry_order.len(), n);
    assert_eq!(d.exit_order[0], u32::try_from(n - 1).unwrap());
    assert_eq!(d.exit_order[n - 1], 0);
    let b = bfs(&g, 0).unwrap();
    assert_eq!(b.distance[n - 1], Some(u32::try_from(n - 1).unwrap()));
}

/// Generador con semilla (xorshift64).
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n as u64).unwrap()
    }
}

/// Oraculo de BFS con la cola de la biblioteca estandar.
fn naive_bfs_distance(adjacency: &[Vec<usize>], source: usize) -> Vec<Option<u32>> {
    let mut distance = vec![None; adjacency.len()];
    distance[source] = Some(0);
    let mut queue = VecDeque::from([source]);
    while let Some(u) = queue.pop_front() {
        for &v in &adjacency[u] {
            if distance[v].is_none() {
                distance[v] = distance[u].map(|d| d + 1);
                queue.push_back(v);
            }
        }
    }
    distance
}

/// Oraculo de DFS: la version recursiva de siempre.
fn naive_dfs(
    adjacency: &[Vec<usize>],
    u: usize,
    seen: &mut [bool],
    entry: &mut Vec<u32>,
    exit: &mut Vec<u32>,
) {
    seen[u] = true;
    entry.push(u32::try_from(u).unwrap());
    for &v in &adjacency[u] {
        if !seen[v] {
            naive_dfs(adjacency, v, seen, entry, exit);
        }
    }
    exit.push(u32::try_from(u).unwrap());
}

#[test]
fn traversals_match_naive_versions_on_random_graphs() {
    let mut rng = Rng(0x5EED_0034);
    for round in 0..200 {
        let nodes = 1 + rng.below(25);
        let count = rng.below(80);
        let edges: Vec<(usize, usize)> = (0..count)
            .map(|_| (rng.below(nodes), rng.below(nodes)))
            .collect();
        for directed in [true, false] {
            let mut adjacency = vec![Vec::new(); nodes];
            for &(from, to) in &edges {
                adjacency[from].push(to);
                if !directed {
                    adjacency[to].push(from);
                }
            }
            let built = graph(nodes, &edges, directed);
            let source = rng.below(nodes);
            let ctx = format!("ronda {round}, dirigido = {directed}, origen = {source}");

            let breadth = bfs(&built, source).unwrap();
            assert_eq!(
                breadth.distance,
                naive_bfs_distance(&adjacency, source),
                "{ctx}"
            );
            assert_eq!(
                breadth.order.len(),
                breadth.distance.iter().flatten().count(),
                "{ctx}"
            );
            for &v in &breadth.order {
                if let Some(p) = breadth.parent[v as usize] {
                    assert_eq!(
                        breadth.distance[v as usize],
                        breadth.distance[p as usize].map(|d| d + 1),
                        "{ctx}"
                    );
                }
            }

            let depth = dfs(&built, source).unwrap();
            let (mut entry, mut exit) = (Vec::new(), Vec::new());
            naive_dfs(
                &adjacency,
                source,
                &mut vec![false; nodes],
                &mut entry,
                &mut exit,
            );
            assert_eq!(depth.entry_order, entry, "{ctx}");
            assert_eq!(depth.exit_order, exit, "{ctx}");
        }
    }
}
