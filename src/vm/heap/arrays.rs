//! Whole-array heap operations backing `System.arraycopy` and `Object.clone`.

use super::{Heap, ObjectKind, ObjectRef, PrimitiveArray, PrimitiveElement};
use crate::{JayError, JayResult};

/// The runtime shape of an array object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vm) enum ArrayKind {
    Primitive(PrimitiveElement),
    /// A reference array, carrying its full descriptor such as `[Ljava/lang/String;`.
    Reference(String),
}

impl Heap {
    /// Reports how `reference` is laid out, or `None` when it is not an array.
    pub(in crate::vm) fn array_kind(&self, reference: ObjectRef) -> JayResult<Option<ArrayKind>> {
        Ok(match self.object(reference)?.kind {
            ObjectKind::PrimitiveArray(ref array) => Some(ArrayKind::Primitive(array.element())),
            ObjectKind::ObjectArray { ref descriptor, .. } => {
                Some(ArrayKind::Reference(descriptor.clone()))
            }
            _ => None,
        })
    }

    /// Copies `length` elements between two primitive arrays of the same
    /// element kind. Overlapping ranges within one array behave as if the
    /// source were copied to a temporary first. Bounds must already be checked.
    pub(in crate::vm) fn copy_primitive_array(
        &mut self,
        source: ObjectRef,
        source_pos: usize,
        destination: ObjectRef,
        destination_pos: usize,
        length: usize,
    ) -> JayResult<()> {
        let snapshot = match self.object(source)?.kind {
            ObjectKind::PrimitiveArray(ref array) => array.slice(source_pos, length)?,
            _ => {
                return Err(JayError::new(format!(
                    "expected primitive array reference, found {}",
                    self.type_name(source)?
                )));
            }
        };
        let type_name = self.type_name(destination)?;
        match self.object_mut(destination)?.kind {
            ObjectKind::PrimitiveArray(ref mut array) => array.splice(destination_pos, &snapshot),
            _ => Err(JayError::new(format!(
                "expected primitive array reference, found {type_name}"
            ))),
        }
    }

    /// Allocates a shallow copy of an array (`Object.clone()` on arrays).
    pub(in crate::vm) fn clone_array(&mut self, reference: ObjectRef) -> JayResult<ObjectRef> {
        let kind = match self.object(reference)?.kind {
            ObjectKind::PrimitiveArray(ref array) => ObjectKind::PrimitiveArray(array.clone()),
            ObjectKind::ObjectArray {
                ref descriptor,
                ref elements,
            } => ObjectKind::ObjectArray {
                descriptor: descriptor.clone(),
                elements: elements.clone(),
            },
            _ => {
                return Err(JayError::new(format!(
                    "expected array reference, found {}",
                    self.type_name(reference)?
                )));
            }
        };
        Ok(self.allocate(kind))
    }

    /// Allocates a shallow copy of an instance (`Object.clone()` on objects):
    /// the new object has the same class and field values.
    pub(in crate::vm) fn clone_instance(&mut self, reference: ObjectRef) -> JayResult<ObjectRef> {
        let kind = match self.object(reference)?.kind {
            ObjectKind::Instance {
                ref class_name,
                ref fields,
            } => ObjectKind::Instance {
                class_name: class_name.clone(),
                fields: fields.clone(),
            },
            _ => {
                return Err(JayError::new(format!(
                    "expected instance reference, found {}",
                    self.type_name(reference)?
                )));
            }
        };
        Ok(self.allocate(kind))
    }
}

impl PrimitiveArray {
    /// Copies `length` elements starting at `start` into a new array of the same kind.
    fn slice(&self, start: usize, length: usize) -> JayResult<PrimitiveArray> {
        fn take<T: Clone>(values: &[T], start: usize, length: usize) -> JayResult<Vec<T>> {
            values
                .get(start..start + length)
                .map(<[T]>::to_vec)
                .ok_or_else(|| JayError::new("primitive array copy source range out of bounds"))
        }
        Ok(match self {
            Self::Boolean(values) => Self::Boolean(take(values, start, length)?),
            Self::Char(values) => Self::Char(take(values, start, length)?),
            Self::Float(values) => Self::Float(take(values, start, length)?),
            Self::Byte(values) => Self::Byte(take(values, start, length)?),
            Self::Short(values) => Self::Short(take(values, start, length)?),
            Self::Int(values) => Self::Int(take(values, start, length)?),
            Self::Long(values) => Self::Long(take(values, start, length)?),
        })
    }

