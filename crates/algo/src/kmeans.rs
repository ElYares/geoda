//! K-means: agrupa filas en `k` grupos de forma que cada fila quede con el centroide mas cercano.
//!
//! # La idea (algoritmo de Lloyd)
//! 1. **Inicializacion k-means++:** el primer centroide es una fila al azar; cada siguiente se
//!    elige entre las filas con probabilidad proporcional a su distancia al cuadrado al centroide
//!    mas cercano ya elegido. Reparte bien los centros de salida.
//! 2. **Asignar:** cada fila va al centroide mas cercano (con distancias iguales, el de menor
//!    indice).
//! 3. **Actualizar:** cada centroide pasa a ser la media de sus filas.
//! 4. Repetir hasta que ningun centroide se mueva mas de `tol` o se llegue a `max_iter`.
//!
//! Minimiza la **inercia**: la suma de las distancias al cuadrado de cada fila a su centroide.
//! Encuentra un minimo local, no el global; por eso importa la semilla.
//!
//! # Determinismo (M3, M8)
//! El azar sale de un [`Rng`] propio con `seed` explicita: la misma entrada, `k` y semilla dan el
//! mismo resultado bit a bit.
//!
//! # Clusters vacios (M4)
//! Si en el paso de actualizar un grupo se queda sin filas, su centroide pasa a la fila mas lejana
//! de su propio centroide (de entre los grupos con mas de una fila). Si los datos tienen al menos
//! `k` filas distintas, salen `k` grupos no vacios. Con **menos filas distintas que `k`** dos
//! centroides coinciden y las filas van al de menor indice: algun grupo queda sin etiquetas (no es
//! un error, no hay forma de repartirlas).
//!
//! # Etiquetas coherentes
//! Tras la ultima actualizacion se asigna una vez mas, asi que `labels` y `inertia` corresponden
//! exactamente a los `centroids` devueltos.

use geoda_core::{Column, Error};

use crate::matrix::{Dataset, Rng, squared_distance};

/// Parametros de [`kmeans`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KMeansParams {
    /// Cantidad de grupos (entre 1 y el numero de filas).
    pub k: usize,
    /// Tope de iteraciones (por defecto 100).
    pub max_iter: usize,
    /// Se detiene cuando ningun centroide se mueve mas de esto (por defecto `1e-4`).
    pub tol: f64,
    /// Semilla del generador.
    pub seed: u64,
}

impl KMeansParams {
    /// Parametros con `k` y `seed` y el resto por defecto.
    #[must_use]
    pub fn new(k: usize, seed: u64) -> Self {
        Self {
            k,
            max_iter: 100,
            tol: 1e-4,
            seed,
        }
    }
}

/// Resultado de [`kmeans`].
#[derive(Debug, Clone, PartialEq)]
pub struct KMeans {
    /// Grupo de cada fila (`0..k`).
    pub labels: Vec<usize>,
    /// Centroides, `k * dims` valores por filas.
    pub centroids: Vec<f64>,
    /// Variables por fila.
    pub dims: usize,
    /// Suma de distancias al cuadrado de cada fila a su centroide.
    pub inertia: f64,
    /// Iteraciones hechas.
    pub iterations: usize,
}

/// K-means sobre una lista de columnas (una por variable).
///
/// # Errors
/// Los de [`Dataset::from_columns`] y los de [`kmeans`].
pub fn kmeans_columns(columns: &[&Column], params: KMeansParams) -> Result<KMeans, Error> {
    kmeans(&Dataset::from_columns(columns)?, params)
}

/// K-means sobre una matriz.
///
/// # Errors
/// [`Error::InvalidArgument`] si `k` es 0 o mayor que el numero de filas, `max_iter` es 0, o `tol`
/// es negativo o no finito.
pub fn kmeans(data: &Dataset, params: KMeansParams) -> Result<KMeans, Error> {
    let (n, d, k) = (data.rows(), data.cols(), params.k);
    if k == 0 || k > n {
        return Err(Error::InvalidArgument(format!(
            "k = {k} debe estar entre 1 y el numero de filas ({n})"
        )));
    }
    if params.max_iter == 0 || !params.tol.is_finite() || params.tol < 0.0 {
        return Err(Error::InvalidArgument(
            "max_iter debe ser >= 1 y tol finito y >= 0".to_string(),
        ));
    }
    let mut rng = Rng::new(params.seed);
    let mut centroids = initial_centroids(data, k, &mut rng);
    let mut labels = vec![0usize; n];
    let mut iterations = 0;
    while iterations < params.max_iter {
        iterations += 1;
        assign(data, &centroids, &mut labels);
        let updated = update(data, &centroids, &labels, k);
        let shift = (0..k)
            .map(|c| squared_distance(&centroids[c * d..(c + 1) * d], &updated[c * d..(c + 1) * d]))
            .fold(0.0_f64, f64::max);
        centroids = updated;
        if shift.sqrt() <= params.tol {
            break;
        }
    }
    let inertia = assign(data, &centroids, &mut labels);
    Ok(KMeans {
        labels,
        centroids,
        dims: d,
        inertia,
        iterations,
    })
}

