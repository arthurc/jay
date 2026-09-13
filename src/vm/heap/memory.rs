//! Byte-level views of primitive arrays, the memory model behind the
//! `jdk.internal.misc.Unsafe` array accessors.
//!
//! Elements are exposed as little-endian bytes packed at their natural width,
//! matching the `BIG_ENDIAN = false` that `UnsafeConstants` reports to JDK
//! code. Reads and writes may start at any byte offset and span any number of
//! elements; a range that leaves the array is a VM error rather than a Java
//! exception, since HotSpot would simply read garbage.

use super::{Heap, ObjectKind, ObjectRef, PrimitiveArray, PrimitiveElement};
use crate::{JayError, JayResult};

impl PrimitiveElement {
    /// The width of one element in bytes, as `Unsafe.arrayIndexScale` reports it.
    pub(in crate::vm) fn byte_width(self) -> usize {
        match self {
            Self::Boolean | Self::Byte => 1,
            Self::Char | Self::Short => 2,
            Self::Int | Self::Float => 4,
            Self::Long => 8,
        }
    }
}

impl Heap {
    /// Reads `length` bytes of a primitive array starting `byte_offset` bytes
    /// into its element storage.
    pub(in crate::vm) fn read_array_bytes(
        &self,
        reference: ObjectRef,
        byte_offset: usize,
        length: usize,
    ) -> JayResult<Vec<u8>> {
        match self.object(reference)?.kind {
            ObjectKind::PrimitiveArray(ref array) => array.read_bytes(byte_offset, length),
            _ => Err(JayError::new(format!(
                "expected primitive array for raw memory access, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    /// Overwrites the bytes of a primitive array starting `byte_offset` bytes
    /// into its element storage.
    pub(in crate::vm) fn write_array_bytes(
        &mut self,
        reference: ObjectRef,
        byte_offset: usize,
        bytes: &[u8],
    ) -> JayResult<()> {
        let type_name = self.type_name(reference)?;
        match self.object_mut(reference)?.kind {
            ObjectKind::PrimitiveArray(ref mut array) => array.write_bytes(byte_offset, bytes),
            _ => Err(JayError::new(format!(
                "expected primitive array for raw memory access, found {type_name}"
            ))),
        }
    }
}

impl PrimitiveArray {
    fn byte_len(&self) -> usize {
        self.len() * self.element().byte_width()
    }

    /// Serializes the elements covering `[byte_offset, byte_offset + length)`
    /// and returns exactly that byte range.
    fn read_bytes(&self, byte_offset: usize, length: usize) -> JayResult<Vec<u8>> {
        let end = byte_offset
            .checked_add(length)
            .filter(|end| *end <= self.byte_len())
            .ok_or_else(|| self.out_of_bounds(byte_offset, length))?;
        let width = self.element().byte_width();
        let (first, last) = (byte_offset / width, end.div_ceil(width));
        let covering = self.elements_to_bytes(first, last);
        let skip = byte_offset - first * width;
        Ok(covering[skip..skip + length].to_vec())
    }

    /// Read-modify-writes the elements covering `bytes` at `byte_offset`.
    fn write_bytes(&mut self, byte_offset: usize, bytes: &[u8]) -> JayResult<()> {
        let end = byte_offset
            .checked_add(bytes.len())
            .filter(|end| *end <= self.byte_len())
            .ok_or_else(|| self.out_of_bounds(byte_offset, bytes.len()))?;
        let width = self.element().byte_width();
        let (first, last) = (byte_offset / width, end.div_ceil(width));
        let mut covering = self.elements_to_bytes(first, last);
        let skip = byte_offset - first * width;
        covering[skip..skip + bytes.len()].copy_from_slice(bytes);
        self.bytes_to_elements(first, &covering);
        Ok(())
    }

    fn out_of_bounds(&self, byte_offset: usize, length: usize) -> JayError {
        JayError::new(format!(
            "raw memory access of {length} bytes at offset {byte_offset} leaves {} of {} bytes",
            self.element().array_name(),
            self.byte_len()
        ))
    }

    /// Little-endian bytes of elements `first..last`.
    fn elements_to_bytes(&self, first: usize, last: usize) -> Vec<u8> {
        match self {
            Self::Boolean(values) | Self::Byte(values) => {
                values[first..last].iter().map(|v| *v as u8).collect()
            }
            Self::Char(values) => values[first..last]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
            Self::Short(values) => values[first..last]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
            Self::Int(values) => values[first..last]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
            Self::Float(values) => values[first..last]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
            Self::Long(values) => values[first..last]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        }
    }

    /// Stores little-endian `bytes` (a whole number of elements) from element `first`.
    fn bytes_to_elements(&mut self, first: usize, bytes: &[u8]) {
        fn decode<const N: usize, T>(bytes: &[u8], from: fn([u8; N]) -> T) -> Vec<T> {
            bytes
                .as_chunks::<N>()
                .0
                .iter()
                .map(|chunk| from(*chunk))
                .collect()
        }
        match self {
            Self::Boolean(values) => {
                for (slot, byte) in values[first..].iter_mut().zip(bytes) {
                    *slot = (*byte & 1) as i8;
                }
            }
            Self::Byte(values) => {
                for (slot, byte) in values[first..].iter_mut().zip(bytes) {
                    *slot = *byte as i8;
                }
            }
            Self::Char(values) => {
                let decoded = decode(bytes, u16::from_le_bytes);
                values[first..first + decoded.len()].copy_from_slice(&decoded);
            }
            Self::Short(values) => {
                let decoded = decode(bytes, i16::from_le_bytes);
                values[first..first + decoded.len()].copy_from_slice(&decoded);
            }
            Self::Int(values) => {
                let decoded = decode(bytes, i32::from_le_bytes);
                values[first..first + decoded.len()].copy_from_slice(&decoded);
            }
            Self::Float(values) => {
                let decoded = decode(bytes, f32::from_le_bytes);
                values[first..first + decoded.len()].copy_from_slice(&decoded);
            }
            Self::Long(values) => {
                let decoded = decode(bytes, i64::from_le_bytes);
                values[first..first + decoded.len()].copy_from_slice(&decoded);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::Value;

    #[test]
    fn byte_views_serialize_elements_little_endian_at_any_offset() {
        let mut heap = Heap::new();
        let ints = heap.allocate_primitive_array(PrimitiveElement::Int, 3);
        heap.store_primitive(ints, 0, Value::Int(0x0403_0201))
            .unwrap();
        heap.store_primitive(ints, 1, Value::Int(0x0807_0605))
            .unwrap();
        heap.store_primitive(ints, 2, Value::Int(-1)).unwrap();

        assert_eq!(
            heap.read_array_bytes(ints, 0, 8).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(heap.read_array_bytes(ints, 3, 3).unwrap(), vec![4, 5, 6]);
        assert_eq!(heap.read_array_bytes(ints, 11, 1).unwrap(), vec![0xff]);
        assert!(heap.read_array_bytes(ints, 11, 2).is_err());
        assert!(heap.read_array_bytes(ints, 12, 0).is_ok());
    }

    #[test]
    fn byte_writes_update_only_the_covered_bytes() {
        let mut heap = Heap::new();
        let chars = heap.allocate_primitive_array(PrimitiveElement::Char, 2);
        heap.write_array_bytes(chars, 1, &[0x41, 0x42]).unwrap();
        assert_eq!(heap.load_primitive(chars, 0).unwrap(), Value::Int(0x4100));
        assert_eq!(heap.load_primitive(chars, 1).unwrap(), Value::Int(0x0042));
        assert!(heap.write_array_bytes(chars, 3, &[0, 0]).is_err());

        let longs = heap.allocate_primitive_array(PrimitiveElement::Long, 1);
        heap.write_array_bytes(longs, 0, &[1, 0, 0, 0, 0, 0, 0, 0x80])
            .unwrap();
        assert_eq!(
            heap.load_primitive(longs, 0).unwrap(),
            Value::Long(i64::MIN + 1)
        );

        let flags = heap.allocate_primitive_array(PrimitiveElement::Boolean, 2);
        heap.write_array_bytes(flags, 0, &[3, 0]).unwrap();
        assert_eq!(heap.load_primitive(flags, 0).unwrap(), Value::Int(1));
    }

    #[test]
    fn byte_views_reject_non_primitive_arrays() {
        let mut heap = Heap::new();
        let instance = heap.allocate_instance("example/Empty");
        assert!(heap.read_array_bytes(instance, 0, 1).is_err());
        assert!(heap.write_array_bytes(instance, 0, &[0]).is_err());
        assert_eq!(PrimitiveElement::Long.byte_width(), 8);
    }
}
