use std::io;

use crate::{bytecode, class_file, jimage};

#[derive(Debug)]
pub enum JayError {
    NotFound(String),
    JImageError(jimage::JImageError),
    ClassFileError(class_file::ClassFileError),
    ClassLoadError(String),
    NoSuchMethod(String),
    BytecodeError(bytecode::BytecodeError),
    IOError(io::Error),
}

impl From<io::Error> for JayError {
    fn from(x: io::Error) -> Self {
        JayError::IOError(x)
    }
}

impl From<jimage::JImageError> for JayError {
    fn from(x: jimage::JImageError) -> Self {
        JayError::JImageError(x)
    }
}

impl From<class_file::ClassFileError> for JayError {
    fn from(x: class_file::ClassFileError) -> Self {
        JayError::ClassFileError(x)
    }
}

impl From<bytecode::BytecodeError> for JayError {
    fn from(x: bytecode::BytecodeError) -> Self {
        JayError::BytecodeError(x)
    }
}