/// Centroides iniciales por k-means++.
fn initial_centroids(data: &Dataset, k: usize, rng: &mut Rng) -> Vec<f64> {
    let (n, d) = (data.rows(), data.cols());
    let mut centroids = Vec::with_capacity(k * d);
    centroids.extend_from_slice(data.row(rng.below(n)));
    // Distancia al cuadrado de cada fila al centroide mas cercano elegido hasta ahora.
    let mut nearest: Vec<f64> = (0..n)
        .map(|i| squared_distance(data.row(i), &centroids[..d]))
        .collect();
    for chosen in 1..k {
        let total: f64 = nearest.iter().sum();
        let pick = if total > 0.0 {
            let target = rng.next_f64() * total;
            let mut acc = 0.0;
            // Primera fila cuya suma acumulada pasa de `target`; las de distancia 0 nunca salen.
            let mut found = n - 1;
            for (i, &weight) in nearest.iter().enumerate() {
                acc += weight;
                if weight > 0.0 && acc > target {
                    found = i;
                    break;
                }
            }
            found
        } else {
            // Todas las filas coinciden con un centroide (datos repetidos): cualquiera sirve.
            rng.below(n)
        };
        centroids.extend_from_slice(data.row(pick));
        let new = &centroids[chosen * d..(chosen + 1) * d];
        for (i, slot) in nearest.iter_mut().enumerate() {
            *slot = slot.min(squared_distance(data.row(i), new));
        }
    }
    centroids
}

/// Asigna cada fila al centroide mas cercano (empate: el de menor indice). Devuelve la inercia.
fn assign(data: &Dataset, centroids: &[f64], labels: &mut [usize]) -> f64 {
    let d = data.cols();
    let k = centroids.len() / d;
    let mut inertia = 0.0;
    for (i, label) in labels.iter_mut().enumerate() {
        let row = data.row(i);
        let mut best = (0, f64::INFINITY);
        for c in 0..k {
            let dist = squared_distance(row, &centroids[c * d..(c + 1) * d]);
            if dist < best.1 {
                best = (c, dist);
            }
        }
        *label = best.0;
        inertia += best.1;
    }
    inertia
}

