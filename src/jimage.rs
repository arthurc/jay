use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use flate2::read::ZlibDecoder;

use crate::{JayError, JayResult};

const IMAGE_MAGIC: u32 = 0xCAFEDADA;
const MAJOR_VERSION: u32 = 1;
const MINOR_VERSION: u32 = 0;
const HEADER_SIZE: usize = 28;
const HASH_MULTIPLIER: u32 = 0x0100_0193;
const ATTRIBUTE_COUNT: usize = 8;
const ATTRIBUTE_MODULE: usize = 1;
const ATTRIBUTE_PARENT: usize = 2;
const ATTRIBUTE_BASE: usize = 3;
const ATTRIBUTE_EXTENSION: usize = 4;
const ATTRIBUTE_OFFSET: usize = 5;
const ATTRIBUTE_COMPRESSED: usize = 6;
const ATTRIBUTE_UNCOMPRESSED: usize = 7;
const RESOURCE_HEADER_MAGIC: u32 = 0xCAFEFAFA;
const RESOURCE_HEADER_SIZE: usize = 29;
const DECOMPRESSOR_ZIP: &str = "zip";
const DECOMPRESSOR_COMPACT_CP: &str = "compact-cp";
const CP_EXTERNALIZED_STRING: u8 = 23;
const CP_EXTERNALIZED_DESCRIPTOR: u8 = 25;
const CP_UTF8: u8 = 1;
const CP_LONG: u8 = 5;
const CP_DOUBLE: u8 = 6;
const CP_ENTRY_SIZES: [usize; 19] = [0, 0, 0, 4, 4, 8, 8, 2, 2, 4, 4, 4, 4, 0, 0, 3, 2, 0, 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    fn read_u32(self, bytes: &[u8]) -> u32 {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        }
    }

    fn read_i32(self, bytes: &[u8]) -> i32 {
        self.read_u32(bytes) as i32
    }

    fn read_u64(self, bytes: &[u8]) -> u64 {
        let bytes = [
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ];
        match self {
            Self::Little => u64::from_le_bytes(bytes),
            Self::Big => u64::from_be_bytes(bytes),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JImageHeader {
    pub major_version: u32,
    pub minor_version: u32,
    pub flags: u32,
    pub resource_count: u32,
    pub table_length: u32,
    pub locations_size: u32,
    pub strings_size: u32,
}

#[derive(Debug, Clone)]
pub struct JImage {
    header: JImageHeader,
    bytes: Vec<u8>,
    endian: Endian,
    index_size: usize,
    class_index: HashMap<String, Vec<ResourceLocation>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLocation {
    location_offset: u32,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
}

impl JImage {
    pub fn open(path: impl AsRef<Path>) -> JayResult<Self> {
        let bytes = std::fs::read(path.as_ref()).map_err(|error| {
            JayError::new(format!(
                "could not read JImage at {}: {error}",
                path.as_ref().display()
            ))
        })?;
        Self::parse(bytes)
    }

    fn parse(bytes: Vec<u8>) -> JayResult<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err(JayError::new("truncated JImage header"));
        }

        let magic_bytes = &bytes[0..4];
        let endian = if u32::from_le_bytes(magic_bytes.try_into().unwrap()) == IMAGE_MAGIC {
            Endian::Little
        } else if u32::from_be_bytes(magic_bytes.try_into().unwrap()) == IMAGE_MAGIC {
            Endian::Big
        } else {
            return Err(JayError::new("invalid JImage magic"));
        };

        let version = endian.read_u32(&bytes[4..8]);
        let major_version = version >> 16;
        let minor_version = version & 0xffff;
        if major_version != MAJOR_VERSION || minor_version != MINOR_VERSION {
            return Err(JayError::new(format!(
                "unsupported JImage version {major_version}.{minor_version}"
            )));
        }

        let header = JImageHeader {
            major_version,
            minor_version,
            flags: endian.read_u32(&bytes[8..12]),
            resource_count: endian.read_u32(&bytes[12..16]),
            table_length: endian.read_u32(&bytes[16..20]),
            locations_size: endian.read_u32(&bytes[20..24]),
            strings_size: endian.read_u32(&bytes[24..28]),
        };

        let table_bytes = (header.table_length as usize)
            .checked_mul(4)
            .and_then(|size| size.checked_mul(2))
            .ok_or_else(|| JayError::new("JImage index size overflow"))?;
        let index_size = HEADER_SIZE
            .checked_add(table_bytes)
            .and_then(|size| size.checked_add(header.locations_size as usize))
            .and_then(|size| size.checked_add(header.strings_size as usize))
            .ok_or_else(|| JayError::new("JImage index size overflow"))?;
        if bytes.len() < index_size {
            return Err(JayError::new("truncated JImage index"));
        }

        let mut image = Self {
            header,
            bytes,
            endian,
            index_size,
            class_index: HashMap::new(),
        };
        image.class_index = image.build_class_index()?;
        Ok(image)
    }

    pub fn header(&self) -> &JImageHeader {
        &self.header
    }

    pub fn find_resource(
        &self,
        module_name: &str,
        resource_name: &str,
    ) -> JayResult<Option<ResourceLocation>> {
        if module_name.is_empty() || resource_name.is_empty() {
            return Ok(None);
        }
        let path = format!("/{module_name}/{resource_name}");
        self.find_resource_by_full_path(&path)
    }

    pub fn get_resource(&self, location: &ResourceLocation) -> JayResult<Vec<u8>> {
        let attributes = self.location_at_offset(location.location_offset)?;
        let offset = attributes.attributes[ATTRIBUTE_OFFSET] as usize;
        let uncompressed_size = attributes.attributes[ATTRIBUTE_UNCOMPRESSED] as usize;
        let compressed_size = attributes.attributes[ATTRIBUTE_COMPRESSED] as usize;

        if compressed_size != 0 {
            let start = self
                .index_size
                .checked_add(offset)
                .ok_or_else(|| JayError::new("JImage resource offset overflow"))?;
            let bytes = self.slice(start, compressed_size, "compressed JImage resource")?;
            return decompress_resource_data(
                bytes,
                uncompressed_size as u64,
                &self.image_strings(),
                self.endian,
            );
        }

        let start = self
            .index_size
            .checked_add(offset)
            .ok_or_else(|| JayError::new("JImage resource offset overflow"))?;
        let bytes = self.slice(start, uncompressed_size, "JImage resource")?;
        Ok(bytes.to_vec())
    }

    pub fn load_class_bytes(&self, class_name: &str) -> JayResult<Option<Vec<u8>>> {
        let _ = class_name_to_resource_name(class_name)?;
        let Some(matches) = self.class_index.get(class_name) else {
            return Ok(None);
        };

        match matches.len() {
            0 => Ok(None),
            1 => self.get_resource(&matches[0]).map(Some),
            _ => Err(JayError::new(format!(
                "ambiguous class {class_name} found in multiple JImage modules"
            ))),
        }
    }
}

impl JImage {
    fn find_resource_by_full_path(&self, path: &str) -> JayResult<Option<ResourceLocation>> {
        let Some(index) = self.find_index(path)? else {
            return Ok(None);
        };
        let Some(location) = self.location_at_index(index)? else {
            return Ok(None);
        };
        if !self.verify_location(&location, path)? {
            return Ok(None);
        }

        Ok(Some(ResourceLocation {
            location_offset: self.location_offset(index)?,
            uncompressed_size: location.attributes[ATTRIBUTE_UNCOMPRESSED],
            compressed_size: location.attributes[ATTRIBUTE_COMPRESSED],
        }))
    }

    fn find_index(&self, path: &str) -> JayResult<Option<u32>> {
        let table_length = self.header.table_length;
        if table_length == 0 {
            return Ok(None);
        }

        let hash = hash_code(path, HASH_MULTIPLIER);
        let redirect_index = hash % table_length;
        let value = self.redirect_value(redirect_index)?;
        if value > 0 {
            Ok(Some(hash_code(path, value as u32) % table_length))
        } else if value < 0 {
            Ok(Some((-1 - value) as u32))
        } else {
            Ok(None)
        }
    }

    fn verify_location(&self, location: &LocationAttributes, path: &str) -> JayResult<bool> {
        let mut remaining = path;

        let module = self.string_at(location.attributes[ATTRIBUTE_MODULE] as u32)?;
        if !module.is_empty() {
            let Some(next) = remaining.strip_prefix('/') else {
                return Ok(false);
            };
            let Some(next) = next.strip_prefix(module) else {
                return Ok(false);
            };
            let Some(next) = next.strip_prefix('/') else {
                return Ok(false);
            };
            remaining = next;
        }

        let parent = self.string_at(location.attributes[ATTRIBUTE_PARENT] as u32)?;
        if !parent.is_empty() {
            let Some(next) = remaining.strip_prefix(parent) else {
                return Ok(false);
            };
            let Some(next) = next.strip_prefix('/') else {
                return Ok(false);
            };
            remaining = next;
        }

        let base = self.string_at(location.attributes[ATTRIBUTE_BASE] as u32)?;
        let Some(next) = remaining.strip_prefix(base) else {
            return Ok(false);
        };
        remaining = next;

        let extension = self.string_at(location.attributes[ATTRIBUTE_EXTENSION] as u32)?;
        if !extension.is_empty() {
            let Some(next) = remaining.strip_prefix('.') else {
                return Ok(false);
            };
            let Some(next) = next.strip_prefix(extension) else {
                return Ok(false);
            };
            remaining = next;
        }

        Ok(remaining.is_empty())
    }

    fn redirect_value(&self, index: u32) -> JayResult<i32> {
        let start = HEADER_SIZE + index as usize * 4;
        let bytes = self.slice(start, 4, "JImage redirect table")?;
        Ok(self.endian.read_i32(bytes))
    }

    fn location_offset(&self, index: u32) -> JayResult<u32> {
        let offsets_start = HEADER_SIZE + self.header.table_length as usize * 4;
        let start = offsets_start + index as usize * 4;
        let bytes = self.slice(start, 4, "JImage location offset table")?;
        Ok(self.endian.read_u32(bytes))
    }

    fn location_at_index(&self, index: u32) -> JayResult<Option<LocationAttributes>> {
        let offset = self.location_offset(index)?;
        if offset == 0 {
            return Ok(None);
        }
        self.location_at_offset(offset).map(Some)
    }

    fn location_at_offset(&self, offset: u32) -> JayResult<LocationAttributes> {
        if offset as usize >= self.header.locations_size as usize {
            return Err(JayError::new("JImage location offset out of bounds"));
        }
        let location_start = HEADER_SIZE + self.header.table_length as usize * 8;
        let mut cursor = location_start + offset as usize;
        let location_end = location_start + self.header.locations_size as usize;
        let mut attributes = [0u64; ATTRIBUTE_COUNT];

        loop {
            if cursor >= location_end {
                return Err(JayError::new("unterminated JImage location attributes"));
            }
            let header = self.bytes[cursor];
            cursor += 1;
            if header == 0 {
                return Ok(LocationAttributes { attributes });
            }

            let kind = (header >> 3) as usize;
            if kind == 0 || kind >= ATTRIBUTE_COUNT {
                return Err(JayError::new("invalid JImage location attribute kind"));
            }
            let length = (header & 0x07) as usize + 1;
            if cursor + length > location_end {
                return Err(JayError::new("truncated JImage location attribute"));
            }

            let mut value = 0u64;
            for byte in &self.bytes[cursor..cursor + length] {
                value = (value << 8) | u64::from(*byte);
            }
            attributes[kind] = value;
            cursor += length;
        }
    }

    fn string_at(&self, offset: u32) -> JayResult<&str> {
        self.image_strings().get(offset)
    }

    fn slice(&self, start: usize, length: usize, label: &str) -> JayResult<&[u8]> {
        let end = start
            .checked_add(length)
            .ok_or_else(|| JayError::new(format!("{label} offset overflow")))?;
        if end > self.bytes.len() {
            return Err(JayError::new(format!("{label} out of bounds")));
        }
        Ok(&self.bytes[start..end])
    }

    fn entry_name(&self, location: &LocationAttributes) -> JayResult<JImageEntryName> {
        let module = self
            .string_at(location.attributes[ATTRIBUTE_MODULE] as u32)?
            .to_string();
        let parent = self.string_at(location.attributes[ATTRIBUTE_PARENT] as u32)?;
        let base = self.string_at(location.attributes[ATTRIBUTE_BASE] as u32)?;
        let extension = self.string_at(location.attributes[ATTRIBUTE_EXTENSION] as u32)?;

        let mut resource_name = String::new();
        if !parent.is_empty() {
            resource_name.push_str(parent);
            resource_name.push('/');
        }
        resource_name.push_str(base);
        if !extension.is_empty() {
            resource_name.push('.');
            resource_name.push_str(extension);
        }

        Ok(JImageEntryName {
            module,
            resource_name,
        })
    }

    fn build_class_index(&self) -> JayResult<HashMap<String, Vec<ResourceLocation>>> {
        let mut index = HashMap::<String, Vec<ResourceLocation>>::new();
        for table_index in 0..self.header.table_length {
            let Some(location) = self.location_at_index(table_index)? else {
                continue;
            };
            let entry = self.entry_name(&location)?;
            if entry.module == "modules" || entry.module == "packages" {
                continue;
            }
            let Some(class_name) = entry.resource_name.strip_suffix(".class") else {
                continue;
            };
            index
                .entry(class_name.replace('/', "."))
                .or_default()
                .push(ResourceLocation {
                    location_offset: self.location_offset(table_index)?,
                    uncompressed_size: location.attributes[ATTRIBUTE_UNCOMPRESSED],
                    compressed_size: location.attributes[ATTRIBUTE_COMPRESSED],
                });
        }
        Ok(index)
    }

    fn image_strings(&self) -> ImageStrings<'_> {
        let strings_start = HEADER_SIZE
            + self.header.table_length as usize * 8
            + self.header.locations_size as usize;
        let strings_end = strings_start + self.header.strings_size as usize;
        ImageStrings::new(&self.bytes[strings_start..strings_end])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocationAttributes {
    attributes: [u64; ATTRIBUTE_COUNT],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JImageEntryName {
    module: String,
    resource_name: String,
}

#[derive(Debug, Clone, Copy)]
struct ImageStrings<'a> {
    bytes: &'a [u8],
}

impl<'a> ImageStrings<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn get(&self, offset: u32) -> JayResult<&'a str> {
        if offset as usize >= self.bytes.len() {
            return Err(JayError::new("JImage string offset out of bounds"));
        }
        let start = offset as usize;
        let relative_end = self.bytes[start..]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| JayError::new("unterminated JImage string"))?;
        std::str::from_utf8(&self.bytes[start..start + relative_end])
            .map_err(|_| JayError::new("invalid UTF-8 in JImage string table"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceHeader {
    size: u64,
    uncompressed_size: u64,
    decompressor_name_offset: u32,
}

fn decompress_resource_data(
    data: &[u8],
    final_uncompressed_size: u64,
    strings: &ImageStrings<'_>,
    endian: Endian,
) -> JayResult<Vec<u8>> {
    let mut current = data.to_vec();
    loop {
        let Some(header) = parse_resource_header(&current, endian)? else {
            let expected = final_uncompressed_size as usize;
            if current.len() < expected {
                return Err(JayError::new("decompressed JImage resource is truncated"));
            }
            current.truncate(expected);
            return Ok(current);
        };

        let payload_start = RESOURCE_HEADER_SIZE;
        let payload_end = payload_start
            .checked_add(header.size as usize)
            .ok_or_else(|| JayError::new("compressed JImage resource size overflow"))?;
        if payload_end > current.len() {
            return Err(JayError::new("truncated compressed JImage resource"));
        }
        let payload = &current[payload_start..payload_end];
        let decompressor = strings.get(header.decompressor_name_offset)?;
        current = match decompressor {
            DECOMPRESSOR_ZIP => decompress_zip(payload, header.uncompressed_size)?,
            DECOMPRESSOR_COMPACT_CP => decompress_compact_cp(payload, &header, strings)?,
            other => {
                return Err(JayError::new(format!(
                    "unsupported JImage decompressor {other}"
                )));
            }
        };
    }
}

fn parse_resource_header(data: &[u8], endian: Endian) -> JayResult<Option<ResourceHeader>> {
    if data.len() < RESOURCE_HEADER_SIZE {
        return Ok(None);
    }
    let magic = endian.read_u32(&data[0..4]);
    if magic != RESOURCE_HEADER_MAGIC {
        return Ok(None);
    }
    Ok(Some(ResourceHeader {
        size: endian.read_u64(&data[4..12]),
        uncompressed_size: endian.read_u64(&data[12..20]),
        decompressor_name_offset: endian.read_u32(&data[20..24]),
    }))
}

fn decompress_zip(data: &[u8], uncompressed_size: u64) -> JayResult<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|error| JayError::new(format!("could not inflate JImage resource: {error}")))?;
    if output.len() != uncompressed_size as usize {
        return Err(JayError::new(
            "inflated JImage resource has unexpected size",
        ));
    }
    Ok(output)
}

fn decompress_compact_cp(
    data: &[u8],
    header: &ResourceHeader,
    strings: &ImageStrings<'_>,
) -> JayResult<Vec<u8>> {
    let mut input = Cursor::new(data);
    let mut output = Vec::with_capacity(header.uncompressed_size as usize);
    let class_header = input.read_bytes(10, "compact-cp class header")?;
    output.extend(class_header);
    let cp_count = u16::from_be_bytes([class_header[8], class_header[9]]);

    let mut index = 1u16;
    while index < cp_count {
        let tag = input.read_u1("compact-cp tag")?;
        match tag {
            CP_EXTERNALIZED_STRING => {
                output.push(CP_UTF8);
                let string_offset = input.read_compressed_int()?;
                write_utf8(strings.get(string_offset)?, &mut output)?;
            }
            CP_EXTERNALIZED_DESCRIPTOR => {
                output.push(CP_UTF8);
                let descriptor_offset = input.read_compressed_int()?;
                let indexes_length = input.read_compressed_int()? as usize;
                let indexes = input.read_bytes(indexes_length, "compact-cp descriptor indexes")?;
                let descriptor = strings.get(descriptor_offset)?;
                let reconstructed = reconstruct_descriptor(descriptor, indexes, strings)?;
                write_utf8(&reconstructed, &mut output)?;
            }
            CP_UTF8 => {
                output.push(tag);
                let length_bytes = input.read_bytes(2, "compact-cp UTF-8 length")?;
                let length = u16::from_be_bytes([length_bytes[0], length_bytes[1]]) as usize;
                output.extend(length_bytes);
                output.extend(input.read_bytes(length, "compact-cp UTF-8 bytes")?);
            }
            CP_LONG | CP_DOUBLE => {
                output.push(tag);
                output.extend(input.read_bytes(8, "compact-cp wide constant")?);
                index += 1;
            }
            other => {
                let Some(size) = CP_ENTRY_SIZES.get(other as usize).copied() else {
                    return Err(JayError::new(format!(
                        "unsupported compact-cp constant pool tag {other}"
                    )));
                };
                output.push(other);
                output.extend(input.read_bytes(size, "compact-cp constant pool entry")?);
            }
        }
        index += 1;
    }

    output.extend(input.remaining());
    if output.len() != header.uncompressed_size as usize {
        return Err(JayError::new(
            "compact-cp reconstruction has unexpected size",
        ));
    }
    Ok(output)
}

fn reconstruct_descriptor(
    descriptor: &str,
    indexes: &[u8],
    strings: &ImageStrings<'_>,
) -> JayResult<String> {
    if indexes.is_empty() {
        return Ok(descriptor.to_string());
    }

    let mut input = Cursor::new(indexes);
    let mut output = String::new();
    for ch in descriptor.chars() {
        output.push(ch);
        if ch == 'L' {
            let package_offset = input.read_compressed_int()?;
            let package = strings.get(package_offset)?;
            if !package.is_empty() {
                output.push_str(package);
                output.push('/');
            }
            let class_offset = input.read_compressed_int()?;
            output.push_str(strings.get(class_offset)?);
        }
    }
    Ok(output)
}

fn write_utf8(value: &str, output: &mut Vec<u8>) -> JayResult<()> {
    let length = u16::try_from(value.len())
        .map_err(|_| JayError::new("compact-cp UTF-8 value is too long"))?;
    output.extend(length.to_be_bytes());
    output.extend(value.as_bytes());
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_u1(&mut self, label: &str) -> JayResult<u8> {
        Ok(self.read_bytes(1, label)?[0])
    }

    fn read_compressed_int(&mut self) -> JayResult<u32> {
        let first = self.read_u1("compact-cp integer")?;
        if first & 0x80 != 0 {
            let length = ((first & 0x60) >> 5) as usize;
            if length == 0 {
                return Err(JayError::new("invalid compact-cp integer length"));
            }
            let mut value = u32::from(first & 0x1f);
            if length > 1 {
                value <<= 8 * (length - 1);
                for position in 1..length {
                    let byte = self.read_u1("compact-cp integer byte")?;
                    value |= u32::from(byte) << (8 * (length - position - 1));
                }
            }
            Ok(value)
        } else {
            let bytes = self.read_bytes(3, "compact-cp integer")?;
            Ok((u32::from(first) << 24)
                | (u32::from(bytes[0]) << 16)
                | (u32::from(bytes[1]) << 8)
                | u32::from(bytes[2]))
        }
    }

    fn read_bytes(&mut self, length: usize, label: &str) -> JayResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| JayError::new(format!("{label} offset overflow")))?;
        if end > self.bytes.len() {
            return Err(JayError::new(format!("truncated {label}")));
        }
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.offset..]
    }
}

