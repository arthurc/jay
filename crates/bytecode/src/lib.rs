mod error;

use std::io;

pub use error::*;

pub type Result<T, E = Error> = std::result::Result<T, E>;

include!(concat!(env!("OUT_DIR"), "/bytecodes.rs"));

pub trait BytecodeStream {
    fn read_u8(&mut self) -> io::Result<u8>;

    fn next(&mut self) -> Option<Bytecode>
    where
        Self: Sized,
    {
        match Bytecode::read(self) {
            Ok(bytecode) => Some(bytecode),
            Err(Error::IO(_)) => None,
            Err(e) => {
                eprintln!("Failed to read bytecode: {}", e);
                None
            }
        }
    }
}