/// Nuevos centroides: la media de cada grupo; un grupo vacio toma la fila mas lejana (M4).
fn update(data: &Dataset, centroids: &[f64], labels: &[usize], k: usize) -> Vec<f64> {
    let d = data.cols();
    let mut sums = vec![0.0; k * d];
    let mut sizes = vec![0usize; k];
    for (i, &c) in labels.iter().enumerate() {
        sizes[c] += 1;
        for (slot, value) in sums[c * d..(c + 1) * d].iter_mut().zip(data.row(i)) {
            *slot += value;
        }
    }
    let mut updated = vec![0.0; k * d];
    for c in 0..k {
        if sizes[c] > 0 {
            #[allow(clippy::cast_precision_loss)]
            let size = sizes[c] as f64;
            for j in 0..d {
                updated[c * d + j] = sums[c * d + j] / size;
            }
        }
    }
    for c in 0..k {
        if sizes[c] == 0 {
            // Fila mas lejana de su centroide, de un grupo que pueda ceder una fila.
            let mut far = (0, -1.0_f64);
            for (i, &owner) in labels.iter().enumerate() {
                if sizes[owner] > 1 {
                    let dist =
                        squared_distance(data.row(i), &centroids[owner * d..(owner + 1) * d]);
                    if dist > far.1 {
                        far = (i, dist);
                    }
                }
            }
            let (row, _) = far;
            updated[c * d..(c + 1) * d].copy_from_slice(data.row(row));
            sizes[labels[row]] -= 1;
            sizes[c] = 1;
        }
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::{KMeansParams, kmeans, kmeans_columns};
    use crate::matrix::Dataset;
    use geoda_core::{Column, Error, PrimitiveBuilder};

    fn floats(values: &[f64]) -> Column {
        let mut b = PrimitiveBuilder::<f64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    fn two_clouds() -> (Column, Column) {
        (
            floats(&[0.0, 0.0, 10.0, 10.0]),
            floats(&[0.0, 1.0, 10.0, 11.0]),
        )
    }

    #[test]
    fn separa_dos_nubes() {
        let (x, y) = two_clouds();
        let result = kmeans_columns(&[&x, &y], KMeansParams::new(2, 1)).unwrap();
        assert_eq!(result.labels[0], result.labels[1]);
        assert_eq!(result.labels[2], result.labels[3]);
        assert_ne!(result.labels[0], result.labels[2]);
        assert!((result.inertia - 1.0).abs() < 1e-12, "{}", result.inertia);
    }

    #[test]
    fn la_misma_semilla_da_lo_mismo() {
        let (x, y) = two_clouds();
        let first = kmeans_columns(&[&x, &y], KMeansParams::new(2, 9)).unwrap();
        for _ in 0..5 {
            assert_eq!(
                kmeans_columns(&[&x, &y], KMeansParams::new(2, 9)).unwrap(),
                first
            );
        }
    }

    #[test]
    fn k_uno_es_la_media() {
        let (x, y) = two_clouds();
        let result = kmeans_columns(&[&x, &y], KMeansParams::new(1, 0)).unwrap();
        assert_eq!(result.labels, [0, 0, 0, 0]);
        assert!((result.centroids[0] - 5.0).abs() < 1e-12);
        assert!((result.centroids[1] - 5.5).abs() < 1e-12);
        // Suma de cuadrados respecto a (5, 5.5).
        assert!(
            (result.inertia - (25.0 + 25.0 + 25.0 + 25.0 + 30.25 + 20.25 + 20.25 + 30.25)).abs()
                < 1e-9
        );
    }

    #[test]
    fn argumentos_invalidos() {
        let (x, y) = two_clouds();
        for k in [0, 5] {
            assert!(matches!(
                kmeans_columns(&[&x, &y], KMeansParams::new(k, 0)),
                Err(Error::InvalidArgument(_))
            ));
        }
        assert!(matches!(
            kmeans_columns(&[&x, &floats(&[1.0])], KMeansParams::new(2, 0)),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            kmeans_columns(
                &[&floats(&[1.0, f64::NAN, 3.0, 4.0])],
                KMeansParams::new(2, 0)
            ),
            Err(Error::InvalidArgument(_))
        ));
        let data = Dataset::from_columns(&[&x]).unwrap();
        let mut params = KMeansParams::new(2, 0);
        params.max_iter = 0;
        assert!(matches!(
            kmeans(&data, params),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn datos_repetidos_no_dejan_clusters_vacios_si_hay_k_valores_distintos() {
        // Muchas filas repetidas pero tres valores distintos, k = 3: tres grupos no vacios.
        let x = floats(&[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 9.0, 9.0, 5.0]);
        for seed in 0..20 {
            let result = kmeans_columns(&[&x], KMeansParams::new(3, seed)).unwrap();
            let mut sizes = [0usize; 3];
            for &l in &result.labels {
                sizes[l] += 1;
            }
            assert!(sizes.iter().all(|&s| s > 0), "semilla {seed}: {sizes:?}");
            assert!(
                result.inertia.abs() < 1e-12,
                "cada valor es su propio centroide"
            );
        }
    }

    #[test]
    fn con_menos_valores_distintos_que_k_no_falla() {
        let x = floats(&[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 9.0]);
        let result = kmeans_columns(&[&x], KMeansParams::new(3, 4)).unwrap();
        assert!(result.labels.iter().all(|&l| l < 3));
        assert!(result.inertia.abs() < 1e-12);
    }

    #[test]
    fn etiquetas_y_centroides_son_coherentes() {
        let mut state: u64 = 0xC1;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 40).unwrap()) / 1000.0
        };
        let x = floats(&(0..200).map(|_| next()).collect::<Vec<_>>());
        let y = floats(&(0..200).map(|_| next()).collect::<Vec<_>>());
        let data = Dataset::from_columns(&[&x, &y]).unwrap();
        let result = kmeans(&data, KMeansParams::new(5, 3)).unwrap();
        // Cada fila esta con su centroide mas cercano y la inercia es la suma de esas distancias.
        let mut inertia = 0.0;
        for i in 0..data.rows() {
            let dists: Vec<f64> = (0..5)
                .map(|c| {
                    crate::matrix::squared_distance(
                        data.row(i),
                        &result.centroids[c * 2..c * 2 + 2],
                    )
                })
                .collect();
            let best = dists.iter().copied().fold(f64::INFINITY, f64::min);
            assert!((dists[result.labels[i]] - best).abs() < 1e-9);
            inertia += best;
        }
        assert!((inertia - result.inertia).abs() < 1e-6 * inertia.max(1.0));
    }
}
