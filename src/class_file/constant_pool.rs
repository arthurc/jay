use std::{fmt, ops::Index};

#[derive(Default)]
pub struct ConstantPool {
    cp_infos: Vec<CpInfo>,
}
impl ConstantPool {
    pub fn new(cp_infos: Vec<CpInfo>) -> Self {
        Self { cp_infos }
    }

    pub fn contains(&self, cp_info: &CpInfo) -> bool {
        self.cp_infos.contains(cp_info)
    }

    fn fmt_class_info(&self, ClassInfo { name_index }: &ClassInfo) -> String {
        format!(
            "{{ name_index: {} ({:?}) }}",
            name_index,
            self.cp_infos[*name_index as usize - 1]
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
            self.cp_infos[*name_index as usize - 1]
                .to_utf8()
                .unwrap_or("???"),
            descriptor_index,
            self.cp_infos[*descriptor_index as usize - 1]
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
            self.cp_infos[*class_index as usize - 1]
                .to_class_info()
                .map(|i| self.fmt_class_info(i))
                .unwrap_or_else(|| String::from("???")),
            name_and_type_index,
            self.cp_infos[*name_and_type_index as usize - 1]
                .to_name_and_type()
                .map(|nt| self.fmt_name_and_type_index(nt))
                .unwrap_or_else(|| String::from("???"))
        )
    }
}
impl Index<u16> for ConstantPool {
    type Output = CpInfo;

    fn index(&self, index: u16) -> &Self::Output {
        &self.cp_infos[index as usize - 1]
    }
}
impl<'a> IntoIterator for &'a ConstantPool {
    type Item = &'a CpInfo;
    type IntoIter = std::slice::Iter<'a, CpInfo>;

    fn into_iter(self) -> Self::IntoIter {
        self.cp_infos.iter()
    }
}
impl fmt::Debug for ConstantPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[")?;
        for (index, i) in self.cp_infos.iter().enumerate() {
            write!(f, "    {}. ", index + 1)?;
            match i {
                CpInfo::Class(ClassInfo { name_index }) => writeln!(
                    f,
                    "Class {{ name_index: {} ({:?}) }}",
                    name_index,
                    self.cp_infos[*name_index as usize - 1]
                        .to_utf8()
                        .unwrap_or("???")
                )?,
                CpInfo::FieldRef(ref_info) => {
                    writeln!(f, "FieldRef {}", self.fmt_ref_info(ref_info))?
                }
                CpInfo::MethodRef(ref_info) => {
                    writeln!(f, "MethodRef {}", self.fmt_ref_info(ref_info))?
                }
                CpInfo::NameAndType(name_and_type) => writeln!(
                    f,
                    "NameAndType {}",
                    self.fmt_name_and_type_index(name_and_type),
                )?,
                CpInfo::Utf8(s) => writeln!(f, "Utf8: {:?}", s)?,
                CpInfo::Long(l) => writeln!(f, "Long: {}", l)?,
                CpInfo::String { string_index } => writeln!(
                    f,
                    "String: {{ string_index: {} ({:?}) }}",
                    string_index,
                    self.cp_infos[*string_index as usize - 1]
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
pub enum CpInfo {
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
impl CpInfo {
    pub fn to_utf8(&self) -> Option<&str> {
        match self {
            Self::Utf8(s) => Some(s),
            _ => None,
        }
    }

    pub fn to_field_ref(&self) -> &RefInfo {
        match self {
            Self::FieldRef(r) => r,
            _ => panic!("Expected FieldRef, got {:?}", self),
        }
    }

    pub fn to_class_info(&self) -> Option<&ClassInfo> {
        match self {
            Self::Class(i) => Some(i),
            _ => None,
        }
    }

    pub fn to_name_and_type(&self) -> Option<&NameAndTypeInfo> {
        match self {
            Self::NameAndType(n) => Some(n),
            _ => None,
        }
    }

    pub fn to_method_ref(&self) -> &RefInfo {
        match self {
            Self::MethodRef(m) => m,
            _ => panic!("Expected MethodRef, got {:?}", self),
        }
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
