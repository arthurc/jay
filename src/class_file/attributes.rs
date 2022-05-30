use std::{fmt, io::Cursor};

use super::{parser::Parser, ConstantPool};

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
pub struct Attributes(pub Vec<Attribute>);
impl Attributes {
    pub fn find_by_name(&self, name: &str, constant_pool: &ConstantPool) -> Option<&Attribute> {
        self.0.iter().find(
            |Attribute {
                 attribute_name_index,
                 ..
             }| matches!(constant_pool[*attribute_name_index].to_utf8(), Ok(name)),
        )
    }

    pub fn code_attribute(&self, constant_pool: &ConstantPool) -> Option<CodeAttribute> {
        let attribute = self.find_by_name("Code", constant_pool)?;
        Parser::new(Cursor::new(&attribute.info))
            .parse_code_attribute()
            .ok()
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
