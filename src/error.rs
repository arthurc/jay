use crate::jimage::{self, JImageError};

#[derive(Debug)]
pub enum JayError {
    NotFound(String),
    JImageError(jimage::JImageError),
}

impl From<JImageError> for JayError {
    fn from(x: JImageError) -> Self {
        JayError::JImageError(x)
    }
}
