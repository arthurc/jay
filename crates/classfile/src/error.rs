#[derive(Debug)]
pub enum ClassFileError {
    ReadError(String, usize),
    UnexpectedConstantPoolEntry(String),
    IO(std::io::Error),
}

impl From<std::io::Error> for ClassFileError {
    fn from(x: std::io::Error) -> Self {
        Self::IO(x)
    }
}
