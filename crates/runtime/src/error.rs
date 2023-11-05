use std::io;

use thiserror::Error;

use crate::{bytecode, classfile, jimage};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    JImageError(#[from] jimage::Error),
    #[error(transparent)]
    ClassFile(#[from] classfile::Error),
    #[error("Class load error: {0}")]
    ClassLoadError(String),
    #[error("No such method: {0}")]
    NoSuchMethod(String),
    #[error(transparent)]
    Bytecode(#[from] bytecode::Error),
    #[error(transparent)]
    IOError(#[from] io::Error),
}
