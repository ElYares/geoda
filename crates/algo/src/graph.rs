//! Grafos en formato CSR (filas comprimidas), la base de la fase 4.
//!
//! # El formato
//! `offsets` tiene `n + 1` entradas y los vecinos del nodo `u` son
//! `targets[offsets[u]..offsets[u + 1]]`. No hay un puntero por arista: dos arreglos planos
//! que se recorren en orden. Los indices son `u32` (la mitad de memoria que `usize`), asi que
//! un grafo tiene como mucho `u32::MAX` nodos y `u32::MAX` aristas dirigidas; pasarse es
//! [`Error::OutOfBounds`], no un desbordamiento.
//!
//! # Que se conserva
//! Las aristas repetidas y los bucles (`u == v`) se guardan tal cual (multigrafo): cada
//! algoritmo documenta como los trata. Los vecinos de un nodo salen en el orden en que
//! aparecen en la lista de aristas, asi que construir dos veces la misma lista da el mismo
//! grafo.
//!
//! # No dirigidos
//! Cada arista `(u, v)` se guarda en los dos sentidos. Un bucle `(u, u)` aparece dos veces
//! en la lista de `u` y suma 2 a su grado (la convencion habitual).

use geoda_core::Error;

/// Grafo sin pesos en formato CSR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    offsets: Vec<u32>,
    targets: Vec<u32>,
    /// Aristas tal como se dieron (una por pareja, aunque sea no dirigido).
    edges: usize,
    directed: bool,
}

/// Grafo con un peso `f64` por arista en formato CSR.
///
/// Es un tipo distinto de [`Graph`] para que un algoritmo que necesita pesos no pueda recibir
/// un grafo sin ellos. Los pesos se aceptan tal cual (incluso negativos o `NaN`): los
/// algoritmos que no los admiten los rechazan al correr.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedGraph {
    graph: Graph,
    /// Alineados con los `targets` del grafo.
    weights: Vec<f64>,
}

impl Graph {
    /// Construye un grafo de `n` nodos desde una lista de aristas `(origen, destino)`.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `n` o el numero de aristas dirigidas no caben en `u32`, o si
    /// una arista nombra un nodo `>= n`; [`Error::Alloc`] si no hay memoria.
    pub fn from_edges(n: usize, edges: &[(usize, usize)], directed: bool) -> Result<Self, Error> {
        let csr = build(n, edges.len(), directed, |i| {
            let (u, v) = edges[i];
            (u, v, ())
        })?;
        Ok(Self {
            offsets: csr.offsets,
            targets: csr.targets,
            edges: edges.len(),
            directed,
        })
    }

    /// Cantidad de nodos.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.offsets.len() - 1
    }

    /// Cantidad de aristas tal como se dieron: en un grafo no dirigido cada pareja cuenta una
    /// vez.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges
    }

    /// `true` si las aristas tienen sentido.
    #[must_use]
    pub fn is_directed(&self) -> bool {
        self.directed
    }

    /// Destinos de las aristas que salen de `u`, en el orden de la lista de aristas.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `u >= node_count()`.
    pub fn neighbors(&self, u: usize) -> Result<&[u32], Error> {
        let (from, to) = self.range(u)?;
        Ok(&self.targets[from..to])
    }

    /// Cantidad de aristas que salen de `u` (en un no dirigido, las que tocan a `u`; un bucle
    /// cuenta 2).
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `u >= node_count()`.
    pub fn degree(&self, u: usize) -> Result<usize, Error> {
        let (from, to) = self.range(u)?;
        Ok(to - from)
    }

    /// Posicion de los vecinos de `u` dentro de `targets`.
    fn range(&self, u: usize) -> Result<(usize, usize), Error> {
        if u >= self.node_count() {
            return Err(Error::OutOfBounds(format!(
                "nodo {u} en un grafo de {} nodos",
                self.node_count()
            )));
        }
        Ok((self.offsets[u] as usize, self.offsets[u + 1] as usize))
    }
}

