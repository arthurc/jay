//! `jdk.internal.misc.Unsafe` natives over primitive arrays.
//!
//! The JDK addresses array elements as `(array, byteOffset)` pairs computed
//! from `arrayBaseOffset` and `arrayIndexScale`. Jay reports a fixed base
//! offset of 16 bytes and the natural element width as the scale, then serves
//! `getX`/`putX`/`copyMemory0` through the little-endian byte views in
//! `heap/memory.rs`. Only primitive arrays are addressable; field offsets,
//! off-heap memory, and reference accesses remain unsupported.

use std::io::Write;

use super::{Interpreter, JayError, JayResult, ObjectRef, Value};
use crate::vm::heap::PrimitiveElement;

/// The byte offset of element 0 in every array, as `Unsafe.arrayBaseOffset` reports it.
const ARRAY_BASE_OFFSET: i64 = 16;

/// The element width `Unsafe.arrayIndexScale` reports for reference arrays.
const REFERENCE_SCALE: i32 = 4;

impl<'a, W: Write> Interpreter<'a, W> {
    /// Handles an `Unsafe` native. Returns `Ok(None)` when the method is not one
    /// of the array-memory natives implemented here.
    pub(super) fn try_invoke_unsafe_native(
        &mut self,
        name: &str,
        descriptor: &str,
        arguments: &[Value],
    ) -> JayResult<Option<Option<Value>>> {
        let result = match (name, descriptor) {
            ("registerNatives", "()V") => None,
            ("arrayBaseOffset0", "(Ljava/lang/Class;)I") => {
                self.array_class_argument(arguments, "arrayBaseOffset0")?;
                Some(Value::Int(ARRAY_BASE_OFFSET as i32))
            }
            ("arrayIndexScale0", "(Ljava/lang/Class;)I") => {
                let type_name = self.array_class_argument(arguments, "arrayIndexScale0")?;
                let component = &type_name[1..];
                let scale = match component {
                    "D" => 8,
                    "Z" | "B" | "C" | "S" | "I" | "J" | "F" => {
                        PrimitiveElement::from_descriptor(component)?.byte_width() as i32
                    }
                    _ => REFERENCE_SCALE,
                };
                Some(Value::Int(scale))
            }
            ("getByte", "(Ljava/lang/Object;J)B") => {
                let bytes = self.unsafe_read(arguments, 1)?;
                Some(Value::Int(bytes[0] as i8 as i32))
            }
            ("getBoolean", "(Ljava/lang/Object;J)Z") => {
                let bytes = self.unsafe_read(arguments, 1)?;
                Some(Value::Int((bytes[0] != 0) as i32))
            }
            ("getShort", "(Ljava/lang/Object;J)S") => {
                let bytes = self.unsafe_read(arguments, 2)?;
                Some(Value::Int(i16::from_le_bytes([bytes[0], bytes[1]]) as i32))
            }
            ("getChar", "(Ljava/lang/Object;J)C") => {
                let bytes = self.unsafe_read(arguments, 2)?;
                Some(Value::Int(u16::from_le_bytes([bytes[0], bytes[1]]) as i32))
            }
            ("getInt", "(Ljava/lang/Object;J)I") => {
                let bytes = self.unsafe_read(arguments, 4)?;
                Some(Value::Int(i32::from_le_bytes(
                    bytes[..4].try_into().expect("4 bytes"),
                )))
            }
            ("getFloat", "(Ljava/lang/Object;J)F") => {
                let bytes = self.unsafe_read(arguments, 4)?;
                Some(Value::Float(f32::from_le_bytes(
                    bytes[..4].try_into().expect("4 bytes"),
                )))
            }
            ("getLong", "(Ljava/lang/Object;J)J") => {
                let bytes = self.unsafe_read(arguments, 8)?;
                Some(Value::Long(i64::from_le_bytes(
                    bytes[..8].try_into().expect("8 bytes"),
                )))
            }
            ("putByte", "(Ljava/lang/Object;JB)V") | ("putBoolean", "(Ljava/lang/Object;JZ)V") => {
                let value = int_payload(arguments, name)?;
                self.unsafe_write(arguments, &[value as u8])?;
                None
            }
            ("putShort", "(Ljava/lang/Object;JS)V") | ("putChar", "(Ljava/lang/Object;JC)V") => {
                let value = int_payload(arguments, name)?;
                self.unsafe_write(arguments, &(value as u16).to_le_bytes())?;
                None
            }
            ("putInt", "(Ljava/lang/Object;JI)V") => {
                let value = int_payload(arguments, name)?;
                self.unsafe_write(arguments, &value.to_le_bytes())?;
                None
            }
            ("putFloat", "(Ljava/lang/Object;JF)V") => {
                let Some(Value::Float(value)) = arguments.get(2) else {
                    return Err(JayError::new("Unsafe.putFloat expected a float payload"));
                };
                self.unsafe_write(arguments, &value.to_le_bytes())?;
                None
            }
            ("putLong", "(Ljava/lang/Object;JJ)V") => {
                let Some(Value::Long(value)) = arguments.get(2) else {
                    return Err(JayError::new("Unsafe.putLong expected a long payload"));
                };
                self.unsafe_write(arguments, &value.to_le_bytes())?;
                None
            }
            ("copyMemory0", "(Ljava/lang/Object;JLjava/lang/Object;JJ)V") => {
                let [
                    source,
                    Value::Long(source_offset),
                    destination,
                    Value::Long(destination_offset),
                    Value::Long(bytes),
                ] = arguments
                else {
                    return Err(JayError::new(
                        "Unsafe.copyMemory0 received unexpected arguments",
                    ));
                };
                let (source, source_offset) = array_address(source, *source_offset, "copyMemory0")?;
                let (destination, destination_offset) =
                    array_address(destination, *destination_offset, "copyMemory0")?;
                let length = usize::try_from(*bytes)
                    .map_err(|_| JayError::new("Unsafe.copyMemory0 received a negative length"))?;
                let payload = self.heap.read_array_bytes(source, source_offset, length)?;
                self.heap
                    .write_array_bytes(destination, destination_offset, &payload)?;
                None
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Reads `length` bytes at the `(array, offset)` address in `arguments[0..2]`.
    fn unsafe_read(&self, arguments: &[Value], length: usize) -> JayResult<Vec<u8>> {
        let (array, offset) = address_argument(arguments)?;
        self.heap.read_array_bytes(array, offset, length)
    }

    /// Writes `bytes` at the `(array, offset)` address in `arguments[0..2]`.
    fn unsafe_write(&mut self, arguments: &[Value], bytes: &[u8]) -> JayResult<()> {
        let (array, offset) = address_argument(arguments)?;
        self.heap.write_array_bytes(array, offset, bytes)
    }

    /// Reads the array-class mirror argument of `arrayBaseOffset0`/`arrayIndexScale0`.
    fn array_class_argument(&self, arguments: &[Value], target: &str) -> JayResult<String> {
        let mirror = match arguments {
            [Value::Reference(mirror)] => *mirror,
            [Value::Null] => return Err(JayError::fault("java/lang/NullPointerException", None)),
            _ => {
                return Err(JayError::new(format!(
                    "Unsafe.{target} expected a Class argument"
                )));
            }
        };
        let type_name = self.mirrored_class_name(mirror)?;
        if !type_name.starts_with('[') {
            return Err(JayError::fault(
                "java/lang/IllegalArgumentException",
                Some(format!(
                    "{} is not an array class",
                    type_name.replace('/', ".")
                )),
            ));
        }
        Ok(type_name)
    }
}

/// Decodes the `(Object, long)` address at the front of an `Unsafe` argument list.
fn address_argument(arguments: &[Value]) -> JayResult<(ObjectRef, usize)> {
    match arguments {
        [base, Value::Long(offset), ..] => array_address(base, *offset, "memory access"),
        _ => Err(JayError::new(
            "Unsafe memory access expected an object and a long offset",
        )),
    }
}

/// Translates an `Unsafe` base object and byte offset into an array and an
/// offset into its element storage.
fn array_address(base: &Value, offset: i64, target: &str) -> JayResult<(ObjectRef, usize)> {
    let Value::Reference(array) = base else {
        return Err(JayError::new(format!(
            "Unsafe.{target} supports array bases only, found {base:?}"
        )));
    };
    let element_offset = usize::try_from(offset - ARRAY_BASE_OFFSET).map_err(|_| {
        JayError::new(format!(
            "Unsafe.{target} offset {offset} precedes the array base offset {ARRAY_BASE_OFFSET}"
        ))
    })?;
    Ok((*array, element_offset))
}

/// The `int`-carried payload (`byte`, `boolean`, `short`, `char`, `int`) of a `putX` call.
fn int_payload(arguments: &[Value], name: &str) -> JayResult<i32> {
    match arguments.get(2) {
        Some(Value::Int(value)) => Ok(*value),
        other => Err(JayError::new(format!(
            "Unsafe.{name} expected an int payload, found {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_relative_to_the_array_base_offset() {
        let array = ObjectRef::from_index(3);
        assert_eq!(
            array_address(&Value::Reference(array), 16 + 5, "test").unwrap(),
            (array, 5)
        );
        assert!(array_address(&Value::Reference(array), 3, "test").is_err());
        assert!(array_address(&Value::Null, 16, "test").is_err());
        assert!(address_argument(&[Value::Reference(array)]).is_err());
        assert_eq!(
            int_payload(&[Value::Null, Value::Long(16), Value::Int(7)], "putInt").unwrap(),
            7
        );
    }
}
