use crate::jimage;

#[derive(Debug)]
pub enum JayError {
    JImageError(jimage::JImageError),
}
