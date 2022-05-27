#[derive(Debug)]
pub enum JImageError {
    ReadError(String, usize),
    IOError(std::io::Error),
}

impl From<std::io::Error> for JImageError {
    fn from(x: std::io::Error) -> Self {
        Self::IOError(x)
    }
}
