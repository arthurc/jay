/// https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html
mod access_flags;
mod attribute;
pub mod constant;
mod error;
mod parser;

pub use access_flags::*;
pub use attribute::*;
pub use constant::{Constant, ConstantPool};
pub use error::*;
use parser::Parser;
use std::io::{Cursor, Read, Seek};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub struct ClassFile {
    pub constant_pool: ConstantPool,
    pub access_flags: AccessFlags,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<FieldInfo>,
    methods: Vec<MethodInfo>,
    pub attributes: Attributes,
}
impl ClassFile {
    pub fn parse(bytes: impl Read + Seek) -> Result<ClassFile> {
        Ok(parser::Parser::new(bytes).parse()?)
    }

    pub fn class_name(&self) -> Result<&str> {
        self.constant_pool.get(self.this_class)?.class_name()
    }

    pub fn super_class_name(&self) -> Result<Option<&str>> {
        if self.super_class == 0 {
            Ok(None)
        } else {
            self.constant_pool
                .get(self.super_class)?
                .class_name()
                .map(Some)
        }
    }

    pub fn methods(&self) -> impl Iterator<Item = MethodRef> {
        self.methods.iter().map(|method_info| MethodRef {
            class_file: self,
            method_info,
        })
    }
}

pub struct MethodRef<'a> {
    class_file: &'a ClassFile,
    method_info: &'a MethodInfo,
}
impl<'a> MethodRef<'a> {
    pub fn name(&self) -> Result<&'a str> {
        self.class_file
            .constant_pool
            .get(self.method_info.name_index)?
            .utf8()
    }

    pub fn access_flags(&self) -> &AccessFlags {
        &self.method_info.access_flags
    }

    pub fn attributes(&self) -> AttributesRef {
        AttributesRef {
            constant_pool: &self.class_file.constant_pool,
            attributes: &self.method_info.attributes,
        }
    }
}

pub struct AttributesRef<'a> {
    constant_pool: &'a ConstantPool,
    attributes: &'a Attributes,
}
impl<'a> AttributesRef<'a> {
    fn by_name(&'a self, name: &str) -> Result<Option<AttributeRef<'a>>> {
        for attribute in self.iter() {
            if attribute.name()? == name {
                return Ok(Some(attribute));
            }
        }
        Ok(None)
    }

    pub fn iter(&self) -> impl Iterator<Item = AttributeRef> {
        self.attributes.iter().map(|attribute| AttributeRef {
            constant_pool: self.constant_pool,
            attribute,
        })
    }

    pub fn code(&self) -> Result<Option<CodeAttribute>> {
        if let Some(attribute) = self.by_name("Code")? {
            Ok(Some(
                Parser::new(Cursor::new(&attribute.attribute.info)).parse_code_attribute()?,
            ))
        } else {
            Ok(None)
        }
    }
}

pub struct AttributeRef<'a> {
    constant_pool: &'a ConstantPool,
    attribute: &'a Attribute,
}
impl AttributeRef<'_> {
    pub fn name(&self) -> Result<&str> {
        self.constant_pool
            .get(self.attribute.attribute_name_index)?
            .utf8()
    }
}

#[derive(Debug)]
pub struct FieldInfo {
    pub access_flags: AccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Attributes,
}

#[derive(Debug)]
pub struct MethodInfo {
    pub access_flags: AccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Attributes,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::constant::Constant;

    use super::*;

    #[test]
    fn test_parse_cp_info_utf8() {
        let class_file = ClassFile::parse(Cursor::new(include_bytes!(
            "../../../classes/com/example/Main.class"
        )))
        .unwrap();

        assert!(class_file
            .constant_pool
            .contains(&Constant::Utf8(String::from("<init>"))));
        assert!(class_file
            .constant_pool
            .contains(&Constant::Utf8(String::from("main"))));
        assert!(class_file
            .constant_pool
            .contains(&Constant::Utf8(String::from("Code"))));
    }
}
