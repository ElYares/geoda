//! Conjuntos disjuntos (union-find), componentes conexas y orden topologico.
//!
//! # Union-find
//! [`UnionFind`] guarda un bosque: cada elemento apunta a su padre y la raiz se apunta a si
//! misma. `find` sube hasta la raiz y **comprime el camino** (deja a todos los visitados
//! apuntando a la raiz); `union` cuelga la raiz del arbol menor bajo la del mayor (**por
//! tamano**), asi que los arboles quedan muy bajos y `find` cuesta casi O(1). Ambas operaciones
//! son iterativas: un millon de uniones encadenadas no desborda la pila.
//!
//! # Componentes
//! [`components`] ignora el sentido de las aristas: en un grafo dirigido da las componentes
//! *debilmente* conexas. Los ids van de 0 en el orden del primer nodo de cada componente, asi
//! que el resultado no depende de que raiz eligio el union-find.
//!
//! # Orden topologico
//! [`topological_order`] usa el algoritmo de Kahn: se van sacando los nodos sin aristas de
//! entrada pendientes. Si hay varios disponibles gana el de menor indice (un monticulo minimo),
//! lo que lo hace determinista. Si sobran nodos al final, hay un ciclo y es un error: nunca se
//! devuelve un orden parcial. Un bucle `u -> u` es un ciclo.

use std::cmp::Ordering;

use geoda_core::Error;

use crate::graph::Graph;
use crate::heap::Heap;

/// Conjuntos disjuntos sobre los elementos `0..n`.
#[derive(Debug, Clone)]
pub struct UnionFind {
    parent: Vec<u32>,
    size: Vec<u32>,
    sets: usize,
}

impl UnionFind {
    /// `n` conjuntos de un elemento cada uno.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `n` no cabe en `u32`; [`Error::Alloc`] si no hay memoria.
    pub fn new(n: usize) -> Result<Self, Error> {
        let top = u32::try_from(n)
            .map_err(|_| Error::OutOfBounds(format!("{n} elementos no caben en u32")))?;
        let mut parent = Vec::new();
        let mut size = Vec::new();
        for vec in [&mut parent, &mut size] {
            vec.try_reserve_exact(n)
                .map_err(|_| Error::Alloc(format!("no hay memoria para {n} elementos")))?;
        }
        parent.extend(0..top);
        size.resize(n, 1);
        Ok(Self {
            parent,
            size,
            sets: n,
        })
    }

    /// Cantidad de elementos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    /// `true` si no hay elementos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Cantidad de conjuntos que quedan.
    #[must_use]
    pub fn set_count(&self) -> usize {
        self.sets
    }

    fn check(&self, x: usize) -> Result<(), Error> {
        if x >= self.parent.len() {
            return Err(Error::OutOfBounds(format!(
                "elemento {x} en un union-find de {}",
                self.parent.len()
            )));
        }
        Ok(())
    }

    /// Representante del conjunto de `x`, comprimiendo el camino.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `x >= len()`.
    pub fn find(&mut self, x: usize) -> Result<usize, Error> {
        self.check(x)?;
        let mut root = x;
        while self.parent[root] as usize != root {
            root = self.parent[root] as usize;
        }
        // Segunda pasada: todos los del camino apuntan ahora a la raiz.
        let mut node = x;
        while self.parent[node] as usize != root && node != root {
            let next = self.parent[node] as usize;
            #[allow(clippy::cast_possible_truncation)] // `root < len <= u32::MAX`
            {
                self.parent[node] = root as u32;
            }
            node = next;
        }
        Ok(root)
    }

    /// Une los conjuntos de `a` y `b`. Devuelve `true` si estaban separados.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si alguno es `>= len()`.
    pub fn union(&mut self, a: usize, b: usize) -> Result<bool, Error> {
        let (mut big, mut small) = (self.find(a)?, self.find(b)?);
        if big == small {
            return Ok(false);
        }
        if self.size[big] < self.size[small] {
            std::mem::swap(&mut big, &mut small);
        }
        #[allow(clippy::cast_possible_truncation)] // `big < len <= u32::MAX`
        {
            self.parent[small] = big as u32;
        }
        self.size[big] += self.size[small];
        self.sets -= 1;
        Ok(true)
    }

    /// `true` si `a` y `b` estan en el mismo conjunto.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si alguno es `>= len()`.
    pub fn same(&mut self, a: usize, b: usize) -> Result<bool, Error> {
        Ok(self.find(a)? == self.find(b)?)
    }
}

/// Componentes conexas de un grafo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Components {
    /// Id de la componente de cada nodo; los ids van de 0 en el orden del primer nodo.
    pub id: Vec<u32>,
    /// Cantidad de componentes.
    pub count: usize,
}

/// Componentes conexas. En un grafo dirigido se ignora el sentido de las aristas
/// (componentes debilmente conexas).
///
/// # Errors
/// [`Error::Alloc`] si no hay memoria.
pub fn components(graph: &Graph) -> Result<Components, Error> {
    let n = graph.node_count();
    let mut sets = UnionFind::new(n)?;
    for u in 0..n {
        for &v in graph.neighbors(u)? {
            sets.union(u, v as usize)?;
        }
    }
    // Numera por primera aparicion de la raiz, recorriendo los nodos en orden.
    let mut label_of_root: Vec<Option<u32>> = vec![None; n];
    let mut id = Vec::with_capacity(n);
    let mut count = 0_u32;
    for u in 0..n {
        let root = sets.find(u)?;
        let label = *label_of_root[root].get_or_insert_with(|| {
            count += 1;
            count - 1
        });
        id.push(label);
    }
    Ok(Components {
        id,
        count: count as usize,
    })
}

/// Un orden de los nodos en el que toda arista `(u, v)` pone a `u` antes que a `v`.
///
/// Entre varios candidatos gana el de menor indice.
///
/// # Errors
/// [`Error::InvalidArgument`] si el grafo no es dirigido o tiene un ciclo (un bucle cuenta).
pub fn topological_order(graph: &Graph) -> Result<Vec<u32>, Error> {
    if !graph.is_directed() {
        return Err(Error::InvalidArgument(
            "el orden topologico necesita un grafo dirigido".to_string(),
        ));
    }
    let n = graph.node_count();
    let mut pending = vec![0_usize; n];
    for u in 0..n {
        for &v in graph.neighbors(u)? {
            pending[v as usize] += 1;
        }
    }
    let mut ready = Heap::new(|a: &u32, b: &u32| -> Ordering { a.cmp(b) });
    for (u, &incoming) in pending.iter().enumerate() {
        if incoming == 0 {
            ready.push(u32::try_from(u).map_err(|_| Error::OutOfBounds(format!("nodo {u}")))?);
        }
    }
    let mut order = Vec::with_capacity(n);
    while let Some(u) = ready.pop() {
        order.push(u);
        for &v in graph.neighbors(u as usize)? {
            pending[v as usize] -= 1;
            if pending[v as usize] == 0 {
                ready.push(v);
            }
        }
    }
    if order.len() < n {
        return Err(Error::InvalidArgument(format!(
            "el grafo tiene un ciclo: {} de {n} nodos no se pueden ordenar",
            n - order.len()
        )));
    }
    Ok(order)
}
