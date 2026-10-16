//! Tipos de dato soportados por la fase 0.

/// Tipo logico de una columna. Subconjunto cerrado de Arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// Booleano, un bit por valor.
    Bool,
    /// Entero con signo de 32 bits.
    Int32,
    /// Entero con signo de 64 bits.
    Int64,
    /// Flotante IEEE 754 de 64 bits.
    Float64,
    /// Cadena UTF-8 de longitud variable.
    Utf8,
}

#[cfg(test)]
mod tests {
    use super::DataType;

    #[test]
    fn tiene_los_cinco_tipos() {
        let todos = [
            DataType::Bool,
            DataType::Int32,
            DataType::Int64,
            DataType::Float64,
            DataType::Utf8,
        ];
        assert_eq!(todos.len(), 5);
    }
}