impl WeightedGraph {
    /// Construye un grafo con pesos desde aristas `(origen, destino, peso)`.
    ///
    /// # Errors
    /// Los de [`Graph::from_edges`].
    pub fn from_edges(
        n: usize,
        edges: &[(usize, usize, f64)],
        directed: bool,
    ) -> Result<Self, Error> {
        let csr = build(n, edges.len(), directed, |i| edges[i])?;
        Ok(Self {
            graph: Graph {
                offsets: csr.offsets,
                targets: csr.targets,
                edges: edges.len(),
                directed,
            },
            weights: csr.weights,
        })
    }

    /// El grafo sin pesos: nodos, aristas y vecinos.
    #[must_use]
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Pesos de las aristas que salen de `u`, alineados con [`Graph::neighbors`].
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `u >= node_count()`.
    pub fn weights(&self, u: usize) -> Result<&[f64], Error> {
        let (from, to) = self.graph.range(u)?;
        Ok(&self.weights[from..to])
    }
}

/// Los tres arreglos de un CSR: `offsets`, `targets` y un peso por posicion de `targets`.
struct Csr<W> {
    offsets: Vec<u32>,
    targets: Vec<u32>,
    weights: Vec<W>,
}

/// Reserva `len` elementos o devuelve [`Error::Alloc`] (nunca aborta).
fn reserved<T: Clone>(len: usize, fill: T) -> Result<Vec<T>, Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| Error::Alloc(format!("no hay memoria para {len} elementos del grafo")))?;
    v.resize(len, fill);
    Ok(v)
}

fn out_of_bounds(what: &str, value: usize) -> Error {
    Error::OutOfBounds(format!("{what} ({value}) no cabe en u32"))
}

/// Construccion CSR en O(n + m): cuenta grados, suma prefija y coloca cada arista en su sitio
/// siguiendo el orden de la lista. `edge(i)` da la arista `i`; `W` es el peso (`()` si no hay).
fn build<W: Copy + Default>(
    n: usize,
    m: usize,
    directed: bool,
    edge: impl Fn(usize) -> (usize, usize, W),
) -> Result<Csr<W>, Error> {
    // `n` se comprueba antes de reservar nada: un `n` enorme no debe pedir memoria.
    let node_limit = u32::try_from(n).map_err(|_| out_of_bounds("nodos", n))?;
    let arcs = if directed {
        m
    } else {
        m.checked_mul(2)
            .ok_or_else(|| out_of_bounds("aristas", m))?
    };
    u32::try_from(arcs).map_err(|_| out_of_bounds("aristas dirigidas", arcs))?;

    let mut offsets = reserved(n + 1, 0_u32)?;
    for i in 0..m {
        let (u, v, _) = edge(i);
        for node in [u, v] {
            if node >= n {
                return Err(Error::OutOfBounds(format!(
                    "la arista {i} nombra el nodo {node} en un grafo de {node_limit} nodos"
                )));
            }
        }
        offsets[u + 1] += 1;
        if !directed {
            offsets[v + 1] += 1;
        }
    }
    for u in 0..n {
        offsets[u + 1] += offsets[u];
    }

    let mut targets = reserved(arcs, 0_u32)?;
    let mut weights = reserved(arcs, W::default())?;
    // Siguiente hueco libre de cada nodo.
    let mut cursor: Vec<u32> = offsets[..n].to_vec();
    let mut place = |from: usize, to: usize, weight: W| {
        let slot = cursor[from] as usize;
        // `to < n <= u32::MAX`, comprobado arriba.
        #[allow(clippy::cast_possible_truncation)]
        {
            targets[slot] = to as u32;
        }
        weights[slot] = weight;
        cursor[from] += 1;
    };
    for i in 0..m {
        let (u, v, weight) = edge(i);
        place(u, v, weight);
        if !directed {
            place(v, u, weight);
        }
    }
    Ok(Csr {
        offsets,
        targets,
        weights,
    })
}
