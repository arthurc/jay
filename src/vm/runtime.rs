//! Operand, call-return, and string-concat runtime helpers.

use std::io::Write;

use super::descriptors::{self, MethodDescriptor, ReturnType};
use super::frame::Frame;
use super::interpreter::Interpreter;
use super::native;
use super::value::Value;
use crate::classfile::ClassFile;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    pub(super) fn check_cast(
        &self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let class_name = class_file.constant_pool.class_name(index)?;
        let expected_type = descriptors::ValueType::Reference(class_name.to_string());
        let value = frame.pop_reference()?;
        self.validate_value_type(
            &value,
            &expected_type,
            &format!("checkcast target {}", class_name.replace('/', ".")),
            "checked",
        )?;
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

    pub(super) fn string_concat_argument(&self, value: Value) -> JayResult<String> {
        match value {
            Value::Null => Ok("null".to_string()),
            Value::Int(value) => Ok(value.to_string()),
            Value::Reference(reference) => Ok(self.heap.string(reference)?.to_string()),
            other => Err(JayError::new(format!(
                "unsupported string concat argument {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    /// Formats the focused subset of values supported by PrintStream.println(Object).
    pub(super) fn println_object_text(&self, value: Value) -> JayResult<String> {
        match value {
            Value::Null => Ok("null".to_string()),
            Value::Reference(reference) => match self.heap.value_type(reference)? {
                Some(descriptors::ValueType::Reference(class_name)) => match class_name.as_str() {
                    "java/lang/String" => Ok(self.heap.string(reference)?.to_string()),
                    "java/util/Date" => Ok(native::date_to_string(self.date_fast_time(reference)?)),
                    "java/time/LocalDateTime" => Ok(native::local_date_time_to_string(
                        self.local_date_time_epoch_millis(reference)?,
                    )),
                    _ => Err(JayError::new(format!(
                        "unsupported PrintStream.println(Object) value {}",
                        self.heap.type_name(reference)?
                    ))),
                },
                _ => Err(JayError::new(format!(
                    "unsupported PrintStream.println(Object) value {}",
                    self.heap.type_name(reference)?
                ))),
            },
            other => Err(JayError::new(format!(
                "unsupported PrintStream.println(Object) value {}",
                other.type_name(&self.heap)?
            ))),
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

pub(super) fn checked_array_index(index: i32) -> JayResult<usize> {
    usize::try_from(index).map_err(|_| JayError::new(format!("negative array index {index}")))
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