fn class_name_to_resource_name(class_name: &str) -> JayResult<String> {
    if class_name.is_empty()
        || class_name.starts_with('.')
        || class_name.ends_with('.')
        || class_name.contains("..")
        || class_name.contains('/')
        || class_name.contains('\\')
    {
        return Err(JayError::new(format!("invalid class name: {class_name}")));
    }
    Ok(format!("{}.class", class_name.replace('.', "/")))
}

fn hash_code(value: &str, seed: u32) -> u32 {
    value.bytes().fold(seed, |hash, byte| {
        hash.wrapping_mul(HASH_MULTIPLIER) ^ u32::from(byte)
    }) & 0x7fff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    const DEFAULT_JIMAGE: &str = "/Users/arthur/.sdkman/candidates/java/current/lib/modules";

    #[test]
    fn parses_default_jdk_jimage_header() {
        let image = JImage::open(DEFAULT_JIMAGE).unwrap();

        assert_eq!(image.header().major_version, 1);
        assert_eq!(image.header().minor_version, 0);
        assert_eq!(image.header().flags, 0);
        assert!(image.header().resource_count > 0);
        assert!(image.header().table_length > 0);
        assert!(image.header().locations_size > 0);
        assert!(image.header().strings_size > 0);
    }

    #[test]
    fn finds_resource_by_module_and_path() {
        let image = JImage::open(DEFAULT_JIMAGE).unwrap();

        let location = image
            .find_resource("java.base", "java/lang/Object.class")
            .unwrap()
            .unwrap();

        assert!(location.uncompressed_size > 0);
    }

    #[test]
    fn loads_uncompressed_resource_bytes() {
        let image = JImage::open(DEFAULT_JIMAGE).unwrap();
        let location = image
            .find_resource("java.base", "java/lang/Object.class")
            .unwrap()
            .unwrap();

        let bytes = image.get_resource(&location).unwrap();

        assert_eq!(&bytes[..4], &[0xCA, 0xFE, 0xBA, 0xBE]);
    }

    #[test]
    fn loads_class_by_binary_name_without_module() {
        let image = JImage::open(DEFAULT_JIMAGE).unwrap();

        let bytes = image.load_class_bytes("java.lang.Object").unwrap().unwrap();

        assert_eq!(&bytes[..4], &[0xCA, 0xFE, 0xBA, 0xBE]);
    }

    #[test]
    fn decompresses_zip_resource_header() {
        let strings = strings_table(["zip"]);
        let payload = b"hello jimage";
        let compressed = zlib(payload);
        let resource = resource_header(compressed.len(), payload.len(), 1, true, &compressed);

        let bytes = decompress_resource_data(
            &resource,
            payload.len() as u64,
            &ImageStrings::new(&strings),
            Endian::Little,
        )
        .unwrap();

        assert_eq!(bytes, payload);
    }

    #[test]
    fn decompresses_stacked_resource_headers() {
        let strings = strings_table(["zip"]);
        let payload = b"stacked jimage";
        let inner_compressed = zlib(payload);
        let inner = resource_header(
            inner_compressed.len(),
            payload.len(),
            1,
            true,
            &inner_compressed,
        );
        let outer_compressed = zlib(&inner);
        let outer = resource_header(
            outer_compressed.len(),
            inner.len(),
            1,
            false,
            &outer_compressed,
        );

        let bytes = decompress_resource_data(
            &outer,
            payload.len() as u64,
            &ImageStrings::new(&strings),
            Endian::Little,
        )
        .unwrap();

        assert_eq!(bytes, payload);
    }

    #[test]
    fn decompresses_compact_cp_descriptor_strings() {
        let strings = strings_table(["compact-cp", "(L;)V", "java/lang", "String"]);
        let descriptor_offset = string_offset(&strings, "(L;)V");
        let package_offset = string_offset(&strings, "java/lang");
        let class_offset = string_offset(&strings, "String");
        let mut compact_class = vec![
            0xCA, 0xFE, 0xBA, 0xBE, // magic
            0x00, 0x00, // minor
            0x00, 0x45, // major
            0x00, 0x02, // constant_pool_count
            25,   // externalized descriptor string
        ];
        compact_class.extend(compressed_int(descriptor_offset));
        let indexes = [compressed_int(package_offset), compressed_int(class_offset)].concat();
        compact_class.extend(compressed_int(indexes.len() as u32));
        compact_class.extend(indexes);
        compact_class.extend([
            0x00, 0x21, // access_flags
            0x00, 0x00, // this_class
            0x00, 0x00, // super_class
            0x00, 0x00, // interfaces_count
            0x00, 0x00, // fields_count
            0x00, 0x00, // methods_count
            0x00, 0x00, // attributes_count
        ]);
        let expected_descriptor = b"(Ljava/lang/String;)V";
        let expected_size = 10 + 1 + 2 + expected_descriptor.len() + 14;
        let resource = resource_header(compact_class.len(), expected_size, 1, true, &compact_class);

        let bytes = decompress_resource_data(
            &resource,
            expected_size as u64,
            &ImageStrings::new(&strings),
            Endian::Little,
        )
        .unwrap();

        assert!(
            bytes
                .windows(expected_descriptor.len())
                .any(|window| window == expected_descriptor)
        );
    }

    #[test]
    fn rejects_bad_jimage_magic() {
        let error = JImage::parse(vec![0; HEADER_SIZE]).unwrap_err();

        assert!(error.to_string().contains("invalid JImage magic"));
    }

    #[test]
    fn rejects_bad_jimage_version() {
        let mut bytes = synthetic_image(0, &[], b"\0", &[]);
        bytes[4..8].copy_from_slice(&0x0002_0000u32.to_le_bytes());

        let error = JImage::parse(bytes).unwrap_err();

        assert!(error.to_string().contains("unsupported JImage version"));
    }

    #[test]
    fn rejects_truncated_jimage_index() {
        let mut bytes = header(1, 0, 1);
        bytes.extend(0i32.to_le_bytes());

        let error = JImage::parse(bytes).unwrap_err();

        assert!(error.to_string().contains("truncated JImage index"));
    }

    #[test]
    fn rejects_out_of_bounds_jimage_string_offsets() {
        let image = JImage::parse(synthetic_image(0, &[], b"\0", &[])).unwrap();

        let error = image.string_at(1).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("JImage string offset out of bounds")
        );
    }

    #[test]
    fn rejects_invalid_location_streams() {
        let image = JImage::parse(synthetic_image(0, &[0x08], b"\0", &[])).unwrap();

        let error = image.location_at_offset(0).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("truncated JImage location attribute")
        );
    }

    #[test]
    fn rejects_resource_ranges_outside_file() {
        let location_stream = [
            location_attribute(ATTRIBUTE_OFFSET, 10),
            location_attribute(ATTRIBUTE_UNCOMPRESSED, 4),
            vec![0],
        ]
        .concat();
        let image = JImage::parse(synthetic_image(0, &location_stream, b"\0", &[])).unwrap();
        let location = ResourceLocation {
            location_offset: 0,
            uncompressed_size: 4,
            compressed_size: 0,
        };

        let error = image.get_resource(&location).unwrap_err();

        assert!(error.to_string().contains("JImage resource out of bounds"));
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn strings_table<const N: usize>(values: [&str; N]) -> Vec<u8> {
        let mut bytes = vec![0];
        for value in values {
            bytes.extend(value.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    fn string_offset(strings: &[u8], value: &str) -> u32 {
        strings
            .windows(value.len() + 1)
            .position(|window| window == [value.as_bytes(), &[0]].concat())
            .unwrap() as u32
    }

    fn resource_header(
        compressed_size: usize,
        uncompressed_size: usize,
        decompressor_name_offset: u32,
        terminal: bool,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(0xCAFEFAFAu32.to_le_bytes());
        bytes.extend((compressed_size as u64).to_le_bytes());
        bytes.extend((uncompressed_size as u64).to_le_bytes());
        bytes.extend(decompressor_name_offset.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.push(u8::from(terminal));
        bytes.extend(payload);
        bytes
    }

    fn synthetic_image(
        table_length: u32,
        locations: &[u8],
        strings: &[u8],
        resources: &[u8],
    ) -> Vec<u8> {
        let mut bytes = header(table_length, locations.len() as u32, strings.len() as u32);
        for _ in 0..table_length {
            bytes.extend(0i32.to_le_bytes());
        }
        for _ in 0..table_length {
            bytes.extend(0u32.to_le_bytes());
        }
        bytes.extend(locations);
        bytes.extend(strings);
        bytes.extend(resources);
        bytes
    }

    fn header(table_length: u32, locations_size: u32, strings_size: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(IMAGE_MAGIC.to_le_bytes());
        bytes.extend(0x0001_0000u32.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(table_length.to_le_bytes());
        bytes.extend(table_length.to_le_bytes());
        bytes.extend(locations_size.to_le_bytes());
        bytes.extend(strings_size.to_le_bytes());
        bytes
    }

    fn location_attribute(kind: usize, value: u64) -> Vec<u8> {
        let mut value_bytes = value.to_be_bytes().to_vec();
        while value_bytes.len() > 1 && value_bytes[0] == 0 {
            value_bytes.remove(0);
        }
        let header = ((kind as u8) << 3) | ((value_bytes.len() as u8) - 1);
        let mut bytes = vec![header];
        bytes.extend(value_bytes);
        bytes
    }

    fn compressed_int(value: u32) -> Vec<u8> {
        assert!(value <= 0x1f);
        vec![0x80 | 0x20 | value as u8]
    }
}
