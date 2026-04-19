pub mod classfile;
pub mod classpath;
pub mod cli;
pub mod vm;

use std::fmt;

pub type JayResult<T> = Result<T, JayError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JayError {
    message: String,
}

impl JayError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for JayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for JayError {}

impl From<std::io::Error> for JayError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}
