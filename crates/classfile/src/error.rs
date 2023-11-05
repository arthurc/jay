use thiserror::Error;

use crate::Constant;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Read error at {1}: {0}")]
    ReadError(String, usize),
    #[error("Unexpected constant: {0}")]
    UnexpectedConstant(String),
    #[error("Constant not found: {0}")]
    ConstantNotFound(u16),
    #[error("Attribute not found: {0}")]
    AttributeNotFound(String),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}
impl Error {
    #[inline]
    pub fn unexpected_constant(expected: &str, constant: &Constant) -> Self {
        Self::UnexpectedConstant(format!("Expected {expected}, found {constant:?}"))
    }
}
