//! Pruebas de union-find, componentes y orden topologico (HU-035).

use geoda_algo::{Graph, UnionFind, components, topological_order};
use geoda_core::Error;

fn graph(n: usize, edges: &[(usize, usize)], directed: bool) -> Graph {
    Graph::from_edges(n, edges, directed).unwrap()
}

#[test]
fn union_find_joins_and_counts_sets() {
    let mut sets = UnionFind::new(6).unwrap();
    assert_eq!((sets.len(), sets.set_count()), (6, 6));
    assert!(sets.union(0, 1).unwrap());
    assert!(sets.union(2, 3).unwrap());
    assert!(!sets.union(1, 0).unwrap());
    assert_eq!(sets.set_count(), 4);
    assert!(sets.same(0, 1).unwrap());
    assert!(!sets.same(1, 2).unwrap());
    assert!(sets.union(1, 3).unwrap());
    assert!(sets.same(0, 2).unwrap());
    assert_eq!(sets.set_count(), 3);
    assert!(UnionFind::new(0).unwrap().is_empty());
}

#[test]
fn union_find_rejects_elements_out_of_range() {
    let mut sets = UnionFind::new(3).unwrap();
    assert!(matches!(sets.find(3), Err(Error::OutOfBounds(_))));
    assert!(matches!(sets.union(0, 3), Err(Error::OutOfBounds(_))));
    assert!(matches!(sets.same(9, 0), Err(Error::OutOfBounds(_))));
    let too_many = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
    assert!(matches!(
        UnionFind::new(too_many),
        Err(Error::OutOfBounds(_))
    ));
}

/// Un millon de uniones encadenadas: ni recursion ni caminos largos.
#[test]
fn a_million_chained_unions_do_not_overflow_the_stack() {
    let n = 1_000_000;
    let mut sets = UnionFind::new(n).unwrap();
    for i in 0..n - 1 {
        sets.union(i, i + 1).unwrap();
    }
    assert_eq!(sets.set_count(), 1);
    assert!(sets.same(0, n - 1).unwrap());
    // Despues de buscar, el camino queda comprimido: la raiz de cualquiera es la misma.
    let root = sets.find(0).unwrap();
    assert!((0..n).step_by(9_973).all(|i| sets.find(i).unwrap() == root));
}

#[test]
fn components_are_numbered_by_first_node() {
    // {0, 3}, {1, 2, 4}, {5}
    let g = graph(6, &[(3, 0), (4, 2), (1, 2)], false);
    let c = components(&g).unwrap();
    assert_eq!(c.id, [0, 1, 1, 0, 1, 2]);
    assert_eq!(c.count, 3);
}

#[test]
fn components_of_a_directed_graph_ignore_the_direction() {
    let g = graph(4, &[(0, 1), (2, 1), (3, 3)], true);
    let c = components(&g).unwrap();
    assert_eq!((c.id, c.count), (vec![0, 0, 0, 1], 2));
}

#[test]
fn components_of_empty_graphs() {
    let c = components(&graph(0, &[], false)).unwrap();
    assert_eq!((c.id.len(), c.count), (0, 0));
    let c = components(&graph(3, &[], false)).unwrap();
    assert_eq!((c.id, c.count), (vec![0, 1, 2], 3));
}

#[test]
fn topological_order_puts_every_source_before_its_target() {
    let edges = [(5, 2), (5, 0), (4, 0), (4, 1), (2, 3), (3, 1)];
    let order = topological_order(&graph(6, &edges, true)).unwrap();
    assert_eq!(order.len(), 6);
    let position = |node: usize| order.iter().position(|&x| x as usize == node).unwrap();
    for (from, to) in edges {
        assert!(position(from) < position(to), "{from} -> {to} en {order:?}");
    }
}

#[test]
fn ties_go_to_the_smallest_index() {
    // Sin aristas todos estan disponibles: sale en orden de indice.
    assert_eq!(
        topological_order(&graph(4, &[], true)).unwrap(),
        [0, 1, 2, 3]
    );
    // 3 y 1 quedan libres a la vez tras sacar 0; gana 1.
    assert_eq!(
        topological_order(&graph(4, &[(0, 3), (0, 1), (1, 2)], true)).unwrap(),
        [0, 1, 2, 3]
    );
}

