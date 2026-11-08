//! Arbol de decision de clasificacion (CART binario con impureza de Gini).
//!
//! # La idea
//! El arbol hace preguntas de la forma «¿la variable `j` es `<=` que un umbral?». Cada nodo
//! interno manda las filas a la izquierda (si) o a la derecha (no); cada hoja responde con una
//! clase. Para elegir la pregunta de un nodo se prueban todas las variables y todos los cortes
//! posibles y se queda la que deja los dos lados mas **puros**.
//!
//! La impureza de Gini de un grupo es `1 - sum(p_c^2)` con `p_c` la fraccion de la clase `c`: vale
//! 0 si todas las filas son de una clase. Un corte se puntua con el Gini de los dos lados
//! ponderado por su tamano; hay que mejorar estrictamente el del nodo, si no el nodo es hoja.
//!
//! # Como se busca el corte
//! Para cada variable se ordenan las filas del nodo por su valor (`merge_sort`, estable) y se
//! recorre de menor a mayor moviendo una fila a la vez al lado izquierdo y llevando los conteos
//! por clase: cada corte candidato cuesta O(clases). Los umbrales son el **punto medio** entre
//! dos valores distintos consecutivos. Total: O(d n log n) por nivel del arbol.
//!
//! # Desempates (M8)
//! Gana el corte de menor impureza ponderada; con empate, la menor variable y luego el menor
//! umbral. En una hoja con clases empatadas gana la menor. El arbol es el mismo en cada corrida.
//!
//! # Parametros
//! - `max_depth`: profundidad maxima (la raiz es 0). Con 0 el arbol es una sola hoja.
//! - `min_samples_leaf`: filas minimas de cada hoja; un corte que deje menos en un lado no vale.
//!
//! # Etiquetas
//! Enteros `>= 0` sin nulos. Pueden ser dispersos (`3`, `1000`): el arbol los guarda tal cual y
//! predice con esos mismos numeros.

use geoda_core::{Column, DataType, Error};

use crate::matrix::Dataset;
use crate::sort::merge_sort;

/// Parametros de [`fit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeParams {
    /// Profundidad maxima (por defecto 8).
    pub max_depth: usize,
    /// Filas minimas por hoja (por defecto 1).
    pub min_samples_leaf: usize,
}

impl Default for TreeParams {
    fn default() -> Self {
        Self {
            max_depth: 8,
            min_samples_leaf: 1,
        }
    }
}

/// Un nodo del arbol.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Responde con `class`.
    Leaf {
        /// Etiqueta que predice.
        class: i64,
    },
    /// Manda a `left` si `data[feature] <= threshold`, y a `right` si no.
    Split {
        /// Variable que se pregunta.
        feature: usize,
        /// Umbral del corte.
        threshold: f64,
        /// Indice del hijo izquierdo en [`DecisionTree::nodes`].
        left: usize,
        /// Indice del hijo derecho.
        right: usize,
    },
}

/// Arbol entrenado.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionTree {
    nodes: Vec<Node>,
    features: usize,
}

/// Entrena un arbol con una lista de columnas y una columna `Int64` de etiquetas.
///
/// # Errors
/// Los de [`Dataset::from_columns`] y [`fit`]; [`Error::TypeMismatch`] si `labels` no es `Int64`.
pub fn fit_columns(
    columns: &[&Column],
    labels: &Column,
    params: TreeParams,
) -> Result<DecisionTree, Error> {
    if labels.dtype() != DataType::Int64 {
        return Err(Error::TypeMismatch(format!(
            "las etiquetas deben ser Int64, no {:?}",
            labels.dtype()
        )));
    }
    if labels.null_count() > 0 {
        return Err(Error::InvalidArgument("las etiquetas tienen nulos".into()));
    }
    fit(
        &Dataset::from_columns(columns)?,
        labels.values::<i64>()?,
        params,
    )
}