    /// Overwrites elements starting at `start` with `values`, which must be
    /// the same kind of array.
    fn splice(&mut self, start: usize, values: &PrimitiveArray) -> JayResult<()> {
        fn put<T: Copy>(target: &mut [T], start: usize, values: &[T]) -> JayResult<()> {
            target
                .get_mut(start..start + values.len())
                .map(|slots| slots.copy_from_slice(values))
                .ok_or_else(|| {
                    JayError::new("primitive array copy destination range out of bounds")
                })
        }
        match (self, values) {
            (Self::Boolean(target), Self::Boolean(values)) => put(target, start, values),
            (Self::Char(target), Self::Char(values)) => put(target, start, values),
            (Self::Float(target), Self::Float(values)) => put(target, start, values),
            (Self::Byte(target), Self::Byte(values)) => put(target, start, values),
            (Self::Short(target), Self::Short(values)) => put(target, start, values),
            (Self::Int(target), Self::Int(values)) => put(target, start, values),
            (Self::Long(target), Self::Long(values)) => put(target, start, values),
            (target, values) => Err(JayError::new(format!(
                "cannot copy {} into {}",
                values.element().array_name(),
                target.element().array_name()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::heap::FieldKey;
    use crate::vm::value::Value;

    fn int_array(heap: &mut Heap, values: &[i32]) -> ObjectRef {
        let array = heap.allocate_primitive_array(PrimitiveElement::Int, values.len());
        for (index, value) in values.iter().enumerate() {
            heap.store_primitive(array, index, Value::Int(*value))
                .unwrap();
        }
        array
    }

    fn ints(heap: &Heap, array: ObjectRef) -> Vec<i32> {
        (0..heap.array_length(array).unwrap())
            .map(|index| match heap.load_primitive(array, index).unwrap() {
                Value::Int(value) => value,
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    #[test]
    fn primitive_copies_move_ranges_and_handle_overlap() {
        let mut heap = Heap::new();
        let source = int_array(&mut heap, &[1, 2, 3, 4, 5]);
        let target = int_array(&mut heap, &[0, 0, 0, 0, 0]);

        heap.copy_primitive_array(source, 1, target, 2, 3).unwrap();
        assert_eq!(ints(&heap, target), vec![0, 0, 2, 3, 4]);

        // Overlapping copy within one array behaves like a temporary copy.
        heap.copy_primitive_array(source, 0, source, 1, 4).unwrap();
        assert_eq!(ints(&heap, source), vec![1, 1, 2, 3, 4]);
    }

    #[test]
    fn primitive_copies_reject_mismatched_kinds() {
        let mut heap = Heap::new();
        let source = int_array(&mut heap, &[1]);
        let target = heap.allocate_primitive_array(PrimitiveElement::Byte, 1);
        assert!(
            heap.copy_primitive_array(source, 0, target, 0, 1)
                .unwrap_err()
                .to_string()
                .contains("cannot copy int[] into byte[]")
        );
    }

    #[test]
    fn array_kinds_distinguish_primitive_reference_and_instances() {
        let mut heap = Heap::new();
        let ints = heap.allocate_primitive_array(PrimitiveElement::Int, 0);
        let strings = heap.allocate_reference_array("[Ljava/lang/String;", 0);
        let instance = heap.allocate_instance("example/Empty");
        assert_eq!(
            heap.array_kind(ints).unwrap(),
            Some(ArrayKind::Primitive(PrimitiveElement::Int))
        );
        assert_eq!(
            heap.array_kind(strings).unwrap(),
            Some(ArrayKind::Reference("[Ljava/lang/String;".to_string()))
        );
        assert_eq!(heap.array_kind(instance).unwrap(), None);
    }

    #[test]
    fn clones_copy_arrays_and_instances_shallowly() {
        let mut heap = Heap::new();
        let source = int_array(&mut heap, &[7, 8]);
        let copy = heap.clone_array(source).unwrap();
        assert_ne!(copy, source);
        assert_eq!(
            heap.array_kind(copy).unwrap(),
            Some(ArrayKind::Primitive(PrimitiveElement::Int))
        );
        assert_eq!(ints(&heap, copy), vec![7, 8]);
        heap.store_primitive(copy, 0, Value::Int(9)).unwrap();
        assert_eq!(ints(&heap, source), vec![7, 8]);

        let element = heap.allocate_instance("example/Item");
        let references = heap.allocate_reference_array("[Ljava/lang/Object;", 1);
        heap.store_array_reference(references, 0, Value::Reference(element))
            .unwrap();
        let references_copy = heap.clone_array(references).unwrap();
        assert_eq!(
            heap.load_array_reference(references_copy, 0).unwrap(),
            Value::Reference(element)
        );

        let field = FieldKey::new("example/Item", "count", "I");
        heap.put_instance_field(element, field.clone(), Value::Int(3))
            .unwrap();
        let instance_copy = heap.clone_instance(element).unwrap();
        assert_eq!(
            heap.get_instance_field(instance_copy, &field).unwrap(),
            Some(Value::Int(3))
        );
        assert!(heap.clone_array(element).is_err());
        assert!(heap.clone_instance(source).is_err());
    }
}
