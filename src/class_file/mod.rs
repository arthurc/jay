mod access_flags;
mod attributes;
pub(crate) mod constant_pool;
mod error;
mod parser;

use std::io::{Read, Seek};

pub use access_flags::*;
pub use attributes::*;
pub use constant_pool::ConstantPool;
pub use error::ClassFileError;

#[derive(Debug)]
pub struct ClassFile {
    pub constant_pool: ConstantPool,
    pub access_flags: AccessFlags,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub attributes: Vec<Attribute>,
}
impl ClassFile {
    pub fn parse(bytes: impl Read + Seek) -> Result<ClassFile, ClassFileError> {
        Ok(parser::Parser::new(bytes).parse()?)
    }
}

#[derive(Debug)]
pub struct FieldInfo {
    pub access_flags: AccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug)]
pub struct MethodInfo {
    pub access_flags: AccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<Attribute>,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::class_file::constant_pool::CpInfo;

    use super::*;

    #[test]
    fn test_parse_cp_info_utf8() {
        let class_file = ClassFile::parse(Cursor::new(include_bytes!(
            "../../classes/com/example/Main.class"
        )))
        .unwrap();

        assert!(class_file
            .constant_pool
            .contains(&CpInfo::Utf8(String::from("<init>"))));
        assert!(class_file
            .constant_pool
            .contains(&CpInfo::Utf8(String::from("main"))));
        assert!(class_file
            .constant_pool
            .contains(&CpInfo::Utf8(String::from("Code"))));
    }
}
