use std::{fmt, ops::Index};

use super::*;

#[derive(Debug, PartialEq)]
pub enum Constant {
    MethodRef(RefInfo),
    FieldRef(RefInfo),
    InterfaceMethodRef(RefInfo),
    Class(ClassInfo),
    NameAndType(NameAndTypeInfo),
    Utf8(String),
    String { string_index: u16 },
    InvokeDynamic(InvokeDynamicInfo),
    Integer(i32),
    MethodHandle(MethodHandleInfo),
    MethodType(MethodTypeInfo),
    Long(i64),
    Unusable,
}
impl Constant {
    pub fn to_utf8(&self) -> Result<&str> {
        match self {
            Self::Utf8(s) => Ok(s),
            _ => Err(ClassFileError::UnexpectedConstantPoolEntry(format!(
                "Expected Utf8, found {:?}",
                self,
            ))),
        }
    }

    pub fn to_class_info(&self) -> Result<&ClassInfo> {
        match self {
            Self::Class(i) => Ok(i),
            _ => Err(ClassFileError::UnexpectedConstantPoolEntry(format!(
                "Expected ClassInfo, found {:?}",
                self,
            ))),
        }
    }

    pub fn to_name_and_type(&self) -> Result<&NameAndTypeInfo> {
        match self {
            Self::NameAndType(n) => Ok(n),
            _ => Err(ClassFileError::UnexpectedConstantPoolEntry(format!(
                "Expected NameAndType, found {:?}",
                self,
            ))),
        }
    }

    pub(crate) fn to_field_ref(&self) -> Result<&RefInfo> {
        match self {
            Self::FieldRef(n) => Ok(n),
            _ => Err(ClassFileError::UnexpectedConstantPoolEntry(format!(
                "Expected FieldRef, found {:?}",
                self,
            ))),
        }
    }
}

#[derive(Default)]
pub struct ConstantPool {
    constants: Vec<Constant>,
}
impl ConstantPool {
    pub fn new(constants: Vec<Constant>) -> Self {
        Self { constants }
    }

    pub fn contains(&self, cp_info: &Constant) -> bool {
        self.constants.contains(cp_info)
    }

    fn fmt_class_info(&self, ClassInfo { name_index }: &ClassInfo) -> String {
        format!(
            "{{ name_index: {} ({:?}) }}",
            name_index,
            self.constants[*name_index as usize - 1]
                .to_utf8()
                .unwrap_or("???")
        )
    }

    fn fmt_name_and_type_index(
        &self,
        NameAndTypeInfo {
            name_index,
            descriptor_index,
        }: &NameAndTypeInfo,
    ) -> String {
        format!(
            "{{ name_index: {} ({:?}), descriptor_index: {} ({:?}) }}",
            name_index,
            self.constants[*name_index as usize - 1]
                .to_utf8()
                .unwrap_or("???"),
            descriptor_index,
            self.constants[*descriptor_index as usize - 1]
                .to_utf8()
                .unwrap_or("???")
        )
    }

    fn fmt_ref_info(
        &self,
        RefInfo {
            class_index,
            name_and_type_index,
        }: &RefInfo,
    ) -> String {
        format!(
            "{{ class_index: {} ({}), name_and_type_index: {} ({}) }}",
            class_index,
            self.constants[*class_index as usize - 1]
                .to_class_info()
                .map(|i| self.fmt_class_info(i))
                .unwrap_or_else(|_| String::from("???")),
            name_and_type_index,
            self.constants[*name_and_type_index as usize - 1]
                .to_name_and_type()
                .map(|nt| self.fmt_name_and_type_index(nt))
                .unwrap_or_else(|_| String::from("???"))
        )
    }
}
impl Index<u16> for ConstantPool {
    type Output = Constant;

    fn index(&self, index: u16) -> &Self::Output {
        &self.constants[index as usize - 1]
    }
}
impl<'a> IntoIterator for &'a ConstantPool {
    type Item = &'a Constant;
    type IntoIter = std::slice::Iter<'a, Constant>;

    fn into_iter(self) -> Self::IntoIter {
        self.constants.iter()
    }
}
impl fmt::Debug for ConstantPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[")?;
        for (index, i) in self.constants.iter().enumerate() {
            write!(f, "    {}. ", index + 1)?;
            match i {
                Constant::Class(ClassInfo { name_index }) => writeln!(
                    f,
                    "Class {{ name_index: {} ({:?}) }}",
                    name_index,
                    self.constants[*name_index as usize - 1]
                        .to_utf8()
                        .unwrap_or("???")
                )?,
                Constant::FieldRef(ref_info) => {
                    writeln!(f, "FieldRef {}", self.fmt_ref_info(ref_info))?
                }
                Constant::MethodRef(ref_info) => {
                    writeln!(f, "MethodRef {}", self.fmt_ref_info(ref_info))?
                }
                Constant::NameAndType(name_and_type) => writeln!(
                    f,
                    "NameAndType {}",
                    self.fmt_name_and_type_index(name_and_type),
                )?,
                Constant::Utf8(s) => writeln!(f, "Utf8: {:?}", s)?,
                Constant::Long(l) => writeln!(f, "Long: {}", l)?,
                Constant::String { string_index } => writeln!(
                    f,
                    "String: {{ string_index: {} ({:?}) }}",
                    string_index,
                    self.constants[*string_index as usize - 1]
                        .to_utf8()
                        .unwrap_or("???")
                )?,
                _ => writeln!(f, "???")?,
            }
        }
        write!(f, "]")?;

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct RefInfo {
    pub class_index: u16,
    pub name_and_type_index: u16,
}

#[derive(Debug, PartialEq)]
pub struct ClassInfo {
    pub name_index: u16,
}

#[derive(Debug, PartialEq)]
pub struct NameAndTypeInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
}

#[derive(Debug, PartialEq)]
pub struct InvokeDynamicInfo {
    pub bootstrap_method_attr_index: u16,
    pub name_and_type_index: u16,
}

#[derive(Debug, PartialEq)]
pub struct MethodHandleInfo {
    pub reference_kind: u8,
    pub reference_index: u16,
}

#[derive(Debug, PartialEq)]
pub struct MethodTypeInfo {
    pub descriptor_index: u16,
}
