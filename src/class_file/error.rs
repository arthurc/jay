#[derive(Debug)]
pub enum ClassFileError {
    ReadError(String, usize),
    IOError(std::io::Error),
}

impl From<std::io::Error> for ClassFileError {
    fn from(x: std::io::Error) -> Self {
        Self::IOError(x)
    }
}
