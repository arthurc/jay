use crate::{JayError, JayResult};

#[derive(Debug, Clone)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub this_class: String,
    pub super_class: Option<String>,
    pub methods: Vec<Method>,
}

impl ClassFile {
    pub fn parse(bytes: &[u8]) -> JayResult<Self> {
        Parser::new(bytes).parse_class()
    }

    pub fn find_method(&self, name: &str, descriptor: &str) -> Option<&Method> {
        self.methods
            .iter()
            .find(|method| method.name == name && method.descriptor == descriptor)
    }
}

#[derive(Debug, Clone)]
pub struct Method {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub code: Option<Code>,
}

impl Method {
    pub fn is_static(&self) -> bool {
        self.access_flags & 0x0008 != 0
    }

    pub fn is_public(&self) -> bool {
        self.access_flags & 0x0001 != 0
    }
}

#[derive(Debug, Clone)]
pub struct Code {
    pub max_stack: u16,
    pub max_locals: u16,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ConstantPool {
    entries: Vec<CpEntry>,
}

impl ConstantPool {
    pub fn utf8(&self, index: u16) -> JayResult<&str> {
        match self.entry(index)? {
            CpEntry::Utf8(value) => Ok(value),
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not UTF-8: {other:?}"
            ))),
        }
    }

    pub fn class_name(&self, index: u16) -> JayResult<&str> {
        match self.entry(index)? {
            CpEntry::Class { name_index } => self.utf8(*name_index),
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not a class: {other:?}"
            ))),
        }
    }

    pub fn string(&self, index: u16) -> JayResult<&str> {
        match self.entry(index)? {
            CpEntry::String { string_index } => self.utf8(*string_index),
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not a string: {other:?}"
            ))),
        }
    }

    pub fn integer(&self, index: u16) -> JayResult<i32> {
        match self.entry(index)? {
            CpEntry::Integer(value) => Ok(*value),
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not an integer: {other:?}"
            ))),
        }
    }

    pub fn field_ref(&self, index: u16) -> JayResult<MemberRef<'_>> {
        match self.entry(index)? {
            CpEntry::FieldRef {
                class_index,
                name_and_type_index,
            } => self.member_ref(*class_index, *name_and_type_index),
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not a field reference: {other:?}"
            ))),
        }
    }

    pub fn method_ref(&self, index: u16) -> JayResult<MemberRef<'_>> {
        match self.entry(index)? {
            CpEntry::MethodRef {
                class_index,
                name_and_type_index,
            }
            | CpEntry::InterfaceMethodRef {
                class_index,
                name_and_type_index,
            } => self.member_ref(*class_index, *name_and_type_index),
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not a method reference: {other:?}"
            ))),
        }
    }

    fn member_ref(&self, class_index: u16, name_and_type_index: u16) -> JayResult<MemberRef<'_>> {
        let class_name = self.class_name(class_index)?;
        match self.entry(name_and_type_index)? {
            CpEntry::NameAndType {
                name_index,
                descriptor_index,
            } => Ok(MemberRef {
                class_name,
                name: self.utf8(*name_index)?,
                descriptor: self.utf8(*descriptor_index)?,
            }),
            other => Err(JayError::new(format!(
                "constant pool entry #{name_and_type_index} is not name-and-type: {other:?}"
            ))),
        }
    }

    fn entry(&self, index: u16) -> JayResult<&CpEntry> {
        self.entries
            .get(index as usize)
            .filter(|entry| !matches!(entry, CpEntry::Unusable))
            .ok_or_else(|| JayError::new(format!("invalid constant pool index #{index}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRef<'a> {
    pub class_name: &'a str,
    pub name: &'a str,
    pub descriptor: &'a str,
}

#[derive(Debug, Clone)]
enum CpEntry {
    Unusable,
    Utf8(String),
    Integer(i32),
    Float,
    Long,
    Double,
    Class {
        name_index: u16,
    },
    String {
        string_index: u16,
    },
    FieldRef {
        class_index: u16,
        name_and_type_index: u16,
    },
    MethodRef {
        class_index: u16,
        name_and_type_index: u16,
    },
    InterfaceMethodRef {
        class_index: u16,
        name_and_type_index: u16,
    },
    NameAndType {
        name_index: u16,
        descriptor_index: u16,
    },
    MethodHandle,
    MethodType,
    Dynamic,
    InvokeDynamic,
    Module,
    Package,
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn parse_class(&mut self) -> JayResult<ClassFile> {
        let magic = self.read_u4()?;
        if magic != 0xCAFEBABE {
            return Err(JayError::new("invalid class file magic"));
        }

        let minor_version = self.read_u2()?;
        let major_version = self.read_u2()?;
        if !(45..=65).contains(&major_version) {
            return Err(JayError::new(format!(
                "unsupported class file major version {major_version}; expected Java 21 or older"
            )));
        }

        let constant_pool = self.parse_constant_pool()?;

        let _access_flags = self.read_u2()?;
        let this_class_index = self.read_u2()?;
        let super_class_index = self.read_u2()?;
        let this_class = constant_pool.class_name(this_class_index)?.to_string();
        let super_class = if super_class_index == 0 {
            None
        } else {
            Some(constant_pool.class_name(super_class_index)?.to_string())
        };

        self.skip_interfaces()?;
        self.skip_fields()?;
        let methods = self.parse_methods(&constant_pool)?;
        self.skip_attributes()?;

        if self.offset != self.bytes.len() {
            return Err(JayError::new("trailing bytes after class file"));
        }

        Ok(ClassFile {
            minor_version,
            major_version,
            constant_pool,
            this_class,
            super_class,
            methods,
        })
    }

    fn parse_constant_pool(&mut self) -> JayResult<ConstantPool> {
        let count = self.read_u2()?;
        let mut entries = Vec::with_capacity(count as usize);
        entries.push(CpEntry::Unusable);

        let mut index = 1;
        while index < count {
            let tag = self.read_u1()?;
            let entry = match tag {
                1 => {
                    let length = self.read_u2()? as usize;
                    let bytes = self.read_bytes(length)?;
                    let value = String::from_utf8(bytes.to_vec())
                        .map_err(|_| JayError::new("invalid UTF-8 in constant pool"))?;
                    CpEntry::Utf8(value)
                }
                3 => CpEntry::Integer(self.read_u4()? as i32),
                4 => {
                    self.skip(4)?;
                    CpEntry::Float
                }
                5 => {
                    self.skip(8)?;
                    entries.push(CpEntry::Long);
                    entries.push(CpEntry::Unusable);
                    index += 2;
                    continue;
                }
                6 => {
                    self.skip(8)?;
                    entries.push(CpEntry::Double);
                    entries.push(CpEntry::Unusable);
                    index += 2;
                    continue;
                }
                7 => CpEntry::Class {
                    name_index: self.read_u2()?,
                },
                8 => CpEntry::String {
                    string_index: self.read_u2()?,
                },
                9 => CpEntry::FieldRef {
                    class_index: self.read_u2()?,
                    name_and_type_index: self.read_u2()?,
                },
                10 => CpEntry::MethodRef {
                    class_index: self.read_u2()?,
                    name_and_type_index: self.read_u2()?,
                },
                11 => CpEntry::InterfaceMethodRef {
                    class_index: self.read_u2()?,
                    name_and_type_index: self.read_u2()?,
                },
                12 => CpEntry::NameAndType {
                    name_index: self.read_u2()?,
                    descriptor_index: self.read_u2()?,
                },
                15 => {
                    self.skip(3)?;
                    CpEntry::MethodHandle
                }
                16 => {
                    self.skip(2)?;
                    CpEntry::MethodType
                }
                17 => {
                    self.skip(4)?;
                    CpEntry::Dynamic
                }
                18 => {
                    self.skip(4)?;
                    CpEntry::InvokeDynamic
                }
                19 => {
                    self.skip(2)?;
                    CpEntry::Module
                }
                20 => {
                    self.skip(2)?;
                    CpEntry::Package
                }
                _ => {
                    return Err(JayError::new(format!(
                        "unsupported constant pool tag {tag}"
                    )));
                }
            };

            entries.push(entry);
            index += 1;
        }

        Ok(ConstantPool { entries })
    }

    fn skip_interfaces(&mut self) -> JayResult<()> {
        let count = self.read_u2()? as usize;
        self.skip(count * 2)
    }

    fn skip_fields(&mut self) -> JayResult<()> {
        let count = self.read_u2()? as usize;
        for _ in 0..count {
            self.skip(6)?;
            self.skip_attributes()?;
        }
        Ok(())
    }

    fn parse_methods(&mut self, constant_pool: &ConstantPool) -> JayResult<Vec<Method>> {
        let count = self.read_u2()? as usize;
        let mut methods = Vec::with_capacity(count);
        for _ in 0..count {
            let access_flags = self.read_u2()?;
            let name = constant_pool.utf8(self.read_u2()?)?.to_string();
            let descriptor = constant_pool.utf8(self.read_u2()?)?.to_string();
            let attributes_count = self.read_u2()? as usize;
            let mut code = None;

            for _ in 0..attributes_count {
                let name_index = self.read_u2()?;
                let attribute_name = constant_pool.utf8(name_index)?;
                let attribute_length = self.read_u4()? as usize;
                if attribute_name == "Code" {
                    let code_end = self.offset.checked_add(attribute_length).ok_or_else(|| {
                        JayError::new("class file offset overflow while reading Code")
                    })?;
                    code = Some(self.parse_code(constant_pool)?);
                    if self.offset != code_end {
                        return Err(JayError::new("Code attribute length mismatch"));
                    }
                } else {
                    self.skip(attribute_length)?;
                }
            }

            methods.push(Method {
                access_flags,
                name,
                descriptor,
                code,
            });
        }
        Ok(methods)
    }

    fn parse_code(&mut self, constant_pool: &ConstantPool) -> JayResult<Code> {
        let max_stack = self.read_u2()?;
        let max_locals = self.read_u2()?;
        let code_length = self.read_u4()? as usize;
        let bytes = self.read_bytes(code_length)?.to_vec();

        let exception_table_length = self.read_u2()? as usize;
        self.skip(exception_table_length * 8)?;
        self.skip_attributes_named(constant_pool)?;

        Ok(Code {
            max_stack,
            max_locals,
            bytes,
        })
    }

    fn skip_attributes(&mut self) -> JayResult<()> {
        let count = self.read_u2()? as usize;
        for _ in 0..count {
            self.skip(2)?;
            let length = self.read_u4()? as usize;
            self.skip(length)?;
        }
        Ok(())
    }

    fn skip_attributes_named(&mut self, constant_pool: &ConstantPool) -> JayResult<()> {
        let count = self.read_u2()? as usize;
        for _ in 0..count {
            let name_index = self.read_u2()?;
            let _ = constant_pool.utf8(name_index)?;
            let length = self.read_u4()? as usize;
            self.skip(length)?;
        }
        Ok(())
    }

    fn read_u1(&mut self) -> JayResult<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u2(&mut self) -> JayResult<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_u4(&mut self) -> JayResult<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_bytes(&mut self, length: usize) -> JayResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| JayError::new("class file offset overflow"))?;
        if end > self.bytes.len() {
            return Err(JayError::new("unexpected end of class file"));
        }
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn skip(&mut self, length: usize) -> JayResult<()> {
        self.read_bytes(length).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_magic() {
        let error = ClassFile::parse(b"not a class").unwrap_err();

        assert!(error.to_string().contains("invalid class file magic"));
    }

    #[test]
    fn rejects_truncated_class_file() {
        let error = ClassFile::parse(&[0xCA, 0xFE, 0xBA, 0xBE]).unwrap_err();

        assert!(error.to_string().contains("unexpected end"));
    }
}
