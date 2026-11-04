//! Recorridos de un grafo: anchura (BFS) y profundidad (DFS).
//!
//! Los dos son **iterativos**: la profundidad de una cadena de un millon de nodos no
//! desborda la pila del hilo. Los vecinos se recorren en el orden del CSR (el de la lista de
//! aristas), asi que repetir un recorrido da exactamente el mismo resultado.
//!
//! # Aristas repetidas y bucles
//! No cambian el resultado: un nodo ya visitado no se vuelve a visitar, y un bucle apunta a
//! un nodo ya visitado.
//!
//! # Que se visita
//! Solo los nodos alcanzables desde el origen siguiendo el sentido de las aristas (en un
//! grafo no dirigido, su componente). El resto queda sin visitar: `None` en las tablas y
//! ausente en los ordenes.

use geoda_core::Error;

use crate::graph::Graph;

/// Resultado de [`bfs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bfs {
    /// Nodos en el orden en que se visitaron.
    pub order: Vec<u32>,
    /// Distancia en aristas desde el origen; `None` si el nodo no es alcanzable.
    pub distance: Vec<Option<u32>>,
    /// Nodo desde el que se descubrio cada nodo; `None` en el origen y en los no alcanzables.
    pub parent: Vec<Option<u32>>,
}

/// Resultado de [`dfs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dfs {
    /// Nodos en el orden en que se entro a ellos (preorden).
    pub entry_order: Vec<u32>,
    /// Nodos en el orden en que se salio de ellos, cuando ya se recorrieron todos sus
    /// descendientes (postorden).
    pub exit_order: Vec<u32>,
    /// Nodo desde el que se llego a cada nodo; `None` en el origen y en los no alcanzables.
    pub parent: Vec<Option<u32>>,
}

fn check_source(graph: &Graph, source: usize) -> Result<u32, Error> {
    u32::try_from(source)
        .ok()
        .filter(|&s| (s as usize) < graph.node_count())
        .ok_or_else(|| {
            Error::OutOfBounds(format!(
                "origen {source} en un grafo de {} nodos",
                graph.node_count()
            ))
        })
}

/// Recorrido en anchura desde `source`.
///
/// # Errors
/// [`Error::OutOfBounds`] si `source >= graph.node_count()`.
pub fn bfs(graph: &Graph, source: usize) -> Result<Bfs, Error> {
    let source = check_source(graph, source)?;
    let n = graph.node_count();
    let mut distance: Vec<Option<u32>> = vec![None; n];
    let mut parent: Vec<Option<u32>> = vec![None; n];
    // La propia lista de visita hace de cola: `head` es el siguiente nodo por expandir.
    let mut order = vec![source];
    distance[source as usize] = Some(0);
    let mut head = 0;
    while head < order.len() {
        let u = order[head];
        head += 1;
        let next = distance[u as usize].map_or(0, |d| d + 1);
        for &v in graph.neighbors(u as usize)? {
            if distance[v as usize].is_none() {
                distance[v as usize] = Some(next);
                parent[v as usize] = Some(u);
                order.push(v);
            }
        }
    }
    Ok(Bfs {
        order,
        distance,
        parent,
    })
}

/// Recorrido en profundidad desde `source`.
///
/// # Errors
/// [`Error::OutOfBounds`] si `source >= graph.node_count()`.
pub fn dfs(graph: &Graph, source: usize) -> Result<Dfs, Error> {
    let source = check_source(graph, source)?;
    let n = graph.node_count();
    let mut visited = vec![false; n];
    let mut parent: Vec<Option<u32>> = vec![None; n];
    let mut entry_order = vec![source];
    let mut exit_order = Vec::new();
    visited[source as usize] = true;
    // Pila de (nodo, cuantos vecinos suyos ya se miraron): reemplaza a la recursion.
    let mut stack: Vec<(u32, usize)> = vec![(source, 0)];
    while let Some(&(u, seen)) = stack.last() {
        let neighbors = graph.neighbors(u as usize)?;
        if let Some(&v) = neighbors.get(seen) {
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            if !visited[v as usize] {
                visited[v as usize] = true;
                parent[v as usize] = Some(u);
                entry_order.push(v);
                stack.push((v, 0));
            }
        } else {
            exit_order.push(u);
            stack.pop();
        }
    }
    Ok(Dfs {
        entry_order,
        exit_order,
        parent,
    })
}
