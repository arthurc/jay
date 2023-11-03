use std::io;

use crate::{bytecode, classfile, jimage};

#[derive(Debug)]
pub enum Error {
    NotFound(String),
    JImageError(jimage::Error),
    ClassFile(classfile::Error),
    ClassLoadError(String),
    NoSuchMethod(String),
    BytecodeError(bytecode::BytecodeError),
    IOError(io::Error),
}

impl From<io::Error> for Error {
    fn from(x: io::Error) -> Self {
        Error::IOError(x)
    }
}

impl From<jimage::Error> for Error {
    fn from(x: jimage::Error) -> Self {
        Error::JImageError(x)
    }
}

impl From<classfile::Error> for Error {
    fn from(x: classfile::Error) -> Self {
        Error::ClassFile(x)
    }
}

impl From<bytecode::BytecodeError> for Error {
    fn from(x: bytecode::BytecodeError) -> Self {
        Error::BytecodeError(x)
    }
}
