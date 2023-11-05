use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unknown bytecode: {0:#02x}")]
    UnknownBytecode(u8),
    #[error(transparent)]
    IO(#[from] io::Error),
}
