#[derive(Debug)]
pub enum Error {
    Read(String, usize),
    IO(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(x: std::io::Error) -> Self {
        Self::IO(x)
    }
}
