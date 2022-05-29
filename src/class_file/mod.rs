mod access_flags;
mod attributes;
mod constant_pool;
mod error;
mod parser;

use std::io::{Read, Seek};

pub use access_flags::*;
pub use attributes::*;
pub use constant_pool::ConstantPool;
pub use error::ClassFileError;
pub use parser::ParseEvent;

pub fn parse<F: FnMut(ParseEvent)>(bytes: impl Read + Seek, f: F) -> Result<(), ClassFileError> {
    parser::Parser::new(bytes, f).parse()?;

    Ok(())
}

#[derive(Debug)]
pub struct ClassInfo {
    pub access_flags: AccessFlags,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
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
        let mut constant_pool = ConstantPool::default();
        parse(
            Cursor::new(include_bytes!("../../classes/com/example/Main.class")),
            |event| {
                if let ParseEvent::ConstantPool(pool) = event {
                    constant_pool = pool;
                }
            },
        )
        .unwrap();

        assert!(constant_pool.contains(&CpInfo::Utf8(String::from("<init>"))));
        assert!(constant_pool.contains(&CpInfo::Utf8(String::from("main"))));
        assert!(constant_pool.contains(&CpInfo::Utf8(String::from("Code"))));
    }
}
