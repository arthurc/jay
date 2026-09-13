//! Rust-side access to `java.lang.String` objects.
//!
//! A string is an ordinary `java/lang/String` instance laid out exactly as the
//! JDK expects — a `byte[] value` holding Latin-1 bytes (`coder == 0`) or
//! little-endian UTF-16 code units (`coder == 1`) — so the JDK's own `String`
//! bytecode runs on strings the VM creates, and strings the bytecode creates
//! can be read back here. Every place the VM reads or creates Java strings
//! from Rust — printing, exception messages, `invokedynamic` concatenation,
//! natives — goes through this module.
//!
//! String literals are interned per run (`intern_string`), which is what makes
//! `"a" == "a"` hold and what `String.intern()` returns.
//!
//! GC contract: `new_java_string` allocates without collecting and returns an
//! unrooted reference. Callers must root it (push it onto a frame stack, store
//! it in a field, ...) before anything that may call `collect_if_needed`.
//! Interned strings are rooted by the intern table.

use std::io::Write;

use super::frame::Frame;
use super::heap::{FieldKey, ObjectRef, PrimitiveElement};
use super::interpreter::Interpreter;
use super::value::Value;
use crate::{JayError, JayResult};

/// `String.LATIN1`: one byte per character.
const LATIN1: i32 = 0;

/// `String.UTF16`: two little-endian bytes per UTF-16 code unit.
const UTF16: i32 = 1;

impl<'a, W: Write> Interpreter<'a, W> {
    /// Reads the text of a `java.lang.String` object.
    pub(super) fn java_string(&self, reference: ObjectRef) -> JayResult<String> {
        if !self.is_java_string(reference) {
            return Err(JayError::new(format!(
                "expected String reference, found {}",
                self.heap.type_name(reference)?
            )));
        }
        let value = match self.heap.get_instance_field(reference, &value_field())? {
            Some(Value::Reference(value)) => value,
            _ => return Err(JayError::new("String value has not been initialized")),
        };
        let coder = match self.heap.get_instance_field(reference, &coder_field())? {
            Some(Value::Int(coder)) => coder,
            None => LATIN1,
            Some(other) => {
                return Err(JayError::new(format!(
                    "String coder found {}",
                    other.type_name(&self.heap)?
                )));
            }
        };
        let length = self.heap.array_length(value)?;
        let bytes = self.heap.read_array_bytes(value, 0, length)?;
        decode(&bytes, coder)
    }

    /// Returns whether `reference` points at a `java.lang.String`.
    pub(super) fn is_java_string(&self, reference: ObjectRef) -> bool {
        self.heap.instance_class_name(reference) == Ok("java/lang/String")
    }

    /// Allocates a new, unrooted `java.lang.String` holding `text`.
    ///
    /// Two objects are allocated (the `byte[]` and the string); neither
    /// allocation collects, so the pair is consistent when this returns.
    pub(super) fn new_java_string(&mut self, text: impl AsRef<str>) -> JayResult<ObjectRef> {
        let (bytes, coder) = encode(text.as_ref());
        let value = self
            .heap
            .allocate_primitive_array(PrimitiveElement::Byte, bytes.len());
        self.heap.write_array_bytes(value, 0, &bytes)?;
        let string = self.heap.allocate_instance("java/lang/String");
        self.heap
            .put_instance_field(string, value_field(), Value::Reference(value))?;
        self.heap
            .put_instance_field(string, coder_field(), Value::Int(coder))?;
        Ok(string)
    }

    /// Returns the canonical, rooted `String` for `text`, creating it on first use.
    pub(super) fn intern_string(&mut self, text: &str) -> JayResult<ObjectRef> {
        if let Some(reference) = self.interned_strings.get(text) {
            return Ok(*reference);
        }
        let reference = self.new_java_string(text)?;
        self.interned_strings.insert(text.to_string(), reference);
        Ok(reference)
    }

    /// Pops a non-null `String` reference from `frame`'s operand stack.
    pub(super) fn pop_java_string(&self, frame: &mut Frame) -> JayResult<ObjectRef> {
        match frame.pop()? {
            Value::Reference(reference) if self.is_java_string(reference) => Ok(reference),
            Value::Reference(reference) => Err(JayError::new(format!(
                "expected String reference, found {}",
                self.heap.type_name(reference)?
            ))),
            other => Err(JayError::new(format!(
                "expected string on stack, found {other:?}"
            ))),
        }
    }
}

/// The `String.value` field.
fn value_field() -> FieldKey {
    FieldKey::new("java/lang/String", "value", "[B")
}

