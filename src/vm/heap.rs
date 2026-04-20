//! Heap allocation, instance field storage, and mark-sweep garbage collection.

use std::collections::HashMap;

use super::descriptors::ValueType;
use super::value::Value;
use crate::{JayError, JayResult};

const DEFAULT_GC_THRESHOLD: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ObjectRef(usize);

#[derive(Debug)]
pub(super) struct Heap {
    objects: Vec<Option<HeapObject>>,
    free_slots: Vec<usize>,
    allocations_since_gc: usize,
    gc_threshold: usize,
}

#[derive(Debug)]
struct HeapObject {
    marked: bool,
    kind: ObjectKind,
}

#[derive(Debug)]
enum ObjectKind {
    String(String),
    Instance {
        class_name: String,
        fields: HashMap<FieldKey, Value>,
    },
    ObjectArray {
        elements: Vec<Option<Value>>,
    },
}

/// Identifies a field exactly as it appears in a class constant pool.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct FieldKey {
    class_name: String,
    name: String,
    descriptor: String,
}

impl FieldKey {
    pub(super) fn new(
        class_name: impl Into<String>,
        name: impl Into<String>,
        descriptor: impl Into<String>,
    ) -> Self {
        Self {
            class_name: class_name.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }
}

impl Heap {
    pub(super) fn new() -> Self {
        Self {
            objects: Vec::new(),
            free_slots: Vec::new(),
            allocations_since_gc: 0,
            gc_threshold: DEFAULT_GC_THRESHOLD,
        }
    }

    pub(super) fn allocate_string(&mut self, value: impl Into<String>) -> ObjectRef {
        self.allocate(ObjectKind::String(value.into()))
    }

    pub(super) fn allocate_instance(&mut self, class_name: impl Into<String>) -> ObjectRef {
        self.allocate(ObjectKind::Instance {
            class_name: class_name.into(),
            fields: HashMap::new(),
        })
    }

    pub(super) fn allocate_object_array(&mut self, length: usize) -> ObjectRef {
        self.allocate(ObjectKind::ObjectArray {
            elements: vec![None; length],
        })
    }

    fn allocate(&mut self, kind: ObjectKind) -> ObjectRef {
        let object = HeapObject {
            marked: false,
            kind,
        };
        self.allocations_since_gc += 1;
        if let Some(index) = self.free_slots.pop() {
            self.objects[index] = Some(object);
            return ObjectRef(index);
        }

        let reference = ObjectRef(self.objects.len());
        self.objects.push(Some(object));
        reference
    }

    pub(super) fn string(&self, reference: ObjectRef) -> JayResult<&str> {
        match self.object(reference)?.kind {
            ObjectKind::String(ref value) => Ok(value),
            ObjectKind::Instance { ref class_name, .. } => Err(JayError::new(format!(
                "expected String reference, found {}",
                class_name.replace('/', ".")
            ))),
            ObjectKind::ObjectArray { .. } => Err(JayError::new(
                "expected String reference, found java.lang.Object[]",
            )),
        }
    }

    pub(super) fn value_type(&self, reference: ObjectRef) -> JayResult<Option<ValueType>> {
        match self.object(reference)?.kind {
            ObjectKind::String(_) => Ok(Some(ValueType::Reference("java/lang/String".to_string()))),
            ObjectKind::Instance { ref class_name, .. } => {
                Ok(Some(ValueType::Reference(class_name.clone())))
            }
            ObjectKind::ObjectArray { .. } => Ok(Some(ValueType::Reference(
                "[Ljava/lang/Object;".to_string(),
            ))),
        }
    }

    pub(super) fn type_name(&self, reference: ObjectRef) -> JayResult<String> {
        match self.object(reference)?.kind {
            ObjectKind::String(_) => Ok("String".to_string()),
            ObjectKind::Instance { ref class_name, .. } => Ok(class_name.replace('/', ".")),
            ObjectKind::ObjectArray { .. } => Ok("java.lang.Object[]".to_string()),
        }
    }

