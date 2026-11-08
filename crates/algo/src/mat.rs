//! `Matrix`: matriz densa de `f64` guardada por filas, la base del algebra lineal.
//!
//! # Que es y que no es
//! Una matriz matematica (la `A` de `A x = b`). Es distinta de [`Dataset`](crate::matrix::Dataset),
//! que es una *tabla de datos* inmutable para mineria; aqui hay operaciones (decision 014, N1).
//!
//! # Valores validos
//! Los constructores rechazan `NaN` e infinitos con [`Error::InvalidArgument`]: ninguna de las
//! factorizaciones tiene una respuesta razonable con ellos. Las operaciones **no** vuelven a
//! comprobarlo, asi que un producto que se desborda puede dar infinitos.
//!
//! # El producto
//! `C = A * B` con el orden de bucles `i-k-j`: para cada fila de `A` y cada `k`, se suma
//! `A[i][k] * B[k][..]` sobre la fila `i` de `C`. Asi `B` y `C` se recorren por filas contiguas, que es
//! lo que la memoria agradece. Cada `C[i][j]` suma sus terminos en el orden de `k`, lo que hace
//! exactas identidades como `(A B)^T == B^T A^T`.

use geoda_core::{Column, DataType, Error};

/// Matriz de `rows` filas por `cols` columnas, por filas.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

fn check_finite(data: &[f64]) -> Result<(), Error> {
    match data.iter().position(|v| !v.is_finite()) {
        Some(at) => Err(Error::InvalidArgument(format!(
            "el valor {at} no es finito ({})",
            data[at]
        ))),
        None => Ok(()),
    }
}

impl Matrix {
    /// Matriz de ceros.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] si `rows * cols` no cabe en `usize`; [`Error::Alloc`] si no hay memoria.
    pub fn zeros(rows: usize, cols: usize) -> Result<Self, Error> {
        let total = rows
            .checked_mul(cols)
            .ok_or_else(|| Error::OutOfBounds(format!("{rows} x {cols} no cabe en usize")))?;
        let mut data = Vec::new();
        data.try_reserve_exact(total)
            .map_err(|_| Error::Alloc(format!("no hay memoria para {total} valores")))?;
        data.resize(total, 0.0);
        Ok(Self { rows, cols, data })
    }

    /// Matriz identidad de `n x n`.
    ///
    /// # Errors
    /// Los de [`Matrix::zeros`].
    pub fn identity(n: usize) -> Result<Self, Error> {
        let mut m = Self::zeros(n, n)?;
        for i in 0..n {
            m.data[i * n + i] = 1.0;
        }
        Ok(m)
    }

    /// Desde `rows * cols` valores por filas.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `data.len() != rows * cols`; [`Error::InvalidArgument`] si hay
    /// un valor no finito.
    pub fn from_flat(rows: usize, cols: usize, data: Vec<f64>) -> Result<Self, Error> {
        if rows.checked_mul(cols) != Some(data.len()) {
            return Err(Error::LengthMismatch(format!(
                "{rows} x {cols} no son {} valores",
                data.len()
            )));
        }
        check_finite(&data)?;
        Ok(Self { rows, cols, data })
    }

    /// Desde una lista de filas (todas del mismo largo). Sin filas, la matriz es de 0 x 0.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si las filas miden distinto; [`Error::InvalidArgument`] si hay un
    /// valor no finito.
    pub fn from_rows(rows: &[Vec<f64>]) -> Result<Self, Error> {
        let cols = rows.first().map_or(0, Vec::len);
        let mut data = Vec::with_capacity(rows.len() * cols);
        for (i, row) in rows.iter().enumerate() {
            if row.len() != cols {
                return Err(Error::LengthMismatch(format!(
                    "la fila {i} mide {} y la primera {cols}",
                    row.len()
                )));
            }
            data.extend_from_slice(row);
        }
        Self::from_flat(rows.len(), cols, data)
    }

