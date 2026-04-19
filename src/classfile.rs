use crate::{JayError, JayResult};

const MIN_SUPPORTED_MAJOR_VERSION: u16 = 45;
const MAX_SUPPORTED_MAJOR_VERSION: u16 = 69;
const MAX_SUPPORTED_JAVA_VERSION: u16 = 25;

#[derive(Debug, Clone)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub this_class: String,
    pub super_class: Option<String>,
    pub methods: Vec<Method>,
    pub fields: Vec<Field>,
    pub bootstrap_methods: Vec<BootstrapMethod>,
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

    pub fn has_field(&self, name: &str, descriptor: &str) -> bool {
        self.fields
            .iter()
            .any(|field| field.name == name && field.descriptor == descriptor)
    }
}

#[derive(Debug, Clone)]
pub struct Method {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub code: Option<Code>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub descriptor: String,
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

    pub fn method_handle(&self, index: u16) -> JayResult<MethodHandleRef> {
        match self.entry(index)? {
            CpEntry::MethodHandle {
                reference_kind,
                reference_index,
            } => Ok(MethodHandleRef {
                reference_kind: *reference_kind,
                reference_index: *reference_index,
            }),
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not a method handle: {other:?}"
            ))),
        }
    }

    pub fn invoke_dynamic(&self, index: u16) -> JayResult<InvokeDynamicRef<'_>> {
        match self.entry(index)? {
            CpEntry::InvokeDynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
            } => match self.entry(*name_and_type_index)? {
                CpEntry::NameAndType {
                    name_index,
                    descriptor_index,
                } => Ok(InvokeDynamicRef {
                    bootstrap_method_attr_index: *bootstrap_method_attr_index,
                    name: self.utf8(*name_index)?,
                    descriptor: self.utf8(*descriptor_index)?,
                }),
                other => Err(JayError::new(format!(
                    "constant pool entry #{name_and_type_index} is not name-and-type: {other:?}"
                ))),
            },
            other => Err(JayError::new(format!(
                "constant pool entry #{index} is not an invokedynamic reference: {other:?}"
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

/// A `CONSTANT_InvokeDynamic` reference resolved to its call-site name and descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeDynamicRef<'a> {
    pub bootstrap_method_attr_index: u16,
    pub name: &'a str,
    pub descriptor: &'a str,
}

/// A `CONSTANT_MethodHandle` entry used by bootstrap method metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodHandleRef {
    pub reference_kind: u8,
    pub reference_index: u16,
}

/// One entry from a class-level `BootstrapMethods` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapMethod {
    pub method_ref: u16,
    pub arguments: Vec<u16>,
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
    MethodHandle {
        reference_kind: u8,
        reference_index: u16,
    },
    MethodType,
    Dynamic,
    InvokeDynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
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
        if !(MIN_SUPPORTED_MAJOR_VERSION..=MAX_SUPPORTED_MAJOR_VERSION).contains(&major_version) {
            return Err(JayError::new(format!(
                "unsupported class file major version {major_version}; expected Java {MAX_SUPPORTED_JAVA_VERSION} or older"
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
        let fields = self.parse_fields(&constant_pool)?;
        let methods = self.parse_methods(&constant_pool)?;
        let bootstrap_methods = self.parse_class_attributes(&constant_pool)?;

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
            fields,
            bootstrap_methods,
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
                15 => CpEntry::MethodHandle {
                    reference_kind: self.read_u1()?,
                    reference_index: self.read_u2()?,
                },
                16 => {
                    self.skip(2)?;
                    CpEntry::MethodType
                }
                17 => {
                    self.skip(4)?;
                    CpEntry::Dynamic
                }
                18 => CpEntry::InvokeDynamic {
                    bootstrap_method_attr_index: self.read_u2()?,
                    name_and_type_index: self.read_u2()?,
                },
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

    fn parse_fields(&mut self, constant_pool: &ConstantPool) -> JayResult<Vec<Field>> {
        let count = self.read_u2()? as usize;
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count {
            self.skip(2)?;
            let name = constant_pool.utf8(self.read_u2()?)?.to_string();
            let descriptor = constant_pool.utf8(self.read_u2()?)?.to_string();
            self.skip_attributes()?;
            fields.push(Field { name, descriptor });
        }
        Ok(fields)
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

    fn parse_class_attributes(
        &mut self,
        constant_pool: &ConstantPool,
    ) -> JayResult<Vec<BootstrapMethod>> {
        let count = self.read_u2()? as usize;
        let mut bootstrap_methods = Vec::new();
        for _ in 0..count {
            let name_index = self.read_u2()?;
            let attribute_name = constant_pool.utf8(name_index)?;
            let attribute_length = self.read_u4()? as usize;
            if attribute_name == "BootstrapMethods" {
                let attribute_end = self.offset.checked_add(attribute_length).ok_or_else(|| {
                    JayError::new("class file offset overflow while reading BootstrapMethods")
                })?;
                bootstrap_methods = self.parse_bootstrap_methods()?;
                if self.offset != attribute_end {
                    return Err(JayError::new("BootstrapMethods attribute length mismatch"));
                }
            } else {
                self.skip(attribute_length)?;
            }
        }
        Ok(bootstrap_methods)
    }

    fn parse_bootstrap_methods(&mut self) -> JayResult<Vec<BootstrapMethod>> {
        let count = self.read_u2()? as usize;
        let mut bootstrap_methods = Vec::with_capacity(count);
        for _ in 0..count {
            let method_ref = self.read_u2()?;
            let argument_count = self.read_u2()? as usize;
            let mut arguments = Vec::with_capacity(argument_count);
            for _ in 0..argument_count {
                arguments.push(self.read_u2()?);
            }
            bootstrap_methods.push(BootstrapMethod {
                method_ref,
                arguments,
            });
        }
        Ok(bootstrap_methods)
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

    #[test]
    fn accepts_java_25_class_file_major_version() {
        let bytes = [
            0xCA, 0xFE, 0xBA, 0xBE, // magic
            0x00, 0x00, // minor
            0x00, 0x45, // major 69
            0x00, 0x05, // constant_pool_count
            0x07, 0x00, 0x02, // #1 Class #2
            0x01, 0x00, 0x05, b'E', b'm', b'p', b't', b'y', // #2 Utf8 Empty
            0x07, 0x00, 0x04, // #3 Class #4
            0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O',
            b'b', b'j', b'e', b'c', b't', // #4 Utf8 java/lang/Object
            0x00, 0x21, // access_flags
            0x00, 0x01, // this_class
            0x00, 0x03, // super_class
            0x00, 0x00, // interfaces_count
            0x00, 0x00, // fields_count
            0x00, 0x00, // methods_count
            0x00, 0x00, // attributes_count
        ];

        let class_file = ClassFile::parse(&bytes).unwrap();

        assert_eq!(class_file.major_version, 69);
    }

    #[test]
    fn parses_invokedynamic_and_bootstrap_methods() {
        fn push_u2(bytes: &mut Vec<u8>, value: u16) {
            bytes.extend(value.to_be_bytes());
        }

        fn push_u4(bytes: &mut Vec<u8>, value: u32) {
            bytes.extend(value.to_be_bytes());
        }

        fn push_utf8(bytes: &mut Vec<u8>, value: &str) {
            bytes.push(1);
            push_u2(bytes, value.len() as u16);
            bytes.extend(value.as_bytes());
        }

        let mut bytes = Vec::new();
        push_u4(&mut bytes, 0xCAFEBABE);
        push_u2(&mut bytes, 0);
        push_u2(&mut bytes, 69);
        push_u2(&mut bytes, 18);

        bytes.extend([7, 0, 2]); // #1 Class Empty
        push_utf8(&mut bytes, "Empty"); // #2
        bytes.extend([7, 0, 4]); // #3 Class java/lang/Object
        push_utf8(&mut bytes, "java/lang/Object"); // #4
        push_utf8(&mut bytes, "BootstrapMethods"); // #5
        bytes.extend([15, 6, 0, 7]); // #6 MethodHandle REF_invokeStatic #7
        bytes.extend([10, 0, 8, 0, 10]); // #7 Methodref #8.#10
        bytes.extend([7, 0, 9]); // #8 Class StringConcatFactory
        push_utf8(&mut bytes, "java/lang/invoke/StringConcatFactory"); // #9
        bytes.extend([12, 0, 11, 0, 12]); // #10 NameAndType #11:#12
        push_utf8(&mut bytes, "makeConcatWithConstants"); // #11
        push_utf8(
            &mut bytes,
            "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/invoke/CallSite;",
        ); // #12
        bytes.extend([8, 0, 14]); // #13 String #14
        push_utf8(&mut bytes, "Make: \x01"); // #14
        bytes.extend([18, 0, 0, 0, 16]); // #15 InvokeDynamic bootstrap 0, #16
        bytes.extend([12, 0, 11, 0, 17]); // #16 NameAndType #11:#17
        push_utf8(&mut bytes, "(Ljava/lang/String;)Ljava/lang/String;"); // #17

        bytes.extend([0, 0x21]); // access_flags
        bytes.extend([0, 1]); // this_class
        bytes.extend([0, 3]); // super_class
        bytes.extend([0, 0]); // interfaces_count
        bytes.extend([0, 0]); // fields_count
        bytes.extend([0, 0]); // methods_count
        bytes.extend([0, 1]); // attributes_count
        bytes.extend([0, 5]); // BootstrapMethods
        push_u4(&mut bytes, 8);
        bytes.extend([0, 1]); // num_bootstrap_methods
        bytes.extend([0, 6]); // bootstrap_method_ref
        bytes.extend([0, 1]); // num_bootstrap_arguments
        bytes.extend([0, 13]); // recipe string

        let class_file = ClassFile::parse(&bytes).unwrap();

        assert_eq!(
            class_file.bootstrap_methods,
            vec![BootstrapMethod {
                method_ref: 6,
                arguments: vec![13]
            }]
        );
        assert_eq!(
            class_file.constant_pool.method_handle(6).unwrap(),
            MethodHandleRef {
                reference_kind: 6,
                reference_index: 7
            }
        );
        assert_eq!(
            class_file.constant_pool.invoke_dynamic(15).unwrap(),
            InvokeDynamicRef {
                bootstrap_method_attr_index: 0,
                name: "makeConcatWithConstants",
                descriptor: "(Ljava/lang/String;)Ljava/lang/String;"
            }
        );
    }
}