/// Entrena un arbol.
///
/// # Errors
/// [`Error::LengthMismatch`] si `labels` no mide lo mismo que las filas;
/// [`Error::InvalidArgument`] si no hay filas, una etiqueta es negativa, o `min_samples_leaf` es 0.
pub fn fit(data: &Dataset, labels: &[i64], params: TreeParams) -> Result<DecisionTree, Error> {
    let n = data.rows();
    if labels.len() != n {
        return Err(Error::LengthMismatch(format!(
            "hay {n} filas y {} etiquetas",
            labels.len()
        )));
    }
    if n == 0 {
        return Err(Error::InvalidArgument("no hay filas para entrenar".into()));
    }
    if params.min_samples_leaf == 0 {
        return Err(Error::InvalidArgument(
            "min_samples_leaf debe ser >= 1".into(),
        ));
    }
    if let Some(i) = labels.iter().position(|&l| l < 0) {
        return Err(Error::InvalidArgument(format!(
            "la etiqueta {i} es negativa ({})",
            labels[i]
        )));
    }
    // Clases densas 0..c por orden de etiqueta; `classes[c]` es la etiqueta original.
    let mut classes: Vec<i64> = labels.to_vec();
    merge_sort(&mut classes, Ord::cmp);
    classes.dedup();
    let dense: Vec<usize> = labels
        .iter()
        .map(|l| classes.partition_point(|c| c < l))
        .collect();

    let mut nodes = vec![Node::Leaf { class: 0 }];
    // (nodo, filas, profundidad); pila explicita: sin recursion.
    let mut stack = vec![(0usize, (0..n).collect::<Vec<usize>>(), 0usize)];
    while let Some((node, rows, depth)) = stack.pop() {
        let counts = class_counts(&rows, &dense, classes.len());
        let majority = counts
            .iter()
            .enumerate()
            .fold(
                (0, 0),
                |best, (c, &k)| if k > best.1 { (c, k) } else { best },
            )
            .0;
        nodes[node] = Node::Leaf {
            class: classes[majority],
        };
        let pure = counts.iter().filter(|&&k| k > 0).count() <= 1;
        if pure || depth >= params.max_depth || rows.len() < 2 * params.min_samples_leaf {
            continue;
        }
        let Some(split) = best_split(data, &rows, &dense, classes.len(), &counts, params) else {
            continue;
        };
        let (left_rows, right_rows): (Vec<usize>, Vec<usize>) = rows
            .iter()
            .partition(|&&r| data.get(r, split.feature) <= split.threshold);
        let (left, right) = (nodes.len(), nodes.len() + 1);
        nodes.push(Node::Leaf { class: 0 });
        nodes.push(Node::Leaf { class: 0 });
        nodes[node] = Node::Split {
            feature: split.feature,
            threshold: split.threshold,
            left,
            right,
        };
        stack.push((right, right_rows, depth + 1));
        stack.push((left, left_rows, depth + 1));
    }
    Ok(DecisionTree {
        nodes,
        features: data.cols(),
    })
}

struct Candidate {
    feature: usize,
    threshold: f64,
}

fn class_counts(rows: &[usize], dense: &[usize], classes: usize) -> Vec<usize> {
    let mut counts = vec![0usize; classes];
    for &r in rows {
        counts[dense[r]] += 1;
    }
    counts
}

/// Gini de unos conteos con `total` filas.
fn gini(counts: &[usize], total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let total = total as f64;
    1.0 - counts
        .iter()
        .map(|&k| {
            #[allow(clippy::cast_precision_loss)]
            let p = k as f64 / total;
            p * p
        })
        .sum::<f64>()
}

