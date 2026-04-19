//! Stack frame storage for local variables and operand stack operations.

use super::descriptors::{FieldType, ValueType};
use super::heap::{Heap, ObjectRef};
use super::value::Value;
use crate::{JayError, JayResult};

pub(super) struct Frame {
    pub(super) stack: Vec<Value>,
    locals: Vec<Value>,
}

impl Frame {
    pub(super) fn new(max_locals: u16) -> Self {
        Self {
            stack: Vec::new(),
            locals: vec![Value::Uninitialized; max_locals as usize],
        }
    }

    pub(super) fn with_arguments(max_locals: u16, arguments: Vec<Value>) -> JayResult<Self> {
        let mut frame = Self::new(max_locals);
        if arguments.len() > frame.locals.len() {
            return Err(JayError::new(format!(
                "method expects {} argument locals but max locals is {}",
                arguments.len(),
                frame.locals.len()
            )));
        }

        for (index, value) in arguments.into_iter().enumerate() {
            frame.locals[index] = value;
        }

        Ok(frame)
    }

    pub(super) fn load_int_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.local_int(index)?;
        self.stack.push(Value::Int(value));
        Ok(())
    }

    pub(super) fn store_int_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.pop_int()?;
        let slot = self.local_slot_mut(index)?;
        *slot = Value::Int(value);
        Ok(())
    }

    pub(super) fn load_reference_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.local_reference(index)?.clone();
        self.stack.push(value);
        Ok(())
    }

    pub(super) fn store_reference_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.pop_reference()?;
        let slot = self.local_slot_mut(index)?;
        *slot = value;
        Ok(())
    }

    pub(super) fn increment_int_local(&mut self, index: usize, value: i32) -> JayResult<()> {
        let current = self.local_int(index)?;
        let slot = self.local_slot_mut(index)?;
        *slot = Value::Int(current.wrapping_add(value));
        Ok(())
    }

    pub(super) fn duplicate_top(&mut self) -> JayResult<()> {
        let value = self
            .stack
            .last()
            .cloned()
            .ok_or_else(|| JayError::new("operand stack underflow on dup"))?;
        self.stack.push(value);
        Ok(())
    }

    fn local_int(&self, index: usize) -> JayResult<i32> {
        match self.local_slot(index)? {
            Value::Int(value) => Ok(*value),
            Value::Uninitialized => Err(JayError::new(format!(
                "local variable #{index} is uninitialized"
            ))),
            other => Err(JayError::new(format!(
                "expected int in local variable #{index}, found {other:?}"
            ))),
        }
    }

    fn local_reference(&self, index: usize) -> JayResult<&Value> {
        match self.local_slot(index)? {
            value @ Value::Reference(_) => Ok(value),
            Value::Uninitialized => Err(JayError::new(format!(
                "local variable #{index} is uninitialized"
            ))),
            other => Err(JayError::new(format!(
                "expected reference in local variable #{index}, found {other:?}"
            ))),
        }
    }

    fn local_slot(&self, index: usize) -> JayResult<&Value> {
        self.locals.get(index).ok_or_else(|| {
            JayError::new(format!(
                "invalid local variable index #{index}; max locals {}",
                self.locals.len()
            ))
        })
    }

    fn local_slot_mut(&mut self, index: usize) -> JayResult<&mut Value> {
        let max_locals = self.locals.len();
        self.locals.get_mut(index).ok_or_else(|| {
            JayError::new(format!(
                "invalid local variable index #{index}; max locals {max_locals}"
            ))
        })
    }

    pub(super) fn pop_print_stream(&mut self) -> JayResult<()> {
        match self.pop()? {
            Value::PrintStream => Ok(()),
            other => Err(JayError::new(format!(
                "expected PrintStream receiver on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_string_reference(&mut self, heap: &Heap) -> JayResult<ObjectRef> {
        match self.pop()? {
            Value::Reference(reference) => {
                let _ = heap.string(reference)?;
                Ok(reference)
            }
            other => Err(JayError::new(format!(
                "expected string on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_int(&mut self) -> JayResult<i32> {
        match self.pop()? {
            Value::Int(value) => Ok(value),
            other => Err(JayError::new(format!(
                "expected int on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_object_ref(&mut self) -> JayResult<ObjectRef> {
        match self.pop_reference()? {
            Value::Reference(reference) => Ok(reference),
            other => Err(JayError::new(format!(
                "expected reference on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_reference(&mut self) -> JayResult<Value> {
        match self.pop()? {
            value @ Value::Reference(_) => Ok(value),
            other => Err(JayError::new(format!(
                "expected reference on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_value_of_type(
        &mut self,
        value_type: ValueType,
        heap: &Heap,
    ) -> JayResult<Value> {
        match value_type {
            ValueType::Int => Ok(Value::Int(self.pop_int()?)),
            ValueType::String => Ok(Value::Reference(self.pop_string_reference(heap)?)),
        }
    }

    pub(super) fn pop_field_value(&mut self, field_type: FieldType) -> JayResult<Value> {
        match field_type {
            FieldType::Int => Ok(Value::Int(self.pop_int()?)),
            FieldType::Reference => self.pop_reference(),
        }
    }

    pub(super) fn pop(&mut self) -> JayResult<Value> {
        self.stack
            .pop()
            .ok_or_else(|| JayError::new("operand stack underflow"))
    }

    pub(super) fn roots(&self) -> impl Iterator<Item = &Value> {
        self.locals
            .iter()
            .chain(self.stack.iter())
            .filter(|value| matches!(value, Value::Reference(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_collection_keeps_frame_local_and_stack_references() {
        let mut heap = Heap::new();
        let local = heap.allocate_string("local");
        let stack = heap.allocate_string("stack");
        let dropped = heap.allocate_string("dropped");
        let mut frame = Frame::new(1);
        frame.locals[0] = Value::Reference(local);
        frame.stack.push(Value::Reference(stack));

        heap.collect(frame.roots());

        assert_eq!(heap.string(local).unwrap(), "local");
        assert_eq!(heap.string(stack).unwrap(), "stack");
        assert!(heap.string(dropped).is_err());
    }

    #[test]
    fn string_type_errors_still_name_expected_string_values() {
        let heap = Heap::new();
        let mut frame = Frame::new(0);
        frame.stack.push(Value::Int(42));

        let error = frame
            .pop_value_of_type(ValueType::String, &heap)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expected string on stack, found Int(42)")
        );
    }
}