    /// Desde una lista de columnas `Int32`, `Int64` o `Float64` sin nulos: la columna `j` de la lista es
    /// la columna `j` de la matriz.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si no hay columnas, hay nulos, un tipo que no es numerico o un valor
    /// no finito; [`Error::LengthMismatch`] si miden distinto.
    pub fn from_columns(columns: &[&Column]) -> Result<Self, Error> {
        let first = columns
            .first()
            .ok_or_else(|| Error::InvalidArgument("se necesita al menos una columna".into()))?;
        let (rows, cols) = (first.len(), columns.len());
        let mut m = Self::zeros(rows, cols)?;
        for (j, column) in columns.iter().enumerate() {
            if column.len() != rows {
                return Err(Error::LengthMismatch(format!(
                    "la columna {j} mide {} y la primera {rows}",
                    column.len()
                )));
            }
            if column.null_count() > 0 {
                return Err(Error::InvalidArgument(format!(
                    "la columna {j} tiene nulos"
                )));
            }
            #[allow(clippy::cast_precision_loss)]
            // mas alla de 2^53 se pierde precision, documentado
            let values: Vec<f64> = match column.dtype() {
                DataType::Float64 => column.values::<f64>()?.to_vec(),
                DataType::Int32 => column
                    .values::<i32>()?
                    .iter()
                    .map(|&v| f64::from(v))
                    .collect(),
                DataType::Int64 => column.values::<i64>()?.iter().map(|&v| v as f64).collect(),
                other => {
                    return Err(Error::InvalidArgument(format!(
                        "la columna {j} es {other:?}: se esperaba Int32, Int64 o Float64"
                    )));
                }
            };
            check_finite(&values)?;
            for (i, value) in values.into_iter().enumerate() {
                m.data[i * cols + j] = value;
            }
        }
        Ok(m)
    }

    /// Filas.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Columnas.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Los valores por filas.
    #[must_use]
    pub fn data(&self) -> &[f64] {
        &self.data
    }

    /// Valor de la fila `i` y la columna `j`.
    ///
    /// # Panics
    /// Si `i >= rows()` o `j >= cols()`.
    #[must_use]
    pub fn get(&self, i: usize, j: usize) -> f64 {
        assert!(i < self.rows && j < self.cols, "indice fuera de la matriz");
        self.data[i * self.cols + j]
    }

    /// Fila `i`.
    ///
    /// # Panics
    /// Si `i >= rows()`.
    #[must_use]
    pub fn row(&self, i: usize) -> &[f64] {
        &self.data[i * self.cols..(i + 1) * self.cols]
    }

    /// Columna `j`, copiada.
    ///
    /// # Panics
    /// Si `j >= cols()`.
    #[must_use]
    pub fn column(&self, j: usize) -> Vec<f64> {
        assert!(j < self.cols, "columna fuera de la matriz");
        (0..self.rows)
            .map(|i| self.data[i * self.cols + j])
            .collect()
    }

    /// Las filas como listas.
    #[must_use]
    pub fn to_rows(&self) -> Vec<Vec<f64>> {
        (0..self.rows).map(|i| self.row(i).to_vec()).collect()
    }

    /// La transpuesta.
    ///
    /// # Errors
    /// Los de [`Matrix::zeros`].
    pub fn transpose(&self) -> Result<Self, Error> {
        let mut t = Self::zeros(self.cols, self.rows)?;
        for i in 0..self.rows {
            for j in 0..self.cols {
                t.data[j * self.rows + i] = self.data[i * self.cols + j];
            }
        }
        Ok(t)
    }

    /// El producto `self * other`.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `self.cols() != other.rows()`.
    pub fn matmul(&self, other: &Self) -> Result<Self, Error> {
        if self.cols != other.rows {
            return Err(Error::LengthMismatch(format!(
                "no se puede multiplicar {} x {} por {} x {}",
                self.rows, self.cols, other.rows, other.cols
            )));
        }
        let mut product = Self::zeros(self.rows, other.cols)?;
        for i in 0..self.rows {
            for k in 0..self.cols {
                let a = self.data[i * self.cols + k];
                let b_row = &other.data[k * other.cols..(k + 1) * other.cols];
                let c_row = &mut product.data[i * other.cols..(i + 1) * other.cols];
                for (c, b) in c_row.iter_mut().zip(b_row) {
                    *c += a * b;
                }
            }
        }
        Ok(product)
    }

