use std::io::{BufReader, Read, Seek};

use byteorder::{BigEndian, ReadBytesExt};

use super::{constant_pool::CpInfo, *};

type Result<T, E = ClassFileError> = std::result::Result<T, E>;
type Endian = BigEndian;

#[derive(Debug)]
pub enum ParseEvent {
    ConstantPool(ConstantPool),
    ClassInfo(ClassInfo),
    Field(FieldInfo),
    Method(MethodInfo),
    Attributes(Vec<Attribute>),
}

pub(crate) struct Parser<R, F> {
    r: BufReader<R>,
    f: F,
}
impl<'a, R: Read + Seek, F: FnMut(ParseEvent)> Parser<R, F> {
    pub fn new(r: R, f: F) -> Self {
        Self {
            r: BufReader::new(r),
            f,
        }
    }

    pub fn parse(mut self) -> Result<()> {
        let _ = self.parse_magic_identifier()?;
        let _version = self.parse_version()?;

        let constnat_pool = self.parse_constant_pool()?;
        (self.f)(ParseEvent::ConstantPool(constnat_pool));

        let access_flags = self.read_u16()?;
        let this_class = self.read_u16()?;
        let super_class = self.read_u16()?;
        let interfaces_count = self.read_u16()?;
        let mut interfaces = vec![0u16; interfaces_count as usize];
        self.r.read_u16_into::<Endian>(&mut interfaces)?;

        (self.f)(ParseEvent::ClassInfo(ClassInfo {
            access_flags: AccessFlags::from_bits_truncate(access_flags),
            this_class,
            super_class,
            interfaces,
        }));

        let fields_count = self.read_u16()?;
        for _ in 0..fields_count {
            self.parse_field_info()?;
        }

        let methods_count = self.read_u16()?;
        for _ in 0..methods_count {
            self.parse_method_info()?;
        }

        let attributes_count = self.read_u16()?;
        let attributes = self.parse_attributes(attributes_count)?;
        (self.f)(ParseEvent::Attributes(attributes));

        Ok(())
    }

    fn parse_field_info(&mut self) -> Result<()> {
        let access_flags = self.read_u16()?;
        let name_index = self.read_u16()?;
        let descriptor_index = self.read_u16()?;
        let attributes_count = self.read_u16()?;
        let attributes = self.parse_attributes(attributes_count)?;

        (self.f)(ParseEvent::Field(FieldInfo {
            access_flags: AccessFlags::from_bits_truncate(access_flags),
            name_index,
            descriptor_index,
            attributes,
        }));

        Ok(())
    }

    fn parse_method_info(&mut self) -> Result<()> {
        let access_flags = self.read_u16()?;
        let name_index = self.read_u16()?;
        let descriptor_index = self.read_u16()?;
        let attributes_count = self.read_u16()?;
        let attributes = self.parse_attributes(attributes_count)?;

        (self.f)(ParseEvent::Method(MethodInfo {
            access_flags: AccessFlags::from_bits_truncate(access_flags),
            name_index,
            descriptor_index,
            attributes,
        }));

        Ok(())
    }

    fn parse_magic_identifier(&mut self) -> Result<()> {
        match self.read_u32()? {
            0xCAFEBABE => Ok(()),
            magic_identifier => Err(ClassFileError::ReadError(
                format!("Invalid magic identifier: 0x{magic_identifier:X}"),
                self.r.stream_position()? as usize,
            )),
        }
    }

    fn parse_version(&mut self) -> Result<(u16, u16)> {
        let minor = self.read_u16()?;
        let major = self.read_u16()?;
        Ok((major, minor))
    }

    fn parse_constant_pool(&mut self) -> Result<ConstantPool> {
        let constant_pool_count = self.read_u16()?;

        let mut count = constant_pool_count as usize - 1;
        let mut res = Vec::with_capacity(count);
        while count > 0 {
            let (cp_info, slot_size) = self.parse_cp_info()?;
            res.push(cp_info);
            (0..slot_size - 1).for_each(|_| res.push(CpInfo::Unusable));

            count -= slot_size;
        }
        Ok(ConstantPool::new(res))
    }

    fn parse_cp_info(&mut self) -> Result<(CpInfo, usize)> {
        let tag = self.read_u8()?;
        let (cp_info, additional_cp_info) = match tag {
            1 => (self.parse_utf8()?, 1),
            3 => (self.parse_integer()?, 1),
            5 => (self.parse_long()?, 2),
            7 => (self.parse_class_info()?, 1),
            8 => (self.parse_string()?, 1),
            9 => (self.parse_field_ref()?, 1),
            10 => (self.parse_method_ref()?, 1),
            11 => (self.parse_interface_method_ref()?, 1),
            12 => (self.parse_name_and_type_info()?, 1),
            15 => (self.parse_method_handle()?, 1),
            16 => (self.parse_method_type_info()?, 1),
            18 => (self.parse_invoke_dynamic_info()?, 1),
            _ => {
                return Err(ClassFileError::ReadError(
                    format!("Invalid cp info tag: {tag}"),
                    self.r.stream_position()? as usize - 1,
                ))
            }
        };

        Ok((cp_info, additional_cp_info))
    }

    fn parse_utf8(&mut self) -> Result<CpInfo> {
        let length = self.read_u16()?;
        let mut bytes = vec![0u8; length as usize];
        self.r.read_exact(&mut bytes)?;

        Ok(CpInfo::Utf8(String::from_utf8_lossy(&bytes).into()))
    }

    fn parse_integer(&mut self) -> Result<CpInfo> {
        let int = self.read_i32()?;

        Ok(CpInfo::Integer(int))
    }

    fn parse_long(&mut self) -> Result<CpInfo> {
        let high_bytes = self.read_u32()?;
        let low_bytes = self.read_u32()?;

        Ok(CpInfo::Long(((high_bytes as i64) << 32) + low_bytes as i64))
    }

    fn parse_class_info(&mut self) -> Result<CpInfo> {
        let name_index = self.read_u16()?;

        Ok(CpInfo::Class(constant_pool::ClassInfo { name_index }))
    }

    fn parse_string(&mut self) -> Result<CpInfo> {
        let string_index = self.read_u16()?;

        Ok(CpInfo::String { string_index })
    }

    fn parse_field_ref(&mut self) -> Result<CpInfo> {
        let ref_info = self.parse_ref_info()?;

        Ok(CpInfo::FieldRef(ref_info))
    }

    fn parse_method_ref(&mut self) -> Result<CpInfo> {
        let ref_info = self.parse_ref_info()?;

        Ok(CpInfo::MethodRef(ref_info))
    }

    fn parse_interface_method_ref(&mut self) -> Result<CpInfo> {
        let ref_info = self.parse_ref_info()?;

        Ok(CpInfo::InterfaceMethodRef(ref_info))
    }

    fn parse_name_and_type_info(&mut self) -> Result<CpInfo> {
        let name_index = self.read_u16()?;
        let descriptor_index = self.read_u16()?;

        Ok(CpInfo::NameAndType(constant_pool::NameAndTypeInfo {
            name_index,
            descriptor_index,
        }))
    }

    fn parse_method_handle(&mut self) -> Result<CpInfo> {
        let reference_kind = self.read_u8()?;
        let reference_index = self.read_u16()?;

        Ok(CpInfo::MethodHandle(constant_pool::MethodHandleInfo {
            reference_kind,
            reference_index,
        }))
    }

    fn parse_method_type_info(&mut self) -> Result<CpInfo> {
        let descriptor_index = self.read_u16()?;

        Ok(CpInfo::MethodType(constant_pool::MethodTypeInfo {
            descriptor_index,
        }))
    }

    fn parse_invoke_dynamic_info(&mut self) -> Result<CpInfo> {
        let bootstrap_method_attr_index = self.read_u16()?;
        let name_and_type_index = self.read_u16()?;

        Ok(CpInfo::InvokeDynamic(constant_pool::InvokeDynamicInfo {
            bootstrap_method_attr_index,
            name_and_type_index,
        }))
    }

    fn parse_ref_info(&mut self) -> Result<constant_pool::RefInfo> {
        let class_index = self.read_u16()?;
        let name_and_type_index = self.read_u16()?;

        Ok(constant_pool::RefInfo {
            class_index,
            name_and_type_index,
        })
    }

    pub fn parse_attribute(&mut self) -> Result<Attribute> {
        let attribute_name_index = self.read_u16()?;
        let attribute_length = self.read_u32()?;
        let mut info = vec![0u8; attribute_length as usize];
        self.r.read_exact(&mut info)?;

        Ok(Attribute {
            attribute_name_index,
            info,
        })
    }

    pub fn parse_code_attribute(&mut self) -> Result<CodeAttribute> {
        let max_stack = self.read_u16()?;
        let max_locals = self.read_u16()?;
        let code_length = self.read_u32()?;
        let mut code = vec![0u8; code_length as usize];
        self.r.read_exact(&mut code)?;
        let exception_table_length = self.read_u16()?;
        let exception_table = (0..exception_table_length)
            .into_iter()
            .map(|_| self.parse_exception_table_entry())
            .collect::<Result<Vec<_>>>()?;
        let attributes_count = self.read_u16()?;
        let attributes = self.parse_attributes(attributes_count)?;

        Ok(CodeAttribute {
            max_stack,
            max_locals,
            code,
            exception_table,
            attributes,
        })
    }

    fn parse_exception_table_entry(&mut self) -> Result<ExceptionTableEntry> {
        let start_pc = self.read_u16()?;
        let end_pc = self.read_u16()?;
        let handler_pc = self.read_u16()?;
        let catch_type = self.read_u16()?;

        Ok(ExceptionTableEntry {
            start_pc,
            end_pc,
            handler_pc,
            catch_type,
        })
    }

    fn parse_attributes(&mut self, attributes_count: u16) -> Result<Vec<Attribute>> {
        (0..attributes_count)
            .into_iter()
            .map(|_| self.parse_attribute())
            .collect::<Result<Vec<_>>>()
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(self.r.read_u32::<Endian>()?)
    }

    fn read_u16(&mut self) -> Result<u16> {
        Ok(self.r.read_u16::<Endian>()?)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.r.read_u8()?)
    }

    fn read_i32(&mut self) -> Result<i32> {
        Ok(self.r.read_i32::<Endian>()?)
    }
}
