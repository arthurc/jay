use std::fmt;

pub struct Attribute {
    pub attribute_name_index: u16,
    pub info: Vec<u8>,
}
impl fmt::Debug for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attribute")
            .field("attribute_name_index", &self.attribute_name_index)
            .field("info", &format!("({} bytes)", self.info.len()))
            .finish()
    }
}

#[derive(Debug)]
pub struct Attributes(Vec<Attribute>);
impl Attributes {
    pub fn new(attributes: Vec<Attribute>) -> Self {
        Self(attributes)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Attribute> {
        self.0.iter()
    }
}

#[derive(Debug)]
pub struct ExceptionTableEntry {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}

#[derive(Debug)]
pub struct CodeAttribute {
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<u8>,
    pub exception_table: Vec<ExceptionTableEntry>,
    pub attributes: Attributes,
}
