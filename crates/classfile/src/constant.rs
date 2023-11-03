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

#[derive(Default, Debug)]
pub struct ConstantPool(Vec<Constant>);
impl ConstantPool {
    pub fn new(constants: Vec<Constant>) -> Self {
        Self(constants)
    }

    pub fn get(&self, index: u16) -> Result<ConstantRef> {
        self.0
            .get(index as usize)
            .map(|constant| ConstantRef {
                pool: self,
                constant,
            })
            .ok_or(Error::ConstantNotFound(index))
    }

    pub fn contains(&self, constant: &Constant) -> bool {
        self.0.contains(constant)
    }
}

pub struct ConstantRef<'a> {
    pool: &'a ConstantPool,
    constant: &'a Constant,
}
impl<'a> ConstantRef<'a> {
    pub fn utf8(&self) -> Result<&'a str> {
        match self.constant {
            Constant::Utf8(s) => Ok(s),
            c => Err(Error::unexpected_constant("Utf8", c)),
        }
    }

    pub fn class_name(&self) -> Result<&'a str> {
        match self.constant {
            Constant::Class(ClassInfo { name_index }) => self.pool.get(*name_index)?.utf8(),
            c @ _ => Err(Error::unexpected_constant("ClassInfo", c)),
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