#[test]
fn a_cycle_is_an_error_not_a_partial_order() {
    let cycle = graph(4, &[(0, 1), (1, 2), (2, 1), (2, 3)], true);
    let Err(Error::InvalidArgument(message)) = topological_order(&cycle) else {
        panic!("se esperaba InvalidArgument");
    };
    assert!(message.contains("ciclo"), "{message}");
    // Un bucle tambien es un ciclo.
    assert!(matches!(
        topological_order(&graph(2, &[(0, 1), (1, 1)], true)),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn an_undirected_graph_has_no_topological_order() {
    assert!(matches!(
        topological_order(&graph(2, &[(0, 1)], false)),
        Err(Error::InvalidArgument(_))
    ));
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

/// Oraculo de componentes: etiquetado por recorrido, ignorando el sentido.
fn naive_components(nodes: usize, edges: &[(usize, usize)]) -> (Vec<u32>, usize) {
    let mut adjacency = vec![Vec::new(); nodes];
    for &(from, to) in edges {
        adjacency[from].push(to);
        adjacency[to].push(from);
    }
    let mut label: Vec<Option<u32>> = vec![None; nodes];
    let mut count = 0_u32;
    for start in 0..nodes {
        if label[start].is_some() {
            continue;
        }
        label[start] = Some(count);
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for &next in &adjacency[node] {
                if label[next].is_none() {
                    label[next] = Some(count);
                    stack.push(next);
                }
            }
        }
        count += 1;
    }
    (label.into_iter().flatten().collect(), count as usize)
}

/// Oraculo de ciclos: DFS de tres colores.
fn has_cycle(nodes: usize, edges: &[(usize, usize)]) -> bool {
    fn visit(node: usize, adjacency: &[Vec<usize>], color: &mut [u8]) -> bool {
        color[node] = 1;
        for &next in &adjacency[node] {
            if color[next] == 1 || (color[next] == 0 && visit(next, adjacency, color)) {
                return true;
            }
        }
        color[node] = 2;
        false
    }
    let mut adjacency = vec![Vec::new(); nodes];
    for &(from, to) in edges {
        adjacency[from].push(to);
    }
    let mut color = vec![0_u8; nodes];
    (0..nodes).any(|node| color[node] == 0 && visit(node, &adjacency, &mut color))
}

#[test]
fn components_match_a_naive_labeling_on_random_graphs() {
    let mut rng = Rng(0x5EED_0035);
    for round in 0..200 {
        let nodes = 1 + rng.below(30);
        let count = rng.below(40);
        let edges: Vec<(usize, usize)> = (0..count)
            .map(|_| (rng.below(nodes), rng.below(nodes)))
            .collect();
        let (expected_id, expected_count) = naive_components(nodes, &edges);
        for directed in [true, false] {
            let got = components(&graph(nodes, &edges, directed)).unwrap();
            assert_eq!(got.id, expected_id, "ronda {round}, dirigido = {directed}");
            assert_eq!(got.count, expected_count, "ronda {round}");
        }
    }
}

#[test]
fn topological_order_agrees_with_cycle_detection_on_random_graphs() {
    let mut rng = Rng(0x5EED_1035);
    for round in 0..300 {
        let nodes = 1 + rng.below(12);
        let count = rng.below(25);
        let edges: Vec<(usize, usize)> = (0..count)
            .map(|_| (rng.below(nodes), rng.below(nodes)))
            .collect();
        let result = topological_order(&graph(nodes, &edges, true));
        if has_cycle(nodes, &edges) {
            assert!(
                matches!(result, Err(Error::InvalidArgument(_))),
                "ronda {round}: {edges:?}"
            );
        } else {
            let order = result.unwrap();
            let mut seen: Vec<u32> = order.clone();
            seen.sort_unstable();
            assert_eq!(seen, (0..u32::try_from(nodes).unwrap()).collect::<Vec<_>>());
            let position = |node: usize| order.iter().position(|&x| x as usize == node).unwrap();
            for &(from, to) in &edges {
                assert!(position(from) < position(to), "ronda {round}: {edges:?}");
            }
        }
    }
}