    pub(super) fn instance_class_name(&self, reference: ObjectRef) -> JayResult<&str> {
        match self.object(reference)?.kind {
            ObjectKind::Instance { ref class_name, .. } => Ok(class_name),
            ObjectKind::String(_) => {
                Err(JayError::new("expected instance reference, found String"))
            }
            ObjectKind::ObjectArray { .. } => Err(JayError::new(
                "expected instance reference, found java.lang.Object[]",
            )),
        }
    }

    pub(super) fn put_instance_field(
        &mut self,
        reference: ObjectRef,
        field: FieldKey,
        value: Value,
    ) -> JayResult<()> {
        match self.object_mut(reference)?.kind {
            ObjectKind::Instance { ref mut fields, .. } => {
                fields.insert(field, value);
                Ok(())
            }
            ObjectKind::String(_) => Err(JayError::new(
                "expected instance reference for putfield, found String",
            )),
            ObjectKind::ObjectArray { .. } => Err(JayError::new(
                "expected instance reference for putfield, found java.lang.Object[]",
            )),
        }
    }

    pub(super) fn get_instance_field(
        &self,
        reference: ObjectRef,
        field: &FieldKey,
    ) -> JayResult<Option<Value>> {
        match self.object(reference)?.kind {
            ObjectKind::Instance { ref fields, .. } => Ok(fields.get(field).cloned()),
            ObjectKind::String(_) => Err(JayError::new(
                "expected instance reference for getfield, found String",
            )),
            ObjectKind::ObjectArray { .. } => Err(JayError::new(
                "expected instance reference for getfield, found java.lang.Object[]",
            )),
        }
    }

