//! Nucleo de geoda. Sin dependencias externas.

/// Version del crate, para comprobar que el workspace compila de punta a punta.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_no_esta_vacia() {
        assert_ne!(version(), "");
    }
}