    /// El producto `self * x` con `x` un vector columna.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] si `x.len() != self.cols()`.
    pub fn matvec(&self, x: &[f64]) -> Result<Vec<f64>, Error> {
        if x.len() != self.cols {
            return Err(Error::LengthMismatch(format!(
                "la matriz tiene {} columnas y el vector {} valores",
                self.cols,
                x.len()
            )));
        }
        Ok((0..self.rows)
            .map(|i| self.row(i).iter().zip(x).map(|(a, b)| a * b).sum())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::Matrix;
    use geoda_core::{Column, Error, PrimitiveBuilder, Utf8Builder};

    fn m(rows: &[&[f64]]) -> Matrix {
        Matrix::from_rows(&rows.iter().map(|r| r.to_vec()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn transpuesta() {
        let a = m(&[&[1.0, 2.0], &[3.0, 4.0]]);
        assert_eq!(a.transpose().unwrap(), m(&[&[1.0, 3.0], &[2.0, 4.0]]));
        let wide = m(&[&[1.0, 2.0, 3.0]]);
        assert_eq!(
            (
                wide.transpose().unwrap().rows(),
                wide.transpose().unwrap().cols()
            ),
            (3, 1)
        );
    }

    #[test]
    fn producto() {
        let a = m(&[&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]]);
        let b = m(&[&[7.0, 8.0], &[9.0, 10.0], &[11.0, 12.0]]);
        assert_eq!(a.matmul(&b).unwrap(), m(&[&[58.0, 64.0], &[139.0, 154.0]]));
    }

    #[test]
    fn la_identidad_no_cambia_nada() {
        let a = m(&[&[1.5, -2.0, 0.25], &[3.0, 4.0, -5.0]]);
        assert_eq!(a.matmul(&Matrix::identity(3).unwrap()).unwrap(), a);
        assert_eq!(Matrix::identity(2).unwrap().matmul(&a).unwrap(), a);
    }

    #[test]
    fn dimensiones_que_no_encajan() {
        let a = m(&[&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]]);
        assert!(matches!(a.matmul(&a), Err(Error::LengthMismatch(_))));
        assert!(matches!(
            a.matvec(&[1.0, 2.0]),
            Err(Error::LengthMismatch(_))
        ));
    }

    #[test]
    fn construccion_invalida() {
        assert!(matches!(
            Matrix::from_rows(&[vec![1.0, 2.0], vec![3.0]]),
            Err(Error::LengthMismatch(_))
        ));
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(
                Matrix::from_rows(&[vec![1.0, bad]]),
                Err(Error::InvalidArgument(_))
            ));
        }
        assert!(matches!(
            Matrix::from_flat(2, 2, vec![1.0; 3]),
            Err(Error::LengthMismatch(_))
        ));
        assert_eq!(Matrix::from_rows(&[]).unwrap().rows(), 0);
    }

    fn floats(values: &[f64]) -> Column {
        let mut b = PrimitiveBuilder::<f64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    #[test]
    fn desde_columnas() {
        let mut ints = PrimitiveBuilder::<i64>::new();
        ints.append(10);
        ints.append(20);
        let (x, y) = (floats(&[1.0, 2.0]), ints.finish().unwrap());
        let a = Matrix::from_columns(&[&x, &y]).unwrap();
        assert_eq!(a, m(&[&[1.0, 10.0], &[2.0, 20.0]]));
        assert_eq!(a.column(1), [10.0, 20.0]);
        assert!(matches!(
            Matrix::from_columns(&[]),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            Matrix::from_columns(&[&x, &floats(&[1.0])]),
            Err(Error::LengthMismatch(_))
        ));
        let mut text = Utf8Builder::new();
        text.append("a").unwrap();
        assert!(matches!(
            Matrix::from_columns(&[&text.finish().unwrap()]),
            Err(Error::InvalidArgument(_))
        ));
        let mut with_null = PrimitiveBuilder::<f64>::new();
        with_null.append(1.0);
        with_null.append_null();
        assert!(matches!(
            Matrix::from_columns(&[&with_null.finish().unwrap()]),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn matvec_es_el_producto_por_una_columna() {
        let a = m(&[&[1.0, 2.0], &[3.0, 4.0], &[5.0, 6.0]]);
        let x = [0.5, -1.0];
        let column = Matrix::from_flat(2, 1, x.to_vec()).unwrap();
        assert_eq!(a.matvec(&x).unwrap(), a.matmul(&column).unwrap().data());
    }

    #[test]
    fn transpuesta_del_producto() {
        let mut state: u64 = 0x3A7;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 40).unwrap()) / 997.0 - 8000.0
        };
        for (r, k, c) in [(1, 1, 1), (2, 3, 4), (5, 5, 5), (7, 2, 3), (3, 9, 1)] {
            let a = Matrix::from_flat(r, k, (0..r * k).map(|_| next()).collect()).unwrap();
            let b = Matrix::from_flat(k, c, (0..k * c).map(|_| next()).collect()).unwrap();
            let left = a.matmul(&b).unwrap().transpose().unwrap();
            let right = b
                .transpose()
                .unwrap()
                .matmul(&a.transpose().unwrap())
                .unwrap();
            assert_eq!(left, right, "{r}x{k}x{c}");
        }
    }
}