    pub(super) fn array_length(&self, reference: ObjectRef) -> JayResult<usize> {
        match self.object(reference)?.kind {
            ObjectKind::ObjectArray { ref elements } => Ok(elements.len()),
            _ => Err(JayError::new(format!(
                "expected object array reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    pub(super) fn load_array_reference(
        &self,
        reference: ObjectRef,
        index: usize,
    ) -> JayResult<Value> {
        match self.object(reference)?.kind {
            ObjectKind::ObjectArray { ref elements } => {
                let Some(value) = elements.get(index) else {
                    return Err(JayError::new(format!(
                        "array index {index} out of bounds for length {}",
                        elements.len()
                    )));
                };
                value
                    .clone()
                    .ok_or_else(|| JayError::new("null array elements are unsupported"))
            }
            _ => Err(JayError::new(format!(
                "expected object array reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    pub(super) fn store_array_reference(
        &mut self,
        reference: ObjectRef,
        index: usize,
        value: Value,
    ) -> JayResult<()> {
        if !matches!(value, Value::Reference(_)) {
            return Err(JayError::new(format!(
                "expected reference for object array store, found {}",
                value.type_name(self)?
            )));
        }

        match self.object_mut(reference)?.kind {
            ObjectKind::ObjectArray { ref mut elements } => {
                let length = elements.len();
                let Some(slot) = elements.get_mut(index) else {
                    return Err(JayError::new(format!(
                        "array index {index} out of bounds for length {length}"
                    )));
                };
                *slot = Some(value);
                Ok(())
            }
            _ => Err(JayError::new(format!(
                "expected object array reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    pub(super) fn should_collect(&self) -> bool {
        self.allocations_since_gc >= self.gc_threshold
    }

    pub(super) fn collect<'a, I>(&mut self, roots: I)
    where
        I: IntoIterator<Item = &'a Value>,
    {
        for root in roots {
            if let Some(reference) = root.object_ref() {
                self.mark(reference);
            }
        }

        self.sweep();
        self.allocations_since_gc = 0;
    }

    fn object(&self, reference: ObjectRef) -> JayResult<&HeapObject> {
        self.objects
            .get(reference.0)
            .and_then(Option::as_ref)
            .ok_or_else(|| JayError::new(format!("invalid heap reference #{}", reference.0)))
    }

    fn object_mut(&mut self, reference: ObjectRef) -> JayResult<&mut HeapObject> {
        self.objects
            .get_mut(reference.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| JayError::new(format!("invalid heap reference #{}", reference.0)))
    }

    fn mark(&mut self, reference: ObjectRef) {
        let field_references = {
            let Some(Some(object)) = self.objects.get_mut(reference.0) else {
                return;
            };

            if object.marked {
                return;
            }

            object.marked = true;
            match object.kind {
                ObjectKind::String(_) => Vec::new(),
                ObjectKind::Instance { ref fields, .. } => {
                    fields.values().filter_map(Value::object_ref).collect()
                }
                ObjectKind::ObjectArray { ref elements } => elements
                    .iter()
                    .filter_map(|element| element.as_ref())
                    .filter_map(Value::object_ref)
                    .collect(),
            }
        };

        for field_reference in field_references {
            self.mark(field_reference);
        }
    }

    fn sweep(&mut self) {
        for (index, object) in self.objects.iter_mut().enumerate() {
            let Some(heap_object) = object else {
                continue;
            };

            if heap_object.marked {
                heap_object.marked = false;
            } else {
                *object = None;
                self.free_slots.push(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_allocates_and_resolves_string_objects() {
        let mut heap = Heap::new();

        let reference = heap.allocate_string("hello");

        assert_eq!(heap.string(reference).unwrap(), "hello");
    }

    #[test]
    fn heap_distinguishes_instance_objects_from_strings() {
        let mut heap = Heap::new();

        let reference = heap.allocate_instance("example/Empty");

        assert_eq!(
            heap.value_type(reference).unwrap(),
            Some(ValueType::Reference("example/Empty".to_string()))
        );
        assert_eq!(heap.type_name(reference).unwrap(), "example.Empty");
        assert!(
            heap.string(reference)
                .unwrap_err()
                .to_string()
                .contains("expected String reference, found example.Empty")
        );
    }

    #[test]
    fn heap_stores_instance_fields_by_owner_name_and_descriptor() {
        let mut heap = Heap::new();
        let instance = heap.allocate_instance("example/Car");
        let year = FieldKey::new("example/Car", "year", "I");
        let make = FieldKey::new("example/Car", "make", "Ljava/lang/String;");
        let make_value = heap.allocate_string("Toyota");

        heap.put_instance_field(instance, year.clone(), Value::Int(2020))
            .unwrap();
        heap.put_instance_field(instance, make.clone(), Value::Reference(make_value))
            .unwrap();

        assert_eq!(
            heap.get_instance_field(instance, &year).unwrap(),
            Some(Value::Int(2020))
        );
        assert_eq!(
            heap.get_instance_field(instance, &make).unwrap(),
            Some(Value::Reference(make_value))
        );
    }

    #[test]
    fn heap_object_arrays_store_length_and_references() {
        let mut heap = Heap::new();
        let array = heap.allocate_object_array(2);
        let first = heap.allocate_string("first");
        let second = heap.allocate_string("second");

        heap.store_array_reference(array, 0, Value::Reference(first))
            .unwrap();
        heap.store_array_reference(array, 1, Value::Reference(second))
            .unwrap();

        assert_eq!(heap.array_length(array).unwrap(), 2);
        assert_eq!(
            heap.load_array_reference(array, 0).unwrap(),
            Value::Reference(first)
        );
        assert_eq!(
            heap.load_array_reference(array, 1).unwrap(),
            Value::Reference(second)
        );
    }

    #[test]
    fn heap_reports_unset_array_reference_slots_as_unsupported_nulls() {
        let mut heap = Heap::new();
        let array = heap.allocate_object_array(1);

        let error = heap.load_array_reference(array, 0).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("null array elements are unsupported")
        );
    }

    #[test]
    fn heap_reports_instance_class_name() {
        let mut heap = Heap::new();
        let instance = heap.allocate_instance("example/Car");

        assert_eq!(heap.instance_class_name(instance).unwrap(), "example/Car");
    }

    #[test]
    fn heap_rejects_field_writes_to_non_instance_references() {
        let mut heap = Heap::new();
        let string = heap.allocate_string("not an instance");
        let field = FieldKey::new("example/Car", "year", "I");

        let error = heap
            .put_instance_field(string, field, Value::Int(2020))
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expected instance reference for putfield")
        );
    }

    #[test]
    fn heap_rejects_field_reads_from_non_instance_references() {
        let mut heap = Heap::new();
        let string = heap.allocate_string("not an instance");
        let field = FieldKey::new("example/Car", "year", "I");

        let error = heap.get_instance_field(string, &field).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expected instance reference for getfield")
        );
    }

    #[test]
    fn garbage_collection_drops_unrooted_strings() {
        let mut heap = Heap::new();
        let dropped = heap.allocate_string("drop me");
        let kept = heap.allocate_string("keep me");

        let roots = [Value::Reference(kept)];
        heap.collect(roots.iter());

        assert!(heap.string(dropped).is_err());
        assert_eq!(heap.string(kept).unwrap(), "keep me");
    }

    #[test]
    fn garbage_collection_keeps_references_stored_in_reachable_instance_fields() {
        let mut heap = Heap::new();
        let instance = heap.allocate_instance("example/Car");
        let kept = heap.allocate_string("keep me");
        let dropped = heap.allocate_string("drop me");
        let field = FieldKey::new("example/Car", "make", "Ljava/lang/String;");
        heap.put_instance_field(instance, field, Value::Reference(kept))
            .unwrap();

        let roots = [Value::Reference(instance)];
        heap.collect(roots.iter());

        assert_eq!(heap.string(kept).unwrap(), "keep me");
        assert!(heap.string(dropped).is_err());
    }

    #[test]
    fn garbage_collection_marks_instance_fields_recursively() {
        let mut heap = Heap::new();
        let root = heap.allocate_instance("example/Root");
        let child = heap.allocate_instance("example/Child");
        let kept = heap.allocate_string("nested");
        let dropped = heap.allocate_string("drop me");
        let child_field = FieldKey::new("example/Root", "child", "Lexample/Child;");
        let value_field = FieldKey::new("example/Child", "value", "Ljava/lang/String;");
        heap.put_instance_field(root, child_field, Value::Reference(child))
            .unwrap();
        heap.put_instance_field(child, value_field, Value::Reference(kept))
            .unwrap();

        let roots = [Value::Reference(root)];
        heap.collect(roots.iter());

        assert_eq!(heap.string(kept).unwrap(), "nested");
        assert!(heap.string(dropped).is_err());
    }

    #[test]
    fn garbage_collection_reuses_freed_slots_without_moving_live_references() {
        let mut heap = Heap::new();
        let live = heap.allocate_string("live");
        let dead = heap.allocate_string("dead");

        let roots = [Value::Reference(live)];
        heap.collect(roots.iter());
        let reused = heap.allocate_string("reused");

        assert_eq!(heap.string(live).unwrap(), "live");
        assert_eq!(reused, dead);
        assert_eq!(heap.string(reused).unwrap(), "reused");
    }

    #[test]
    fn heap_requests_collection_at_default_threshold_and_resets_after_collecting() {
        let mut heap = Heap::new();

        for index in 0..DEFAULT_GC_THRESHOLD - 1 {
            heap.allocate_string(format!("value {index}"));
            assert!(!heap.should_collect());
        }

        heap.allocate_string("threshold");
        assert!(heap.should_collect());

        heap.collect(std::iter::empty::<&Value>());
        assert!(!heap.should_collect());
    }
}