/// Mejor corte del nodo, o `None` si ninguno mejora estrictamente la impureza.
fn best_split(
    data: &Dataset,
    rows: &[usize],
    dense: &[usize],
    classes: usize,
    parent_counts: &[usize],
    params: TreeParams,
) -> Option<Candidate> {
    let n = rows.len();
    let parent = gini(parent_counts, n);
    let mut best: Option<(f64, Candidate)> = None;
    for feature in 0..data.cols() {
        let mut order = rows.to_vec();
        merge_sort(&mut order, |&a, &b| {
            data.get(a, feature).total_cmp(&data.get(b, feature))
        });
        let mut left = vec![0usize; classes];
        let mut right = parent_counts.to_vec();
        for (taken, window) in order.windows(2).enumerate() {
            let class = dense[window[0]];
            left[class] += 1;
            right[class] -= 1;
            let (low, high) = (data.get(window[0], feature), data.get(window[1], feature));
            let left_size = taken + 1;
            let right_size = n - left_size;
            if low >= high
                || left_size < params.min_samples_leaf
                || right_size < params.min_samples_leaf
            {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let score = (left_size as f64 * gini(&left, left_size)
                + right_size as f64 * gini(&right, right_size))
                / n as f64;
            if score < best.as_ref().map_or(parent, |b| b.0) {
                // El punto medio puede redondear a `high`; entonces `<= low` separa igual.
                let mid = low + (high - low) / 2.0;
                let threshold = if mid >= high { low } else { mid };
                best = Some((score, Candidate { feature, threshold }));
            }
        }
    }
    best.map(|(_, candidate)| candidate)
}

impl DecisionTree {
    /// Nodos del arbol; el 0 es la raiz.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Cantidad de variables con las que se entreno.
    #[must_use]
    pub fn features(&self) -> usize {
        self.features
    }

    /// Profundidad del arbol (0 si es una sola hoja).
    #[must_use]
    pub fn depth(&self) -> usize {
        let mut deepest = 0;
        let mut stack = vec![(0usize, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            deepest = deepest.max(depth);
            if let Node::Split { left, right, .. } = self.nodes[node] {
                stack.push((left, depth + 1));
                stack.push((right, depth + 1));
            }
        }
        deepest
    }

    /// Clase que predice el arbol para cada fila.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `data` no tiene las variables con las que se entreno.
    pub fn predict(&self, data: &Dataset) -> Result<Vec<i64>, Error> {
        if data.cols() != self.features {
            return Err(Error::LengthMismatch(format!(
                "el arbol se entreno con {} variables y llegan {}",
                self.features,
                data.cols()
            )));
        }
        Ok((0..data.rows())
            .map(|row| {
                let mut node = 0;
                loop {
                    match self.nodes[node] {
                        Node::Leaf { class } => break class,
                        Node::Split {
                            feature,
                            threshold,
                            left,
                            right,
                        } => {
                            node = if data.get(row, feature) <= threshold {
                                left
                            } else {
                                right
                            }
                        }
                    }
                }
            })
            .collect())
    }

    /// Como [`DecisionTree::predict`] sobre una lista de columnas.
    ///
    /// # Errors
    /// Los de [`Dataset::from_columns`] y [`DecisionTree::predict`].
    pub fn predict_columns(&self, columns: &[&Column]) -> Result<Vec<i64>, Error> {
        self.predict(&Dataset::from_columns(columns)?)
    }
}

#[cfg(test)]
mod tests {
    use super::{DecisionTree, Node, TreeParams, fit, fit_columns};
    use crate::matrix::Dataset;
    use geoda_core::{Column, Error, PrimitiveBuilder};

    fn floats(values: &[f64]) -> Column {
        let mut b = PrimitiveBuilder::<f64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    fn ints(values: &[i64]) -> Column {
        let mut b = PrimitiveBuilder::<i64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    #[test]
    fn dos_clases_separables_por_un_umbral() {
        let x = floats(&[1.0, 2.0, 3.0, 10.0, 11.0, 12.0]);
        let y = ints(&[0, 0, 0, 1, 1, 1]);
        let tree = fit_columns(&[&x], &y, TreeParams::default()).unwrap();
        assert_eq!(tree.nodes().len(), 3);
        match tree.nodes()[0] {
            Node::Split {
                feature, threshold, ..
            } => {
                assert_eq!(feature, 0);
                assert!((threshold - 6.5).abs() < 1e-12);
            }
            Node::Leaf { .. } => panic!("la raiz debia dividir"),
        }
        assert_eq!(tree.predict_columns(&[&x]).unwrap(), [0, 0, 0, 1, 1, 1]);
        assert_eq!(tree.depth(), 1);
    }

    #[test]
    fn profundidad_cero_es_una_hoja() {
        let x = floats(&[1.0, 2.0, 3.0, 10.0]);
        let y = ints(&[0, 0, 0, 1]);
        let params = TreeParams {
            max_depth: 0,
            min_samples_leaf: 1,
        };
        let tree = fit_columns(&[&x], &y, params).unwrap();
        assert_eq!(tree.nodes().len(), 1);
        assert_eq!(tree.predict_columns(&[&x]).unwrap(), [0, 0, 0, 0]);
    }

    #[test]
    fn min_samples_leaf_grande_deja_una_hoja() {
        let x = floats(&[1.0, 2.0, 3.0, 10.0, 11.0, 12.0]);
        let y = ints(&[0, 0, 0, 1, 1, 1]);
        let params = TreeParams {
            max_depth: 8,
            min_samples_leaf: 4,
        };
        let tree = fit_columns(&[&x], &y, params).unwrap();
        assert_eq!(tree.nodes().len(), 1);
    }

    #[test]
    fn empate_de_clases_gana_la_menor() {
        let x = floats(&[1.0, 1.0]);
        let y = ints(&[7, 3]);
        let tree = fit_columns(&[&x], &y, TreeParams::default()).unwrap();
        assert_eq!(tree.predict_columns(&[&x]).unwrap(), [3, 3]);
    }

    #[test]
    fn etiquetas_dispersas_se_conservan() {
        let x = floats(&[1.0, 2.0, 30.0, 40.0]);
        let y = ints(&[5, 5, 1000, 1000]);
        let tree = fit_columns(&[&x], &y, TreeParams::default()).unwrap();
        assert_eq!(tree.predict_columns(&[&x]).unwrap(), [5, 5, 1000, 1000]);
    }

    #[test]
    fn errores() {
        let x = floats(&[1.0, 2.0]);
        let p = TreeParams::default();
        assert!(matches!(
            fit_columns(&[&x], &ints(&[0]), p),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            fit_columns(&[&x], &ints(&[0, -1]), p),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            fit_columns(&[&x], &floats(&[0.0, 1.0]), p),
            Err(Error::TypeMismatch(_))
        ));
        let bad = TreeParams {
            max_depth: 3,
            min_samples_leaf: 0,
        };
        assert!(matches!(
            fit_columns(&[&x], &ints(&[0, 1]), bad),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            fit_columns(&[&floats(&[])], &ints(&[]), p),
            Err(Error::InvalidArgument(_))
        ));
        let tree = fit_columns(&[&x], &ints(&[0, 1]), p).unwrap();
        assert!(matches!(
            tree.predict_columns(&[&x, &x]),
            Err(Error::LengthMismatch(_))
        ));
    }

    /// Oraculo: el mejor corte de la raiz por fuerza bruta (variable, umbral) con el mismo criterio.
    fn brute_root(data: &Dataset, labels: &[i64]) -> Option<(usize, f64)> {
        let n = data.rows();
        let gini = |rows: &[usize]| {
            let mut counts = std::collections::BTreeMap::new();
            for &r in rows {
                *counts.entry(labels[r]).or_insert(0usize) += 1;
            }
            let total = f64::from(u32::try_from(rows.len()).unwrap());
            1.0 - counts
                .values()
                .map(|&k| {
                    let p = f64::from(u32::try_from(k).unwrap()) / total;
                    p * p
                })
                .sum::<f64>()
        };
        let all: Vec<usize> = (0..n).collect();
        let mut best = (gini(&all), None);
        for feature in 0..data.cols() {
            let mut values: Vec<f64> = (0..n).map(|r| data.get(r, feature)).collect();
            values.sort_by(f64::total_cmp);
            values.dedup();
            for pair in values.windows(2) {
                let threshold = pair[0] + (pair[1] - pair[0]) / 2.0;
                let (left, right): (Vec<usize>, Vec<usize>) = all
                    .iter()
                    .partition(|&&r| data.get(r, feature) <= threshold);
                let score = (f64::from(u32::try_from(left.len()).unwrap()) * gini(&left)
                    + f64::from(u32::try_from(right.len()).unwrap()) * gini(&right))
                    / f64::from(u32::try_from(n).unwrap());
                if score < best.0 {
                    best = (score, Some((feature, threshold)));
                }
            }
        }
        best.1
    }

    #[test]
    fn la_raiz_coincide_con_la_fuerza_bruta_y_ajusta_los_datos_de_entrenamiento() {
        let mut state: u64 = 0x7EE;
        let mut next = move |bound: u64| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) % bound
        };
        for _ in 0..40 {
            let rows = 12 + usize::try_from(next(20)).unwrap();
            let first: Vec<f64> = (0..rows)
                .map(|_| f64::from(u32::try_from(next(8)).unwrap()))
                .collect();
            let second: Vec<f64> = (0..rows)
                .map(|_| f64::from(u32::try_from(next(8)).unwrap()))
                .collect();
            let labels: Vec<i64> = (0..rows)
                .map(|i| i64::from(first[i] + second[i] > 7.0))
                .collect();
            let (col_first, col_second) = (floats(&first), floats(&second));
            let data = Dataset::from_columns(&[&col_first, &col_second]).unwrap();
            let tree: DecisionTree = fit(
                &data,
                &labels,
                TreeParams {
                    max_depth: 30,
                    min_samples_leaf: 1,
                },
            )
            .unwrap();
            match (tree.nodes()[0].clone(), brute_root(&data, &labels)) {
                (
                    Node::Split {
                        feature, threshold, ..
                    },
                    Some((f, t)),
                ) => {
                    assert_eq!(feature, f);
                    assert!((threshold - t).abs() < 1e-12);
                }
                (Node::Leaf { .. }, None) => {}
                (got, want) => panic!("raiz {got:?} contra {want:?}"),
            }
            // Sin limite de profundidad y sin filas contradictorias, ajusta todo el entrenamiento.
            assert_eq!(tree.predict(&data).unwrap(), labels);
        }
    }

    #[test]
    fn es_determinista() {
        let x = floats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let z = floats(&[6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
        let y = ints(&[0, 0, 1, 1, 0, 1]);
        let first = fit_columns(&[&x, &z], &y, TreeParams::default()).unwrap();
        for _ in 0..5 {
            assert_eq!(
                fit_columns(&[&x, &z], &y, TreeParams::default()).unwrap(),
                first
            );
        }
    }
}
