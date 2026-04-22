//! Object allocation, constants, and field bytecode handlers.

use std::io::Write;

use super::descriptors::{FieldType, parse_field_descriptor};
use super::frame::Frame;
use super::heap::FieldKey;
use super::interpreter::Interpreter;
use super::value::Value;
use crate::classfile::ClassFile;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    pub(super) fn new_object(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let class_name = class_file.constant_pool.class_name(index)?;
        let reference = self.heap.allocate_instance(class_name);
        frame.stack.push(Value::Reference(reference));
        self.collect_if_needed(frame);
        Ok(())
    }

    pub(super) fn new_object_array(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let class_name = class_file.constant_pool.class_name(index)?;
        let length = frame.pop_int()?;
        if length < 0 {
            return Err(JayError::new(format!("negative array length {length}")));
        }

        let descriptor = reference_array_descriptor(class_name);
        let reference = self
            .heap
            .allocate_reference_array(descriptor, length as usize);
        frame.stack.push(Value::Reference(reference));
        self.collect_if_needed(frame);
        Ok(())
    }

    pub(super) fn load_constant(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let constant_pool = &class_file.constant_pool;
        if let Ok(value) = constant_pool.string(index) {
            let reference = self.heap.allocate_string(value);
            frame.stack.push(Value::Reference(reference));
            self.collect_if_needed(frame);
            return Ok(());
        }

        if let Ok(value) = constant_pool.integer(index) {
            frame.stack.push(Value::Int(value));
            return Ok(());
        }

        if let Ok(value) = constant_pool.float(index) {
            frame.stack.push(Value::Float(value));
            return Ok(());
        }

        if let Ok(class_name) = constant_pool.class_name(index) {
            let reference = self.class_mirror(class_name);
            frame.stack.push(Value::Reference(reference));
            self.collect_if_needed(frame);
            return Ok(());
        }

        Err(JayError::new(format!(
            "unsupported ldc constant at pool index #{index}"
        )))
    }

    pub(super) fn load_wide_constant(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        if let Ok(value) = class_file.constant_pool.long(index) {
            frame.stack.push(Value::Long(value));
            return Ok(());
        }

        Err(JayError::new(format!(
            "unsupported ldc2_w constant at pool index #{index}"
        )))
    }

    pub(super) fn get_static(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        if field.class_name == "java/lang/System"
            && field.name == "out"
            && field.descriptor == "Ljava/io/PrintStream;"
        {
            frame.stack.push(Value::PrintStream);
            Ok(())
        } else {
            let field_type = parse_field_descriptor(field.descriptor)?;
            let declaring_class_name =
                self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
            self.initialize_class(&declaring_class_name, frame)?;
            let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
            let value = self.static_fields.get(&field_key).cloned();
            match (field_type, value) {
                (FieldType::Int, Some(Value::Int(value))) => {
                    frame.stack.push(Value::Int(value));
                    Ok(())
                }
                (FieldType::Int, None) => {
                    frame.stack.push(Value::Int(0));
                    Ok(())
                }
                (FieldType::Float, Some(Value::Float(value))) => {
                    frame.stack.push(Value::Float(value));
                    Ok(())
                }
                (FieldType::Float, None) => {
                    frame.stack.push(Value::Float(0.0));
                    Ok(())
                }
                (FieldType::Long, Some(Value::Long(value))) => {
                    frame.stack.push(Value::Long(value));
                    Ok(())
                }
                (FieldType::Long, None) => {
                    frame.stack.push(Value::Long(0));
                    Ok(())
                }
                (FieldType::Reference, Some(value @ (Value::Reference(_) | Value::Null))) => {
                    frame.stack.push(value);
                    Ok(())
                }
                (FieldType::Reference, None) => {
                    frame.stack.push(Value::Null);
                    Ok(())
                }
                (_, Some(other)) => Err(JayError::new(format!(
                    "getstatic {}.{}:{} found {}",
                    field.class_name.replace('/', "."),
                    field.name,
                    field.descriptor,
                    other.type_name(&self.heap)?
                ))),
            }
        }
    }

    pub(super) fn put_static(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        let field_type = parse_field_descriptor(field.descriptor)?;
        let declaring_class_name =
            self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
        self.initialize_class(&declaring_class_name, frame)?;
        let value = frame.pop_field_value(field_type)?;
        let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
        self.static_fields.insert(field_key, value);
        Ok(())
    }

    pub(super) fn get_field(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        let field_type = parse_field_descriptor(field.descriptor)?;
        let receiver = frame.pop_object_ref()?;
        let declaring_class_name =
            self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
        let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
        let value = self.heap.get_instance_field(receiver, &field_key)?;
        match (field_type, value) {
            (FieldType::Int, Some(Value::Int(value))) => {
                frame.stack.push(Value::Int(value));
                Ok(())
            }
            (FieldType::Int, None) => {
                frame.stack.push(Value::Int(0));
                Ok(())
            }
            (FieldType::Float, Some(Value::Float(value))) => {
                frame.stack.push(Value::Float(value));
                Ok(())
            }
            (FieldType::Float, None) => {
                frame.stack.push(Value::Float(0.0));
                Ok(())
            }
            (FieldType::Long, Some(Value::Long(value))) => {
                frame.stack.push(Value::Long(value));
                Ok(())
            }
            (FieldType::Long, None) => {
                frame.stack.push(Value::Long(0));
                Ok(())
            }
            (FieldType::Reference, Some(value @ (Value::Reference(_) | Value::Null))) => {
                frame.stack.push(value);
                Ok(())
            }
            (FieldType::Reference, None) => {
                frame.stack.push(Value::Null);
                Ok(())
            }
            (_, Some(other)) => Err(JayError::new(format!(
                "getfield {}.{}:{} found {}",
                field.class_name.replace('/', "."),
                field.name,
                field.descriptor,
                other.type_name(&self.heap)?
            ))),
        }
    }

    pub(super) fn put_field(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        let field_type = parse_field_descriptor(field.descriptor)?;
        let value = frame.pop_field_value(field_type)?;
        let receiver = frame.pop_object_ref()?;
        let declaring_class_name =
            self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
        let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
        self.heap.put_instance_field(receiver, field_key, value)
    }
}

fn reference_array_descriptor(component_type: &str) -> String {
    if component_type.starts_with('[') {
        format!("[{component_type}")
    } else {
        format!("[L{component_type};")
    }
}

#[cfg(test)]
mod tests {
    use super::reference_array_descriptor;

    #[test]
    fn builds_reference_array_descriptor_for_class_components() {
        assert_eq!(
            reference_array_descriptor("java/lang/String"),
            "[Ljava/lang/String;"
        );
    }

    #[test]
    fn builds_reference_array_descriptor_for_array_components() {
        assert_eq!(
            reference_array_descriptor("[Ljava/lang/String;"),
            "[[Ljava/lang/String;"
        );
    }
}
