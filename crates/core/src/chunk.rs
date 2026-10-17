//! `Chunk`: un trozo de tabla, columnas de la misma longitud bajo un `Schema`.

use crate::{Column, Error, Schema};

/// Equivalente de un `RecordBatch`: unidad de trabajo del motor.
///
/// Invariante 6: una columna por campo, con el mismo tipo, todas de igual longitud,
/// y ninguna columna con nulos si su campo es `nullable == false`.
#[derive(Debug, Clone)]
pub struct Chunk {
    schema: Schema,
    columns: Vec<Column>,
    len: usize,
}

impl Chunk {
    /// Une un esquema con sus columnas, validando el invariante 6.
    ///
    /// # Errors
    /// - [`Error::LengthMismatch`] si la cantidad de columnas no coincide con la de
    ///   campos, o las columnas no tienen todas la misma longitud.
    /// - [`Error::TypeMismatch`] si el tipo de una columna difiere del de su campo.
    /// - [`Error::InvalidArgument`] si un campo no nullable recibe una columna con nulos.
    pub fn try_new(schema: Schema, columns: Vec<Column>) -> Result<Self, Error> {
        if schema.len() != columns.len() {
            return Err(Error::LengthMismatch(format!(
                "el esquema tiene {} campos y llegaron {} columnas",
                schema.len(),
                columns.len()
            )));
        }
        let len = columns.first().map_or(0, Column::len);
        for (field, column) in schema.fields().iter().zip(&columns) {
            if column.dtype() != field.dtype() {
                return Err(Error::TypeMismatch(format!(
                    "{}: el campo es {:?} y la columna {:?}",
                    field.name(),
                    field.dtype(),
                    column.dtype()
                )));
            }
            if column.len() != len {
                return Err(Error::LengthMismatch(format!(
                    "{}: tiene {} valores y la primera columna {len}",
                    field.name(),
                    column.len()
                )));
            }
            if !field.nullable() && column.null_count() > 0 {
                return Err(Error::InvalidArgument(format!(
                    "{}: no admite nulos y la columna trae {}",
                    field.name(),
                    column.null_count()
                )));
            }
        }
        Ok(Self {
            schema,
            columns,
            len,
        })
    }

    /// Esquema.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Columnas, en el orden del esquema.
    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Columna por posicion.
    #[must_use]
    pub fn column(&self, i: usize) -> Option<&Column> {
        self.columns.get(i)
    }

    /// Cantidad de filas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` si no tiene filas.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    use super::Chunk;
    use crate::{
        BoolBuilder, Column, DataType, Error, Field, PrimitiveBuilder, Schema, Utf8Builder,
    };

    fn ints(valores: &[Option<i32>]) -> Column {
        let mut b = PrimitiveBuilder::<i32>::new();
        for v in valores {
            match v {
                Some(v) => b.append(*v),
                None => b.append_null(),
            }
        }
        b.finish().unwrap()
    }

    fn schema(campos: &[(&str, DataType, bool)]) -> Schema {
        Schema::try_new(
            campos
                .iter()
                .map(|&(n, t, nullable)| Field::new(n, t, nullable))
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn chunk_valido() {
        let mut texto = Utf8Builder::new();
        texto.append("a").unwrap();
        texto.append("b").unwrap();
        let s = schema(&[("n", DataType::Int32, false), ("t", DataType::Utf8, true)]);
        let c =
            Chunk::try_new(s, vec![ints(&[Some(1), Some(2)]), texto.finish().unwrap()]).unwrap();
        assert_eq!((c.len(), c.columns().len()), (2, 2));
        assert_eq!(c.column(1).unwrap().str_value(1).unwrap(), "b");
        assert!(c.column(2).is_none());
        assert_eq!(c.schema().len(), 2);
    }

    #[test]
    fn chunk_sin_columnas_es_valido_y_vacio() {
        let c = Chunk::try_new(schema(&[]), vec![]).unwrap();
        assert!(c.is_empty());
    }

    #[test]
    fn columnas_de_distinta_longitud_es_err() {
        let s = schema(&[("a", DataType::Int32, true), ("b", DataType::Int32, true)]);
        let r = Chunk::try_new(s, vec![ints(&[Some(1), Some(2)]), ints(&[Some(1)])]);
        assert!(matches!(r, Err(Error::LengthMismatch(_))));
    }

    #[test]
    fn cantidad_de_columnas_distinta_de_la_de_campos_es_err() {
        let s = schema(&[("a", DataType::Int32, true), ("b", DataType::Int32, true)]);
        let r = Chunk::try_new(s, vec![ints(&[Some(1)])]);
        assert!(matches!(r, Err(Error::LengthMismatch(_))));
    }

    #[test]
    fn tipo_de_columna_distinto_al_del_campo_es_err() {
        let s = schema(&[("a", DataType::Bool, true)]);
        let r = Chunk::try_new(s, vec![ints(&[Some(1)])]);
        assert!(matches!(r, Err(Error::TypeMismatch(_))));
    }

    #[test]
    fn campo_no_nullable_con_nulos_es_err() {
        let s = schema(&[("a", DataType::Int32, false)]);
        let r = Chunk::try_new(s, vec![ints(&[Some(1), None])]);
        assert!(matches!(r, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn campo_nullable_acepta_nulos() {
        let s = schema(&[("a", DataType::Int32, true)]);
        assert!(Chunk::try_new(s, vec![ints(&[Some(1), None])]).is_ok());
    }

    #[test]
    fn campo_no_nullable_acepta_columna_sin_nulos_aunque_tenga_validity() {
        // Un slice que deja fuera los nulos reduce null_count a 0.
        let col = ints(&[Some(1), None]).slice(0, 1).unwrap();
        let s = schema(&[("a", DataType::Int32, false)]);
        assert!(Chunk::try_new(s, vec![col]).is_ok());
    }

    #[test]
    fn bool_en_chunk() {
        let mut b = BoolBuilder::new();
        b.append(true);
        let s = schema(&[("ok", DataType::Bool, false)]);
        assert_eq!(
            Chunk::try_new(s, vec![b.finish().unwrap()]).unwrap().len(),
            1
        );
    }
}
