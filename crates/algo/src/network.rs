//! Un grafo listo para consultar desde un binding: topologia, pesos opcionales y la lista de
//! aristas original.
//!
//! La capi y Python envuelven este tipo, asi que la semantica de cada consulta (que error da
//! un grafo sin pesos, que pasa con un dirigido) vive aqui y se prueba una sola vez.
//!
//! # Por que guarda la lista de aristas
//! El CSR pierde la posicion original de cada arista, y [`kruskal`] desempata por ella. Con
//! pesos se conserva la lista (una copia de `m` aristas); sin pesos no hace falta.

use geoda_core::Error;

use crate::components::{Components, components, topological_order};
use crate::graph::{Graph, WeightedGraph};
use crate::shortest::{ShortestPaths, dijkstra};
use crate::spanning::{SpanningForest, kruskal};
use crate::traverse::{Bfs, bfs};

/// Un grafo con o sin pesos.
#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Plain(Graph),
    Weighted {
        graph: WeightedGraph,
        /// Aristas tal como se dieron, para Kruskal.
        edges: Vec<(usize, usize, f64)>,
    },
}

impl Network {
    /// Construye el grafo de `n` nodos. Con `weights`, debe haber uno por arista.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `weights` y `edges` miden distinto; los de
    /// [`Graph::from_edges`] (nodo fuera de rango, grafo demasiado grande, sin memoria).
    pub fn new(
        n: usize,
        edges: &[(usize, usize)],
        weights: Option<&[f64]>,
        directed: bool,
    ) -> Result<Self, Error> {
        let kind = match weights {
            None => Kind::Plain(Graph::from_edges(n, edges, directed)?),
            Some(weights) => {
                if weights.len() != edges.len() {
                    return Err(Error::LengthMismatch(format!(
                        "{} aristas y {} pesos",
                        edges.len(),
                        weights.len()
                    )));
                }
                let mut with_weights = Vec::new();
                with_weights.try_reserve_exact(edges.len()).map_err(|_| {
                    Error::Alloc(format!("no hay memoria para {} aristas", edges.len()))
                })?;
                with_weights.extend(edges.iter().zip(weights).map(|(&(u, v), &w)| (u, v, w)));
                Kind::Weighted {
                    graph: WeightedGraph::from_edges(n, &with_weights, directed)?,
                    edges: with_weights,
                }
            }
        };
        Ok(Self { kind })
    }

    /// El grafo sin pesos: nodos, aristas y vecinos.
    #[must_use]
    pub fn graph(&self) -> &Graph {
        match &self.kind {
            Kind::Plain(graph) => graph,
            Kind::Weighted { graph, .. } => graph.graph(),
        }
    }

    /// `true` si cada arista lleva un peso.
    #[must_use]
    pub fn is_weighted(&self) -> bool {
        matches!(self.kind, Kind::Weighted { .. })
    }

    /// Recorrido en anchura desde `source`.
    ///
    /// # Errors
    /// Los de [`bfs`].
    pub fn bfs(&self, source: usize) -> Result<Bfs, Error> {
        bfs(self.graph(), source)
    }

    /// Componentes conexas (debiles en un dirigido).
    ///
    /// # Errors
    /// Los de [`components`].
    pub fn components(&self) -> Result<Components, Error> {
        components(self.graph())
    }

    /// Orden topologico de un grafo dirigido sin ciclos.
    ///
    /// # Errors
    /// Los de [`topological_order`].
    pub fn topological_order(&self) -> Result<Vec<u32>, Error> {
        topological_order(self.graph())
    }

    /// Caminos minimos desde `source`.
    ///
    /// # Errors
    /// [`Error::TypeMismatch`] si el grafo no tiene pesos; los de [`dijkstra`].
    pub fn dijkstra(&self, source: usize) -> Result<ShortestPaths, Error> {
        match &self.kind {
            Kind::Weighted { graph, .. } => dijkstra(graph, source),
            Kind::Plain(_) => Err(Error::TypeMismatch(
                "Dijkstra necesita un grafo con pesos".into(),
            )),
        }
    }

    /// Bosque de expansion minimo de un grafo **no dirigido** con pesos.
    ///
    /// # Errors
    /// [`Error::TypeMismatch`] si no tiene pesos; [`Error::InvalidArgument`] si es dirigido
    /// (el arbol de expansion minimo no esta definido igual para aristas con sentido); los de
    /// [`kruskal`].
    pub fn mst(&self) -> Result<SpanningForest, Error> {
        match &self.kind {
            Kind::Weighted { graph, edges } => {
                if graph.graph().is_directed() {
                    return Err(Error::InvalidArgument(
                        "el arbol de expansion minimo pide un grafo no dirigido".into(),
                    ));
                }
                kruskal(graph.graph().node_count(), edges)
            }
            Kind::Plain(_) => Err(Error::TypeMismatch(
                "el arbol de expansion minimo necesita un grafo con pesos".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_and_plain_answer_the_same_traversals() {
        let edges = [(0, 1), (1, 2), (3, 3)];
        let plain = Network::new(4, &edges, None, false).unwrap();
        let weighted = Network::new(4, &edges, Some(&[1.0, 2.0, 3.0]), false).unwrap();
        assert!(!plain.is_weighted() && weighted.is_weighted());
        assert_eq!(plain.bfs(0).unwrap(), weighted.bfs(0).unwrap());
        assert_eq!(plain.components().unwrap(), weighted.components().unwrap());
        assert_eq!(plain.components().unwrap().count, 2);
    }

    #[test]
    fn weights_must_match_the_edges() {
        assert!(matches!(
            Network::new(2, &[(0, 1)], Some(&[]), true),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            Network::new(2, &[(0, 2)], None, true),
            Err(Error::OutOfBounds(_))
        ));
    }

    #[test]
    fn weighted_queries_need_weights() {
        let plain = Network::new(2, &[(0, 1)], None, false).unwrap();
        assert!(matches!(plain.dijkstra(0), Err(Error::TypeMismatch(_))));
        assert!(matches!(plain.mst(), Err(Error::TypeMismatch(_))));
    }

    #[test]
    fn mst_keeps_the_input_positions_and_rejects_directed() {
        let edges = [(2, 1), (1, 0), (0, 2)];
        let weights = [1.0, 1.0, 1.0];
        let tree = Network::new(3, &edges, Some(&weights), false).unwrap();
        // Con empate gana la posicion de entrada, no el orden del CSR.
        assert_eq!(tree.mst().unwrap().edges, [0, 1]);
        let directed = Network::new(3, &edges, Some(&weights), true).unwrap();
        assert!(matches!(directed.mst(), Err(Error::InvalidArgument(_))));
        assert_eq!(directed.dijkstra(2).unwrap().distance, [2.0, 1.0, 0.0]);
    }

    #[test]
    fn topological_order_goes_through() {
        let dag = Network::new(3, &[(2, 1), (1, 0)], None, true).unwrap();
        assert_eq!(dag.topological_order().unwrap(), [2, 1, 0]);
    }
}
