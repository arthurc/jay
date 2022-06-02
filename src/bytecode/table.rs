pub struct Descriptor {
    pub mnemonic: &'static str,
    pub format: Format,
}

pub enum Format {
    NoIndex(fn()),
    Index(fn(u8)),
    WideIndex(fn(u16)),
}

pub const SIZE: usize = u8::MAX as usize + 1;

pub struct Builder {
    pub table: [Option<Descriptor>; SIZE],
}
impl Builder {
    pub const fn new() -> Self {
        const INIT: Option<Descriptor> = None;
        Self {
            table: [INIT; SIZE],
        }
    }

    pub const fn build(mut self, opcode: u8, mnemonic: &'static str, format: Format) -> Self {
        self.table[opcode as usize] = Some(Descriptor { mnemonic, format });
        self
    }
}
