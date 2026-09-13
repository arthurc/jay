//! Operand, call-return, and string-concat runtime helpers.

use std::io::Write;

use super::descriptors::{self, MethodDescriptor, ReturnType};
use super::frame::Frame;
use super::heap::ObjectRef;
use super::interpreter::Interpreter;
use super::native;
use super::value::Value;
use crate::classfile::ClassFile;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    /// Implements `checkcast`: `null` passes through, incompatible references
    /// raise `ClassCastException`.
    pub(super) fn check_cast(
        &self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let expected = class_file.constant_pool.class_name(index)?;
        let value = frame.pop_reference()?;
        if let Value::Reference(reference) = value {
            let actual = self.reference_type_name(reference)?;
            if !self.is_reference_compatible(&actual, expected)? {
                return Err(JayError::fault(
                    "java/lang/ClassCastException",
                    Some(format!(
                        "class {} cannot be cast to class {}",
                        self.heap.type_name(reference)?,
                        class_name_for_display(expected)
                    )),
                ));
            }
        }
        frame.stack.push(value);
        Ok(())
    }

    pub(super) fn pop_method_arguments(
        &self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_description: &str,
    ) -> JayResult<Vec<Value>> {
        let mut arguments = Vec::with_capacity(descriptor.parameter_types.len());
        for parameter_type in descriptor.parameter_types.iter().rev() {
            let value = caller.pop_value_of_type(parameter_type)?;
            self.validate_value_type(&value, parameter_type, target_description, "received")?;
            arguments.push(value);
        }
        arguments.reverse();
        Ok(arguments)
    }

    /// Pops two operand-stack words, matching JVM `pop2` semantics.
    ///
    /// If the first popped value is a category-2 value (`long` in this VM), the
    /// instruction is complete. Otherwise, this pops and discards a second
    /// category-1 value.
    pub(super) fn pop_two_words(&self, frame: &mut Frame) -> JayResult<()> {
        let first = frame.pop()?;
        if !matches!(first, Value::Long(_)) {
            let second = frame.pop()?;
            if matches!(second, Value::Long(_)) {
                return Err(JayError::new(
                    "invalid pop2 operand shape: category-1 value over category-2 value",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn complete_call(
        &self,
        caller: &mut Frame,
        return_type: ReturnType,
        result: Option<Value>,
        target_description: &str,
    ) -> JayResult<()> {
        match (return_type, result) {
            (ReturnType::Void, None) => Ok(()),
            (ReturnType::Void, Some(_)) => Err(JayError::new(format!(
                "{target_description} returned a value from void method"
            ))),
            (ReturnType::Type(descriptors::ValueType::Reference(_)), Some(Value::Null)) => {
                caller.stack.push(Value::Null);
                Ok(())
            }
            (ReturnType::Type(return_type), Some(value)) => {
                if let Some(actual_type) = value.value_type(&self.heap)?
                    && self.is_assignable_type(&actual_type, &return_type)?
                {
                    caller.stack.push(value);
                    Ok(())
                } else {
                    Err(JayError::new(format!(
                        "{target_description} returned {}, expected {}",
                        value.type_name(&self.heap)?,
                        return_type.name()
                    )))
                }
            }
            (ReturnType::Type(return_type), None) => Err(JayError::new(format!(
                "{target_description} returned void from {} method",
                return_type.name()
            ))),
        }
    }

    /// Formats a value the way string concatenation and `println` do.
    ///
    /// `value_type` distinguishes `char` and `boolean` from other `int`-carried
    /// values. General objects are formatted through their interpreted
    /// `toString()`, which may allocate and collect, so `value` must be rooted
    /// by the caller (typically by still being on `frame`'s operand stack).
    pub(super) fn value_to_text(
        &mut self,
        frame: &mut Frame,
        value_type: &descriptors::ValueType,
        value: &Value,
    ) -> JayResult<String> {
        match value {
            Value::Null => Ok("null".to_string()),
            Value::Int(int) => Ok(match value_type {
                descriptors::ValueType::Char => native::char_to_string(*int),
                descriptors::ValueType::Boolean => {
                    if *int == 0 { "false" } else { "true" }.to_string()
                }
                _ => int.to_string(),
            }),
            Value::Long(long) => Ok(long.to_string()),
            Value::Float(float) => Ok(native::float_to_string(*float)),
            Value::Reference(reference) => self.reference_to_text(frame, *reference),
            other => Err(JayError::new(format!(
                "unsupported text conversion for {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    /// Formats a heap object as `String.valueOf(Object)` would.
    ///
    /// Strings, builders, and boxed integers are read directly; other objects
    /// go through their interpreted `toString()`, so `reference` must be rooted
    /// by the caller.
    pub(super) fn reference_to_text(
        &mut self,
        frame: &mut Frame,
        reference: ObjectRef,
    ) -> JayResult<String> {
        let class_name = self.reference_type_name(reference)?;
        match class_name.as_str() {
            "java/lang/String" => Ok(self.heap.string(reference)?.to_string()),
            "java/lang/StringBuilder" => Ok(self.heap.string_builder(reference)?.to_string()),
            "java/lang/Integer" => Ok(self.boxed_integer_value(reference)?.to_string()),
            "java/time/LocalDateTime" => Ok(native::local_date_time_to_string(
                self.local_date_time_epoch_millis(reference)?,
            )),
            _ => {
                let text = self.invoke_reference_to_string(frame, reference)?;
                Ok(self.heap.string(text)?.to_string())
            }
        }
    }

    pub(super) fn pop_constructor_arguments(
        &self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_description: &str,
    ) -> JayResult<Vec<Value>> {
        let mut arguments = Vec::with_capacity(descriptor.parameter_types.len());
        for parameter_type in descriptor.parameter_types.iter().rev() {
            let value = caller.pop_value_of_type(parameter_type)?;
            self.validate_value_type(&value, parameter_type, target_description, "received")?;
            arguments.push(value);
        }
        arguments.reverse();
        Ok(arguments)
    }
}

/// Renders a constant-pool class name or array descriptor for a Java message.
fn class_name_for_display(name: &str) -> String {
    if name.starts_with('[') {
        super::heap::reference_array_name(name)
    } else {
        name.replace('/', ".")
    }
}

impl<'a, W: Write> Interpreter<'a, W> {
    /// Validates an array index against the array's length, raising
    /// `ArrayIndexOutOfBoundsException` with Java's message otherwise.
    pub(super) fn checked_array_index(&self, array: ObjectRef, index: i32) -> JayResult<usize> {
        let length = self.heap.array_length(array)?;
        match usize::try_from(index) {
            Ok(index) if index < length => Ok(index),
            _ => Err(super::heap::array_index_fault(index, length)),
        }
    }
}

pub(super) fn apply_string_concat_recipe(recipe: &str, arguments: &[String]) -> JayResult<String> {
    let mut output = String::new();
    let mut argument_index = 0usize;
    for character in recipe.chars() {
        match character {
            '\u{0001}' => {
                let Some(argument) = arguments.get(argument_index) else {
                    return Err(JayError::new(
                        "StringConcatFactory recipe has more placeholders than arguments",
                    ));
                };
                output.push_str(argument);
                argument_index += 1;
            }
            '\u{0002}' => {
                return Err(JayError::new(
                    "StringConcatFactory constant placeholders are unsupported",
                ));
            }
            _ => output.push(character),
        }
    }

    if argument_index != arguments.len() {
        return Err(JayError::new(
            "StringConcatFactory recipe has fewer placeholders than arguments",
        ));
    }

    Ok(output)
}
