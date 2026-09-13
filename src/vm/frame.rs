//! Stack frame storage for local variables and operand stack operations.

use super::descriptors::{FieldType, ValueType};
use super::heap::ObjectRef;
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
        let required_locals = arguments.iter().map(value_local_width).sum::<usize>();
        if required_locals > frame.locals.len() {
            return Err(JayError::new(format!(
                "method expects {} argument locals but max locals is {}",
                required_locals,
                frame.locals.len()
            )));
        }

        // Category-2 values occupy two local variable slots in JVM frames.
        let mut index = 0usize;
        for value in arguments {
            let width = value_local_width(&value);
            frame.locals[index] = value;
            if width == 2 {
                frame.locals[index + 1] = Value::Uninitialized;
            }
            index += width;
        }

        Ok(frame)
    }

    pub(super) fn load_int_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.local_int(index)?;
        self.stack.push(Value::Int(value));
        Ok(())
    }

    pub(super) fn load_float_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.local_float(index)?;
        self.stack.push(Value::Float(value));
        Ok(())
    }

    pub(super) fn store_int_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.pop_int()?;
        let slot = self.local_slot_mut(index)?;
        *slot = Value::Int(value);
        Ok(())
    }

    pub(super) fn store_float_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.pop_float()?;
        let slot = self.local_slot_mut(index)?;
        *slot = Value::Float(value);
        Ok(())
    }

    pub(super) fn load_long_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.local_long(index)?;
        self.stack.push(Value::Long(value));
        Ok(())
    }

    pub(super) fn store_long_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.pop_long()?;
        self.ensure_category_two_local(index)?;
        self.locals[index] = Value::Long(value);
        self.locals[index + 1] = Value::Uninitialized;
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

    pub(super) fn duplicate_top_insert_two_down(&mut self) -> JayResult<()> {
        let len = self.stack.len();
        if len < 2 {
            return Err(JayError::new("operand stack underflow on dup_x1"));
        }

        let value = self.stack[len - 1].clone();
        self.stack.insert(len - 2, value);
        Ok(())
    }

    /// Implements `dup_x2`: duplicates the top value beneath the next one or two values.
    ///
    /// Category-2 values (`long`) occupy one stack entry in this VM, so the
    /// second value being a `Long` means the copy goes only two slots down.
    pub(super) fn duplicate_top_insert_three_down(&mut self) -> JayResult<()> {
        let len = self.stack.len();
        if len < 2 {
            return Err(JayError::new("operand stack underflow on dup_x2"));
        }

        let value = self.stack[len - 1].clone();
        let depth = if matches!(self.stack[len - 2], Value::Long(_)) {
            2
        } else {
            3
        };
        if len < depth {
            return Err(JayError::new("operand stack underflow on dup_x2"));
        }
        self.stack.insert(len - depth, value);
        Ok(())
    }

    /// Implements `dup2`: duplicates the top two category-1 values or one `long`.
    pub(super) fn duplicate_top_two(&mut self) -> JayResult<()> {
        let len = self.stack.len();
        let Some(top) = self.stack.last().cloned() else {
            return Err(JayError::new("operand stack underflow on dup2"));
        };
        if matches!(top, Value::Long(_)) {
            self.stack.push(top);
            return Ok(());
        }
        if len < 2 {
            return Err(JayError::new("operand stack underflow on dup2"));
        }
        let second = self.stack[len - 2].clone();
        self.stack.push(second);
        self.stack.push(top);
        Ok(())
    }

    /// Implements `dup2_x1`: duplicates the top two words beneath the third.
    pub(super) fn duplicate_top_two_insert_three_down(&mut self) -> JayResult<()> {
        let len = self.stack.len();
        let Some(top) = self.stack.last().cloned() else {
            return Err(JayError::new("operand stack underflow on dup2_x1"));
        };
        if matches!(top, Value::Long(_)) {
            if len < 2 {
                return Err(JayError::new("operand stack underflow on dup2_x1"));
            }
            self.stack.insert(len - 2, top);
            return Ok(());
        }
        if len < 3 {
            return Err(JayError::new("operand stack underflow on dup2_x1"));
        }
        let second = self.stack[len - 2].clone();
        self.stack.insert(len - 3, top);
        self.stack.insert(len - 3, second);
        Ok(())
    }

    /// Implements `swap` for two category-1 values.
    pub(super) fn swap_top_two(&mut self) -> JayResult<()> {
        let len = self.stack.len();
        if len < 2 {
            return Err(JayError::new("operand stack underflow on swap"));
        }
        if matches!(self.stack[len - 1], Value::Long(_))
            || matches!(self.stack[len - 2], Value::Long(_))
        {
            return Err(JayError::new("swap requires two category-1 values"));
        }
        self.stack.swap(len - 1, len - 2);
        Ok(())
    }

    pub(super) fn references_equal(&self, left: &Value, right: &Value) -> JayResult<bool> {
        match (left, right) {
            (Value::Reference(left), Value::Reference(right)) => Ok(left == right),
            (Value::Null, Value::Null) => Ok(true),
            (Value::Reference(_), Value::Null) | (Value::Null, Value::Reference(_)) => Ok(false),
            _ => Err(JayError::new(format!(
                "expected references for comparison, found {left:?} and {right:?}"
            ))),
        }
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

    fn local_float(&self, index: usize) -> JayResult<f32> {
        match self.local_slot(index)? {
            Value::Float(value) => Ok(*value),
            Value::Uninitialized => Err(JayError::new(format!(
                "local variable #{index} is uninitialized"
            ))),
            other => Err(JayError::new(format!(
                "expected float in local variable #{index}, found {other:?}"
            ))),
        }
    }

    fn local_long(&self, index: usize) -> JayResult<i64> {
        self.ensure_category_two_local(index)?;
        match self.local_slot(index)? {
            Value::Long(value) => Ok(*value),
            Value::Uninitialized => Err(JayError::new(format!(
                "local variable #{index} is uninitialized"
            ))),
            other => Err(JayError::new(format!(
                "expected long in local variable #{index}, found {other:?}"
            ))),
        }
    }

    fn local_reference(&self, index: usize) -> JayResult<&Value> {
        match self.local_slot(index)? {
            value @ (Value::Reference(_) | Value::Null) => Ok(value),
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

    fn ensure_category_two_local(&self, index: usize) -> JayResult<()> {
        if index + 1 < self.locals.len() {
            return Ok(());
        }

        Err(JayError::new(format!(
            "invalid category-2 local variable index #{index}; max locals {}",
            self.locals.len()
        )))
    }

    pub(super) fn pop_print_stream(&mut self) -> JayResult<()> {
        match self.pop()? {
            Value::PrintStream => Ok(()),
            other => Err(JayError::new(format!(
                "expected PrintStream receiver on stack, found {other:?}"
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

    pub(super) fn pop_float(&mut self) -> JayResult<f32> {
        match self.pop()? {
            Value::Float(value) => Ok(value),
            other => Err(JayError::new(format!(
                "expected float on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_long(&mut self) -> JayResult<i64> {
        match self.pop()? {
            Value::Long(value) => Ok(value),
            other => Err(JayError::new(format!(
                "expected long on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_object_ref(&mut self) -> JayResult<ObjectRef> {
        match self.pop_reference()? {
            Value::Reference(reference) => Ok(reference),
            Value::Null => Err(JayError::fault("java/lang/NullPointerException", None)),
            other => Err(JayError::new(format!(
                "expected reference on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_reference(&mut self) -> JayResult<Value> {
        match self.pop()? {
            value @ (Value::Reference(_) | Value::Null) => Ok(value),
            other => Err(JayError::new(format!(
                "expected reference on stack, found {other:?}"
            ))),
        }
    }

    pub(super) fn pop_value_of_type(&mut self, value_type: &ValueType) -> JayResult<Value> {
        match value_type {
            ValueType::Boolean
            | ValueType::Byte
            | ValueType::Char
            | ValueType::Short
            | ValueType::Int => Ok(Value::Int(self.pop_int()?)),
            ValueType::Float => Ok(Value::Float(self.pop_float()?)),
            ValueType::Long => Ok(Value::Long(self.pop_long()?)),
            ValueType::Reference(_) => self.pop_reference(),
        }
    }

    pub(super) fn pop_field_value(&mut self, field_type: FieldType) -> JayResult<Value> {
        match field_type {
            FieldType::Int => Ok(Value::Int(self.pop_int()?)),
            FieldType::Float => Ok(Value::Float(self.pop_float()?)),
            FieldType::Long => Ok(Value::Long(self.pop_long()?)),
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

fn value_local_width(value: &Value) -> usize {
    match value {
        Value::Long(_) => 2,
        Value::Uninitialized
        | Value::Null
        | Value::Int(_)
        | Value::Float(_)
        | Value::Reference(_)
        | Value::PrintStream => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::heap::Heap;

    #[test]
    fn garbage_collection_keeps_frame_local_and_stack_references() {
        let mut heap = Heap::new();
        let local = heap.allocate_instance("example/Local");
        let stack = heap.allocate_instance("example/Stack");
        let dropped = heap.allocate_instance("example/Dropped");
        let mut frame = Frame::new(1);
        frame.locals[0] = Value::Reference(local);
        frame.stack.push(Value::Reference(stack));

        heap.collect(frame.roots());

        assert_eq!(heap.type_name(local).unwrap(), "example.Local");
        assert_eq!(heap.type_name(stack).unwrap(), "example.Stack");
        assert!(heap.type_name(dropped).is_err());
    }

    #[test]
    fn reference_type_errors_still_name_expected_reference_values() {
        let mut frame = Frame::new(0);
        frame.stack.push(Value::Int(42));

        let error = frame
            .pop_value_of_type(&ValueType::Reference("java/lang/String".to_string()))
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expected reference on stack, found Int(42)")
        );
    }

    fn stack_of(values: &[Value]) -> Frame {
        let mut frame = Frame::new(0);
        frame.stack.extend_from_slice(values);
        frame
    }

    #[test]
    fn dup_x2_inserts_under_two_ints_or_one_long() {
        let mut frame = stack_of(&[Value::Int(1), Value::Int(2), Value::Int(3)]);
        frame.duplicate_top_insert_three_down().unwrap();
        assert_eq!(
            frame.stack,
            [Value::Int(3), Value::Int(1), Value::Int(2), Value::Int(3)]
        );

        let mut frame = stack_of(&[Value::Long(1), Value::Int(3)]);
        frame.duplicate_top_insert_three_down().unwrap();
        assert_eq!(frame.stack, [Value::Int(3), Value::Long(1), Value::Int(3)]);
    }

    #[test]
    fn dup2_copies_two_ints_or_one_long() {
        let mut frame = stack_of(&[Value::Int(1), Value::Int(2)]);
        frame.duplicate_top_two().unwrap();
        assert_eq!(
            frame.stack,
            [Value::Int(1), Value::Int(2), Value::Int(1), Value::Int(2)]
        );

        let mut frame = stack_of(&[Value::Long(7)]);
        frame.duplicate_top_two().unwrap();
        assert_eq!(frame.stack, [Value::Long(7), Value::Long(7)]);
    }

    #[test]
    fn dup2_x1_inserts_two_words_under_the_third() {
        let mut frame = stack_of(&[Value::Int(1), Value::Int(2), Value::Int(3)]);
        frame.duplicate_top_two_insert_three_down().unwrap();
        assert_eq!(
            frame.stack,
            [
                Value::Int(2),
                Value::Int(3),
                Value::Int(1),
                Value::Int(2),
                Value::Int(3)
            ]
        );

        let mut frame = stack_of(&[Value::Int(1), Value::Long(2)]);
        frame.duplicate_top_two_insert_three_down().unwrap();
        assert_eq!(frame.stack, [Value::Long(2), Value::Int(1), Value::Long(2)]);
    }

    #[test]
    fn swap_exchanges_two_ints_and_rejects_longs() {
        let mut frame = stack_of(&[Value::Int(1), Value::Int(2)]);
        frame.swap_top_two().unwrap();
        assert_eq!(frame.stack, [Value::Int(2), Value::Int(1)]);

        let mut frame = stack_of(&[Value::Long(1), Value::Int(2)]);
        assert!(frame.swap_top_two().is_err());
    }

    #[test]
    fn float_locals_round_trip_through_stack() {
        let mut frame = Frame::new(1);
        frame.stack.push(Value::Float(0.75));

        frame.store_float_local(0).unwrap();
        frame.load_float_local(0).unwrap();

        assert_eq!(frame.pop_float().unwrap(), 0.75);
    }
}
