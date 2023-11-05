use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Read error at {1}: {0}")]
    Read(String, usize),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}
