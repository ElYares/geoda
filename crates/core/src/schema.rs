//! `Field` y `Schema`.

use std::collections::HashSet;

use crate::{DataType, Error};

/// Una columna con nombre dentro de un `Schema`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    name: String,
    dtype: DataType,
    nullable: bool,
}

impl Field {
    /// Crea un campo.
    pub fn new(name: impl Into<String>, dtype: DataType, nullable: bool) -> Self {
        Self {
            name: name.into(),
            dtype,
            nullable,
        }
    }

    /// Nombre del campo.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Tipo de dato del campo.
    #[must_use]
    pub fn dtype(&self) -> DataType {
        self.dtype
    }

    /// Si el campo admite nulos.
    #[must_use]
    pub fn nullable(&self) -> bool {
        self.nullable
    }
}

/// Lista ordenada de campos con nombres unicos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    fields: Vec<Field>,
}

impl Schema {
    /// Construye un schema validando que los nombres sean unicos.
    ///
    /// # Errors
    ///
    /// Devuelve [`Error::InvalidArgument`] si dos campos comparten nombre.
    pub fn try_new(fields: Vec<Field>) -> Result<Self, Error> {
        let mut vistos = HashSet::new();
        for field in &fields {
            if !vistos.insert(field.name()) {
                return Err(Error::InvalidArgument(format!(
                    "campo repetido: {}",
                    field.name()
                )));
            }
        }
        Ok(Self { fields })
    }

    /// Campos en orden.
    #[must_use]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Numero de campos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Si no tiene campos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Busca un campo por nombre.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::{Field, Schema};
    use crate::{DataType, Error};

    #[test]
    fn nombres_distintos_son_validos() {
        let s = Schema::try_new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("nombre", DataType::Utf8, true),
        ])
        .unwrap();
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn nombre_repetido_es_error() {
        let r = Schema::try_new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("id", DataType::Int32, true),
        ]);
        assert!(matches!(r, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn schema_vacio_es_valido() {
        let s = Schema::try_new(vec![]).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn busca_campo_por_nombre() {
        let s = Schema::try_new(vec![Field::new("x", DataType::Float64, true)]).unwrap();
        assert!(s.field("x").is_some());
        assert!(s.field("y").is_none());
    }

    #[test]
    fn debug_es_estable() {
        let s = Schema::try_new(vec![Field::new("x", DataType::Bool, false)]).unwrap();
        assert_eq!(
            format!("{s:?}"),
            r#"Schema { fields: [Field { name: "x", dtype: Bool, nullable: false }] }"#
        );
    }
}
