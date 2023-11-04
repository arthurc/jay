use std::fmt;

mod bytecodes;
mod error;
mod table;

use bytecodes::*;
pub use error::*;
use table::{Descriptor, Format};

use Format::*;

pub type Result<T, E = Error> = std::result::Result<T, E>;

const BYTECODE_LOOKUP_TABLE: [Option<Descriptor>; table::SIZE] = table::Builder::new()
    .build(0x01, "aconst_null", NoIndex(aconst_null))
    .build(0x12, "ldc", Index(ldc))
    .build(0x19, "aload", Format::Index(aload))
    .build(0xb2, "getstatic", WideIndex(getstatic))
    .build(0xbd, "anewarray", WideIndex(anewarray))
    .table;

pub trait BytecodeStream {
    fn readb(&mut self) -> u8;

    fn readw(&mut self) -> u16 {
        ((self.readb() as u16) << 8u16) | self.readb() as u16
    }
}

enum Index {
    Byte(u8),
    Wide(u16),
}
impl fmt::Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Byte(b) => write!(f, "{}", b),
            Self::Wide(s) => write!(f, "{}", s),
        }
    }
}

pub struct Bytecode {
    descriptor: &'static Descriptor,
    index: Option<Index>,
}
impl Bytecode {
    pub fn from_stream(stream: &mut dyn BytecodeStream) -> Result<Bytecode> {
        let opcode = stream.readb();
        let descriptor = BYTECODE_LOOKUP_TABLE[opcode as usize]
            .as_ref()
            .ok_or_else(|| Error::UnknownBytecode(opcode))?;
        let index = match descriptor.format {
            Format::NoIndex(_) => None,
            Format::Index(_) => Some(Index::Byte(stream.readb())),
            Format::WideIndex(_) => Some(Index::Wide(stream.readw())),
        };

        Ok(Bytecode { descriptor, index })
    }

    pub fn handle(&self) {
        match (&self.descriptor.format, &self.index) {
            (Format::NoIndex(f), _) => f(),
            (Format::Index(f), Some(Index::Byte(index))) => f(*index),
            (Format::WideIndex(f), Some(Index::Wide(index))) => f(*index),
            _ => panic!("Oh no!"),
        }
    }
}
impl fmt::Display for Bytecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:15}", self.descriptor.mnemonic)?;

        if let Some(index) = self.index.as_ref() {
            write!(f, "#{}", index)?;
        }

        Ok(())
    }
}
