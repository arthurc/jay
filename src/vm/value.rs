//! Runtime operand values shared by frames, heap objects, and call returns.

use super::descriptors::ValueType;
use super::heap::{Heap, ObjectRef};
use crate::JayResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    Uninitialized,
    Int(i32),
    Reference(ObjectRef),
    PrintStream,
}

impl Value {
    pub(super) fn value_type(&self, heap: &Heap) -> JayResult<Option<ValueType>> {
        match self {
            Value::Int(_) => Ok(Some(ValueType::Int)),
            Value::Reference(reference) => heap.value_type(*reference),
            Value::Uninitialized | Value::PrintStream => Ok(None),
        }
    }

    pub(super) fn type_name(&self, heap: &Heap) -> JayResult<String> {
        match self {
            Value::Uninitialized => Ok("uninitialized".to_string()),
            Value::Int(_) => Ok("int".to_string()),
            Value::Reference(reference) => heap.type_name(*reference),
            Value::PrintStream => Ok("PrintStream".to_string()),
        }
    }

    pub(super) fn object_ref(&self) -> Option<ObjectRef> {
        match self {
            Value::Reference(reference) => Some(*reference),
            Value::Uninitialized | Value::Int(_) | Value::PrintStream => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_value_type_is_carried_by_references() {
        let mut heap = Heap::new();
        let reference = heap.allocate_string("value");

        assert_eq!(
            Value::Reference(reference).value_type(&heap).unwrap(),
            Some(ValueType::String)
        );
    }
}
