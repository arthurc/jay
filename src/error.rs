#[derive(Debug)]
pub enum JayError {
    IOError(std::io::Error),
}

impl From<std::io::Error> for JayError {
    fn from(x: std::io::Error) -> Self {
        Self::IOError(x)
    }
}
