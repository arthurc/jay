use std::{
    io::{Cursor, Read},
    marker::PhantomData,
};

use byteorder::{ByteOrder, ReadBytesExt};

use super::*;

pub struct Parser<'a, E: ByteOrder> {
    r: Cursor<&'a [u8]>,
    phantom: PhantomData<E>,
}

impl<'a, E: ByteOrder> Parser<'a, E> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self {
            r: Cursor::new(buf),
            phantom: PhantomData,
        }
    }

    pub(crate) fn parse_archive(mut self) -> Result<Archive<'a>, JImageError> {
        let header = self.parse_header()?;
        let index = self.parse_index(&header)?;
        let resource_data_start = self.r.position() as usize;

        Ok(Archive {
            buf: self.r.into_inner(),
            header,
            index,
            resource_data_start,
        })
    }

    fn parse_header(&mut self) -> Result<Header, JImageError> {
        let _ = self.parse_magic_identifier()?;
        let version = self.parse_version()?;
        let flags = self.read_u32()?;
        let resource_count = self.read_u32()?;
        let table_length = self.read_u32()?;
        let attributes_size = self.read_u32()?;
        let strings_size = self.read_u32()?;

        Ok(Header {
            version,
            flags,
            resource_count,
            table_length,
            attributes_size,
            strings_size,
        })
    }

    fn parse_index(&mut self, header: &Header) -> Result<Index, JImageError> {
        let mut redirect_table = vec![0i32; header.table_length as usize];
        self.r.read_i32_into::<E>(&mut redirect_table)?;

        let mut attribute_offsets = vec![0u32; header.table_length as usize];
        self.r.read_u32_into::<E>(&mut attribute_offsets)?;

        let mut attribute_data = vec![0u8; header.attributes_size as usize];
        self.r.read(&mut attribute_data)?;

        let mut strings_data = vec![0u8; header.strings_size as usize];
        self.r.read(&mut strings_data)?;

        Ok(Index {
            redirect_table,
            attribute_offsets,
            attribute_data,
            strings_data,
        })
    }

    pub(crate) fn parse_attributes(
        &mut self,
    ) -> Result<[u64; AttributeKind::Total as usize], JImageError> {
        let mut attributes = [0; AttributeKind::Total as usize];
        while let Some((kind, value)) = self.parse_attribute()? {
            attributes[kind as usize] = value;
        }

        Ok(attributes)
    }

    fn parse_attribute(&mut self) -> Result<Option<(AttributeKind, u64)>, JImageError> {
        let header_byte = self.read_u8()?;
        let kind = header_byte >> 3;
        let length = header_byte as usize & 0x7;

        if kind == 0 {
            return Ok(None);
        }

        let kind = AttributeKind::try_from(kind).map_err(|e| {
            JImageError::ReadError(
                format!("Invalid attribute kind: {:?}", e),
                self.r.position() as usize - 1,
            )
        })?;

        let value = (0..=length)
            .map(|_| self.read_u8())
            .try_fold(0u64, |acc, b| b.map(|b| acc << 8 | b as u64))?;

        Ok(Some((kind, value)))
    }

    fn parse_magic_identifier(&mut self) -> Result<(), JImageError> {
        match self.read_u32()? {
            0xCAFEDADA => Ok(()),
            magic_identifier => Err(JImageError::ReadError(
                format!("Invalid magic identifier: {magic_identifier:X}",),
                self.r.position() as usize,
            )),
        }
    }

    fn parse_version(&mut self) -> Result<(u16, u16), JImageError> {
        let minor = self.read_u16()?;
        let major = self.read_u16()?;
        Ok((major, minor))
    }

    fn read_u32(&mut self) -> Result<u32, JImageError> {
        Ok(self.r.read_u32::<E>()?)
    }

    fn read_u16(&mut self) -> Result<u16, JImageError> {
        Ok(self.r.read_u16::<E>()?)
    }

    fn read_u8(&mut self) -> Result<u8, JImageError> {
        Ok(self.r.read_u8()?)
    }
}

#[cfg(test)]
mod parse_magic_identifier_tests {
    use byteorder::LittleEndian;

    use super::*;

    #[test]
    fn it_should_be_able_to_parse_the_correct_identifier() {
        assert!(Parser::<LittleEndian>::new(&[0xda, 0xda, 0xfe, 0xca])
            .parse_magic_identifier()
            .is_ok());
    }

    #[test]
    fn it_should_fail_if_there_is_not_enough_data() {
        assert!(Parser::<LittleEndian>::new(&[0xca, 0xfe, 0xda])
            .parse_magic_identifier()
            .is_err());
    }

    #[test]
    fn it_should_fail_if_the_magic_identifier_is_incorrect() {
        assert!(Parser::<LittleEndian>::new(&[0xda, 0xda, 0xfe, 0xcb])
            .parse_magic_identifier()
            .is_err());
    }
}

#[cfg(test)]
mod parse_version_tests {
    use byteorder::LittleEndian;

    use super::*;

    #[test]
    fn it_should_be_able_to_parse_a_version() {
        assert_eq!(
            Parser::<LittleEndian>::new(&[0x34, 0x12, 0x78, 0x56])
                .parse_version()
                .unwrap(),
            (0x5678, 0x1234)
        );
    }
}

#[cfg(test)]
mod parse_attribute_tests {
    use byteorder::LittleEndian;

    use super::*;

    #[test]
    fn it_should_be_able_to_parse_an_attribute() {
        assert_eq!(
            Parser::<LittleEndian>::new(&[0x22, 0x03, 0x35, 0x62])
                .parse_attribute()
                .unwrap(),
            Some((AttributeKind::Extension, 0x33562))
        );
    }

    #[test]
    fn it_should_fail_if_there_are_not_enough_bytes_read() {
        assert!(Parser::<LittleEndian>::new(&[0x22, 0x03, 0x35])
            .parse_attribute()
            .is_err());
    }
}