/// The `String.coder` field.
fn coder_field() -> FieldKey {
    FieldKey::new("java/lang/String", "coder", "B")
}

/// Encodes text the way the JDK's compact strings do: Latin-1 when every
/// code unit fits in a byte, otherwise little-endian UTF-16.
fn encode(text: &str) -> (Vec<u8>, i32) {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.iter().all(|unit| *unit <= 0xFF) {
        (units.iter().map(|unit| *unit as u8).collect(), LATIN1)
    } else {
        (
            units.iter().flat_map(|unit| unit.to_le_bytes()).collect(),
            UTF16,
        )
    }
}

/// Decodes a `String.value` array; unpaired surrogates become U+FFFD.
fn decode(bytes: &[u8], coder: i32) -> JayResult<String> {
    match coder {
        LATIN1 => Ok(bytes.iter().map(|byte| *byte as char).collect()),
        UTF16 => {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect();
            Ok(String::from_utf16_lossy(&units))
        }
        other => Err(JayError::new(format!("unsupported String coder {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::classpath::ClassResolver;

    fn test_classes() -> ClassResolver {
        let root = std::env::temp_dir().join(format!(
            "jay-strings-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        ClassResolver::new(PathBuf::from(&root)).unwrap()
    }

    #[test]
    fn encoding_picks_latin1_when_possible_and_utf16_otherwise() {
        assert_eq!(
            encode("héllo"),
            (vec![b'h', 0xE9, b'l', b'l', b'o'], LATIN1)
        );
        assert_eq!(encode(""), (Vec::new(), LATIN1));
        // U+1F600 is a surrogate pair; each unit is stored low byte first.
        assert_eq!(
            encode("a\u{1F600}"),
            (vec![0x61, 0x00, 0x3D, 0xD8, 0x00, 0xDE], UTF16)
        );
        assert_eq!(decode(&[b'h', 0xE9], LATIN1).unwrap(), "hé");
        assert_eq!(
            decode(&[0x61, 0x00, 0x3D, 0xD8, 0x00, 0xDE], UTF16).unwrap(),
            "a\u{1F600}"
        );
        assert!(decode(&[], 2).is_err());
    }

    #[test]
    fn java_strings_round_trip_through_the_heap() {
        let classes = test_classes();
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);

        let text = interpreter.new_java_string("héllo \u{1F600}").unwrap();
        assert!(interpreter.is_java_string(text));
        assert_eq!(interpreter.java_string(text).unwrap(), "héllo \u{1F600}");
        assert_eq!(
            interpreter.heap.type_name(text).unwrap(),
            "java.lang.String"
        );

        let other = interpreter.heap.allocate_instance("example/Empty");
        assert!(!interpreter.is_java_string(other));
        assert!(interpreter.java_string(other).is_err());

        // A `new String` whose constructor has not run yet has no value.
        let blank = interpreter.heap.allocate_instance("java/lang/String");
        assert!(interpreter.is_java_string(blank));
        assert!(
            interpreter
                .java_string(blank)
                .unwrap_err()
                .to_string()
                .contains("not been initialized")
        );
    }

    #[test]
    fn interned_strings_are_shared_and_survive_collection() {
        let classes = test_classes();
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);

        let first = interpreter.intern_string("shared").unwrap();
        let second = interpreter.intern_string("shared").unwrap();
        assert_eq!(first, second);
        assert_ne!(interpreter.intern_string("other").unwrap(), first);

        let frame = Frame::new(0);
        for _ in 0..20 {
            interpreter.new_java_string("garbage").unwrap();
            interpreter.collect_if_needed(&frame);
        }
        assert_eq!(interpreter.java_string(first).unwrap(), "shared");
    }

    #[test]
    fn popping_a_java_string_rejects_other_values() {
        let classes = test_classes();
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);
        let mut frame = Frame::new(0);

        let text = interpreter.new_java_string("value").unwrap();
        frame.stack.push(Value::Reference(text));
        assert_eq!(interpreter.pop_java_string(&mut frame).unwrap(), text);

        let other = interpreter.heap.allocate_instance("example/Empty");
        frame.stack.push(Value::Reference(other));
        assert!(
            interpreter
                .pop_java_string(&mut frame)
                .unwrap_err()
                .to_string()
                .contains("expected String reference, found example.Empty")
        );

        frame.stack.push(Value::Int(1));
        assert!(
            interpreter
                .pop_java_string(&mut frame)
                .unwrap_err()
                .to_string()
                .contains("expected string on stack")
        );
    }
}
