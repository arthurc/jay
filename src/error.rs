use crate::{
    class_file::{self, ClassFileError},
    jimage::{self, JImageError},
};

#[derive(Debug)]
pub enum JayError {
    NotFound(String),
    JImageError(jimage::JImageError),
    ClassFileError(class_file::ClassFileError),
}

impl From<JImageError> for JayError {
    fn from(x: JImageError) -> Self {
        JayError::JImageError(x)
    }
}

impl From<ClassFileError> for JayError {
    fn from(x: ClassFileError) -> Self {
        JayError::ClassFileError(x)
    }
}
