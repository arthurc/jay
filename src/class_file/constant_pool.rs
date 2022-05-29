use std::ops::Index;

#[derive(Debug, Default)]
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
    pub fn to_str(&self) -> &str {
        match self {
            Self::Utf8(s) => s,
            _ => panic!("Expected Utf8, got {:?}", self),
        }
    }

    pub fn to_field_ref(&self) -> &RefInfo {
        match self {
            Self::FieldRef(r) => r,
            _ => panic!("Expected FieldRef, got {:?}", self),
        }
    }

    pub fn to_class_info(&self) -> &ClassInfo {
        match self {
            Self::Class(i) => i,
            _ => panic!("Expected Class, got {:?}", self),
        }
    }

    pub fn to_name_and_type(&self) -> &NameAndTypeInfo {
        match self {
            Self::NameAndType(n) => n,
            _ => panic!("Expected NameAndType, got {:?}", self),
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
