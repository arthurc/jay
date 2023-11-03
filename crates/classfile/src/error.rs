use crate::Constant;

#[derive(Debug)]
pub enum Error {
    ReadError(String, usize),
    UnexpectedConstant(String),
    ConstantNotFound(u16),
    AttributeNotFound(String),
    IO(std::io::Error),
}
impl Error {
    #[inline]
    pub fn unexpected_constant(expected: &str, constant: &Constant) -> Self {
        Self::UnexpectedConstant(format!("Expected {expected}, found {constant:?}"))
    }
}
impl From<std::io::Error> for Error {
    fn from(x: std::io::Error) -> Self {
        Self::IO(x)
    }
}
