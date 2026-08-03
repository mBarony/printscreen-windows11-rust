//! Erro mínimo da aplicação (substitui o crate `anyhow`).
//!
//! Um erro é uma cadeia de mensagens: `contexto: causa`. O formato `{err}`
//! imprime a cadeia completa (equivalente ao `{err:#}` do anyhow, que o
//! código usava em todos os pontos de exibição).

use std::fmt;

#[derive(Debug, Clone)]
pub struct Error(String);

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// Nota: `Error` NÃO implementa `std::error::Error` de propósito — isso
// permite o `From` genérico abaixo sem colidir com o impl reflexivo
// (mesma decisão de projeto do anyhow).
impl<E: std::error::Error> From<E> for Error {
    fn from(err: E) -> Self {
        // Achata a cadeia de `source()` em uma única mensagem legível.
        let mut text = err.to_string();
        let mut source = err.source();
        while let Some(cause) = source {
            text.push_str(": ");
            text.push_str(&cause.to_string());
            source = cause.source();
        }
        Self(text)
    }
}

/// Equivalente do `anyhow::anyhow!`.
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::error::Error::msg(format!($($arg)*))
    };
}
pub(crate) use err;

/// Anexa contexto a `Result`/`Option`, como o `anyhow::Context`.
pub trait Context<T> {
    fn context(self, message: impl fmt::Display) -> Result<T>;
    fn with_context(self, message: impl FnOnce() -> String) -> Result<T>;
}

impl<T, E: Into<Error>> Context<T> for std::result::Result<T, E> {
    fn context(self, message: impl fmt::Display) -> Result<T> {
        self.map_err(|e| Error(format!("{message}: {}", e.into())))
    }

    fn with_context(self, message: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|e| Error(format!("{}: {}", message(), e.into())))
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, message: impl fmt::Display) -> Result<T> {
        self.ok_or_else(|| Error(message.to_string()))
    }

    fn with_context(self, message: impl FnOnce() -> String) -> Result<T> {
        self.ok_or_else(|| Error(message()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_chains_messages() {
        let base: std::result::Result<(), std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "sumiu"));
        let err = base.context("lendo arquivo").unwrap_err();
        assert_eq!(err.to_string(), "lendo arquivo: sumiu");
    }

    #[test]
    fn option_context() {
        let none: Option<u8> = None;
        assert_eq!(none.context("vazio").unwrap_err().to_string(), "vazio");
    }

    #[test]
    fn err_macro_formats() {
        let e = err!("falha {} de {}", 1, 2);
        assert_eq!(e.to_string(), "falha 1 de 2");
    }
}
