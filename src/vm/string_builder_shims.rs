//! `java.lang.StringBuilder` shims.
//!
//! The JDK implementation relies on `System.arraycopy`, `Unsafe`, and the
//! `StringLatin1`/`StringUTF16` intrinsics, none of which the VM provides.
//! Builders are therefore backed by a native buffer: the `new` opcode
//! allocates an ordinary instance and the constructor shim swaps its kind in
//! place, after which the common `append`/`toString` family is served here.

use std::io::Write;

use super::descriptors::ValueType;
use super::frame::Frame;
use super::heap::ObjectRef;
use super::interpreter::Interpreter;
use super::string_shims::{from_utf16, utf16};
use super::value::Value;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    /// Handles `new StringBuilder(...)` constructors.
    ///
    /// Returns `Ok(false)` when the constructor is not shimmed.
    pub(super) fn try_invoke_string_builder_constructor(
        &mut self,
        frame: &mut Frame,
        descriptor: &str,
    ) -> JayResult<bool> {
        let initial = match descriptor {
            "()V" => String::new(),
            "(I)V" => {
                // The capacity hint has no effect on a native buffer.
                frame.pop_int()?;
                String::new()
            }
            "(Ljava/lang/String;)V" => {
                let reference = self.pop_java_string(frame)?;
                self.java_string(reference)?
            }
            "(Ljava/lang/CharSequence;)V" => {
                let reference = frame.pop_object_ref()?;
                self.char_sequence_text(reference)?
            }
            _ => return Ok(false),
        };
        let receiver = frame.pop_object_ref()?;
        self.heap.replace_with_string_builder(receiver, initial)?;
        Ok(true)
    }

    /// Handles an `invokevirtual` on a `StringBuilder` receiver.
    ///
    /// Returns `Ok(false)` when the method is not shimmed. Arguments have
    /// already been popped. `append(Object)` may run an interpreted
    /// `toString()`, so the receiver is pushed back onto `frame` to keep it
    /// rooted for the duration of the call.
    pub(super) fn try_invoke_string_builder_method(
        &mut self,
        frame: &mut Frame,
        name: &str,
        descriptor: &str,
        receiver: ObjectRef,
        arguments: &[Value],
    ) -> JayResult<bool> {
        let result = match (name, descriptor) {
            ("append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;")
            | ("append", "(Ljava/lang/Object;)Ljava/lang/StringBuilder;")
            | ("append", "(Ljava/lang/CharSequence;)Ljava/lang/StringBuilder;")
            | ("append", "(Ljava/lang/StringBuffer;)Ljava/lang/StringBuilder;") => {
                let argument = single_argument(arguments, "StringBuilder.append")?;
                frame.stack.push(Value::Reference(receiver));
                let object_type = ValueType::Reference("java/lang/Object".to_string());
                let text = self.value_to_text(frame, &object_type, &argument);
                frame.pop()?;
                self.heap.string_builder_mut(receiver)?.push_str(&text?);
                Value::Reference(receiver)
            }
            ("append", "(I)Ljava/lang/StringBuilder;")
            | ("append", "(J)Ljava/lang/StringBuilder;")
            | ("append", "(F)Ljava/lang/StringBuilder;")
            | ("append", "(C)Ljava/lang/StringBuilder;")
            | ("append", "(Z)Ljava/lang/StringBuilder;") => {
                let argument = single_argument(arguments, "StringBuilder.append")?;
                let value_type = match descriptor.as_bytes()[1] {
                    b'C' => ValueType::Char,
                    b'Z' => ValueType::Boolean,
                    b'J' => ValueType::Long,
                    b'F' => ValueType::Float,
                    _ => ValueType::Int,
                };
                let text = self.value_to_text(frame, &value_type, &argument)?;
                self.heap.string_builder_mut(receiver)?.push_str(&text);
                Value::Reference(receiver)
            }
            ("toString", "()Ljava/lang/String;") => {
                let text = self.heap.string_builder(receiver)?.to_string();
                Value::Reference(self.new_java_string(text))
            }
            ("length", "()I") => {
                let length = self.heap.string_builder(receiver)?.encode_utf16().count();
                Value::Int(length as i32)
            }
            ("isEmpty", "()Z") => {
                let empty = self.heap.string_builder(receiver)?.is_empty();
                Value::Int(if empty { 1 } else { 0 })
            }
            ("charAt", "(I)C") => {
                let Some(Value::Int(index)) = arguments.first() else {
                    return Err(JayError::new("StringBuilder.charAt expected an int"));
                };
                let units = utf16(self.heap.string_builder(receiver)?);
                let unit = usize::try_from(*index)
                    .ok()
                    .and_then(|index| units.get(index).copied())
                    .ok_or_else(|| {
                        JayError::fault(
                            "java/lang/StringIndexOutOfBoundsException",
                            Some(format!("index {index},length {}", units.len())),
                        )
                    })?;
                Value::Int(unit as i32)
            }
            ("reverse", "()Ljava/lang/StringBuilder;") => {
                let buffer = self.heap.string_builder_mut(receiver)?;
                *buffer = buffer.chars().rev().collect();
                Value::Reference(receiver)
            }
            ("setLength", "(I)V") => {
                let Some(Value::Int(length)) = arguments.first() else {
                    return Err(JayError::new("StringBuilder.setLength expected an int"));
                };
                if *length < 0 {
                    return Err(JayError::fault(
                        "java/lang/StringIndexOutOfBoundsException",
                        Some(format!("String index out of range: {length}")),
                    ));
                }
                let buffer = self.heap.string_builder_mut(receiver)?;
                let mut units = utf16(buffer);
                units.resize(*length as usize, 0);
                *buffer = from_utf16(&units);
                return Ok(true);
            }
            _ => return Ok(false),
        };

        frame.stack.push(result);
        self.collect_if_needed(frame);
        Ok(true)
    }

    /// Reads the text of a `CharSequence` argument, which in this VM is a
    /// `String` or a `StringBuilder`.
    fn char_sequence_text(&self, reference: ObjectRef) -> JayResult<String> {
        if self.is_java_string(reference) {
            return self.java_string(reference);
        }
        Ok(self.heap.string_builder(reference)?.to_string())
    }
}

fn single_argument(arguments: &[Value], target: &str) -> JayResult<Value> {
    match arguments {
        [argument] => Ok(argument.clone()),
        _ => Err(JayError::new(format!(
            "{target} expected one argument, found {}",
            arguments.len()
        ))),
    }
}
