//! Heap allocation, instance field storage, and mark-sweep garbage collection.

use std::collections::HashMap;

use super::descriptors::ValueType;
use super::value::Value;
use crate::{JayError, JayResult};

mod arrays;

pub(super) use arrays::ArrayKind;

const DEFAULT_GC_THRESHOLD: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ObjectRef(usize);

impl ObjectRef {
    /// The heap slot index, used to carry a reference through `JayError`.
    pub(super) fn index(self) -> usize {
        self.0
    }

    /// Rebuilds a reference from a slot index previously obtained from `index`.
    pub(super) fn from_index(index: usize) -> Self {
        Self(index)
    }
}

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
        descriptor: String,
        elements: Vec<Value>,
    },
    PrimitiveArray(PrimitiveArray),
    /// A `java.lang.StringBuilder` backed by a native buffer instead of the JDK's `byte[]`.
    StringBuilder(String),
}

/// Element type of a primitive array, as encoded by the `newarray` `atype` operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrimitiveElement {
    Boolean,
    Char,
    Float,
    Byte,
    Short,
    Int,
    Long,
}

impl PrimitiveElement {
    /// Decodes the `newarray` operand. `double` (7) is rejected because the VM has no double values.
    pub(super) fn from_atype(atype: u8) -> JayResult<Self> {
        match atype {
            4 => Ok(Self::Boolean),
            5 => Ok(Self::Char),
            6 => Ok(Self::Float),
            7 => Err(JayError::new("unsupported newarray element type double")),
            8 => Ok(Self::Byte),
            9 => Ok(Self::Short),
            10 => Ok(Self::Int),
            11 => Ok(Self::Long),
            other => Err(JayError::new(format!(
                "invalid newarray element type {other}"
            ))),
        }
    }

    /// The array descriptor for this element type, such as `[I`.
    /// Decodes a primitive field descriptor such as `I`. `double` (`D`) is rejected
    /// because the VM has no double values.
    pub(super) fn from_descriptor(descriptor: &str) -> JayResult<Self> {
        match descriptor {
            "Z" => Ok(Self::Boolean),
            "C" => Ok(Self::Char),
            "F" => Ok(Self::Float),
            "D" => Err(JayError::new("unsupported array element type double")),
            "B" => Ok(Self::Byte),
            "S" => Ok(Self::Short),
            "I" => Ok(Self::Int),
            "J" => Ok(Self::Long),
            other => Err(JayError::new(format!(
                "invalid primitive array element type {other}"
            ))),
        }
    }

    pub(super) fn array_descriptor(self) -> &'static str {
        match self {
            Self::Boolean => "[Z",
            Self::Char => "[C",
            Self::Float => "[F",
            Self::Byte => "[B",
            Self::Short => "[S",
            Self::Int => "[I",
            Self::Long => "[J",
        }
    }

    /// The Java source spelling of the array type, such as `int[]`.
    fn array_name(self) -> &'static str {
        match self {
            Self::Boolean => "boolean[]",
            Self::Char => "char[]",
            Self::Float => "float[]",
            Self::Byte => "byte[]",
            Self::Short => "short[]",
            Self::Int => "int[]",
            Self::Long => "long[]",
        }
    }
}

/// Storage for a primitive array. Elements are kept at their natural width so
/// stores narrow and loads widen exactly as the JVM specifies.
#[derive(Debug, Clone)]
pub(super) enum PrimitiveArray {
    Boolean(Vec<i8>),
    Char(Vec<u16>),
    Float(Vec<f32>),
    Byte(Vec<i8>),
    Short(Vec<i16>),
    Int(Vec<i32>),
    Long(Vec<i64>),
}

impl PrimitiveArray {
    fn new(element: PrimitiveElement, length: usize) -> Self {
        match element {
            PrimitiveElement::Boolean => Self::Boolean(vec![0; length]),
            PrimitiveElement::Char => Self::Char(vec![0; length]),
            PrimitiveElement::Float => Self::Float(vec![0.0; length]),
            PrimitiveElement::Byte => Self::Byte(vec![0; length]),
            PrimitiveElement::Short => Self::Short(vec![0; length]),
            PrimitiveElement::Int => Self::Int(vec![0; length]),
            PrimitiveElement::Long => Self::Long(vec![0; length]),
        }
    }

    fn element(&self) -> PrimitiveElement {
        match self {
            Self::Boolean(_) => PrimitiveElement::Boolean,
            Self::Char(_) => PrimitiveElement::Char,
            Self::Float(_) => PrimitiveElement::Float,
            Self::Byte(_) => PrimitiveElement::Byte,
            Self::Short(_) => PrimitiveElement::Short,
            Self::Int(_) => PrimitiveElement::Int,
            Self::Long(_) => PrimitiveElement::Long,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Boolean(values) | Self::Byte(values) => values.len(),
            Self::Char(values) => values.len(),
            Self::Float(values) => values.len(),
            Self::Short(values) => values.len(),
            Self::Int(values) => values.len(),
            Self::Long(values) => values.len(),
        }
    }

    fn load(&self, index: usize) -> Option<Value> {
        match self {
            Self::Boolean(values) | Self::Byte(values) => {
                values.get(index).map(|value| Value::Int(*value as i32))
            }
            Self::Char(values) => values.get(index).map(|value| Value::Int(*value as i32)),
            Self::Float(values) => values.get(index).map(|value| Value::Float(*value)),
            Self::Short(values) => values.get(index).map(|value| Value::Int(*value as i32)),
            Self::Int(values) => values.get(index).map(|value| Value::Int(*value)),
            Self::Long(values) => values.get(index).map(|value| Value::Long(*value)),
        }
    }

    /// Stores `value`, narrowing ints to the element width. Returns `Ok(false)`
    /// when the index is out of bounds and an error when the value kind is wrong.
    fn store(&mut self, index: usize, value: &Value) -> JayResult<bool> {
        let element = self.element();
        let wrong_kind = || {
            JayError::new(format!(
                "expected {} element for {}, found {value:?}",
                element_value_name(element),
                element.array_name()
            ))
        };
        match (self, value) {
            (Self::Boolean(values), Value::Int(int)) => Ok(set(values, index, (*int & 1) as i8)),
            (Self::Byte(values), Value::Int(int)) => Ok(set(values, index, *int as i8)),
            (Self::Char(values), Value::Int(int)) => Ok(set(values, index, *int as u16)),
            (Self::Short(values), Value::Int(int)) => Ok(set(values, index, *int as i16)),
            (Self::Int(values), Value::Int(int)) => Ok(set(values, index, *int)),
            (Self::Float(values), Value::Float(float)) => Ok(set(values, index, *float)),
            (Self::Long(values), Value::Long(long)) => Ok(set(values, index, *long)),
            _ => Err(wrong_kind()),
        }
    }
}

fn element_value_name(element: PrimitiveElement) -> &'static str {
    match element {
        PrimitiveElement::Float => "float",
        PrimitiveElement::Long => "long",
        _ => "int",
    }
}

fn set<T>(values: &mut [T], index: usize, value: T) -> bool {
    match values.get_mut(index) {
        Some(slot) => {
            *slot = value;
            true
        }
        None => false,
    }
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

    pub(super) fn allocate_reference_array(
        &mut self,
        descriptor: impl Into<String>,
        length: usize,
    ) -> ObjectRef {
        self.allocate(ObjectKind::ObjectArray {
            descriptor: descriptor.into(),
            elements: vec![Value::Null; length],
        })
    }

    pub(super) fn allocate_primitive_array(
        &mut self,
        element: PrimitiveElement,
        length: usize,
    ) -> ObjectRef {
        self.allocate(ObjectKind::PrimitiveArray(PrimitiveArray::new(
            element, length,
        )))
    }

    /// Allocates a `char[]` holding the given UTF-16 code units.
    pub(super) fn allocate_char_array(&mut self, units: &[u16]) -> ObjectRef {
        self.allocate(ObjectKind::PrimitiveArray(PrimitiveArray::Char(
            units.to_vec(),
        )))
    }

    /// Reads the UTF-16 code units of a `char[]`.
    pub(super) fn char_array_units(&self, reference: ObjectRef) -> JayResult<&[u16]> {
        match self.object(reference)?.kind {
            ObjectKind::PrimitiveArray(PrimitiveArray::Char(ref units)) => Ok(units),
            _ => Err(JayError::new(format!(
                "expected char[] reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    /// Turns a freshly allocated `java.lang.String` instance into a native string.
    ///
    /// `new` allocates an empty instance before the constructor runs; the
    /// constructor shim swaps the object kind in place so existing references
    /// to the slot see the string value.
    pub(super) fn replace_with_string(
        &mut self,
        reference: ObjectRef,
        value: impl Into<String>,
    ) -> JayResult<()> {
        let object = self.object_mut(reference)?;
        match object.kind {
            ObjectKind::Instance { ref class_name, .. } if class_name == "java/lang/String" => {
                object.kind = ObjectKind::String(value.into());
                Ok(())
            }
            _ => Err(JayError::new(format!(
                "expected uninitialized String instance, found {}",
                self.type_name(reference)?
            ))),
        }
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
            ObjectKind::ObjectArray { ref descriptor, .. } => Err(JayError::new(format!(
                "expected String reference, found {}",
                reference_array_name(descriptor)
            ))),
            ObjectKind::PrimitiveArray(ref array) => Err(JayError::new(format!(
                "expected String reference, found {}",
                array.element().array_name()
            ))),
            ObjectKind::StringBuilder(_) => Err(JayError::new(
                "expected String reference, found java.lang.StringBuilder",
            )),
        }
    }

    /// Reads the current contents of a `StringBuilder`.
    pub(super) fn string_builder(&self, reference: ObjectRef) -> JayResult<&str> {
        match self.object(reference)?.kind {
            ObjectKind::StringBuilder(ref value) => Ok(value),
            _ => Err(JayError::new(format!(
                "expected StringBuilder reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    /// Mutably borrows the buffer of a `StringBuilder`.
    pub(super) fn string_builder_mut(&mut self, reference: ObjectRef) -> JayResult<&mut String> {
        let type_name = self.type_name(reference)?;
        match self.object_mut(reference)?.kind {
            ObjectKind::StringBuilder(ref mut value) => Ok(value),
            _ => Err(JayError::new(format!(
                "expected StringBuilder reference, found {type_name}"
            ))),
        }
    }

    /// Turns a freshly allocated `java.lang.StringBuilder` instance into a native buffer.
    pub(super) fn replace_with_string_builder(
        &mut self,
        reference: ObjectRef,
        value: impl Into<String>,
    ) -> JayResult<()> {
        let object = self.object_mut(reference)?;
        match object.kind {
            ObjectKind::Instance { ref class_name, .. }
                if class_name == "java/lang/StringBuilder" =>
            {
                object.kind = ObjectKind::StringBuilder(value.into());
                Ok(())
            }
            _ => Err(JayError::new(format!(
                "expected uninitialized StringBuilder instance, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    pub(super) fn value_type(&self, reference: ObjectRef) -> JayResult<Option<ValueType>> {
        match self.object(reference)?.kind {
            ObjectKind::String(_) => Ok(Some(ValueType::Reference("java/lang/String".to_string()))),
            ObjectKind::Instance { ref class_name, .. } => {
                Ok(Some(ValueType::Reference(class_name.clone())))
            }
            ObjectKind::ObjectArray { ref descriptor, .. } => {
                Ok(Some(ValueType::Reference(descriptor.clone())))
            }
            ObjectKind::PrimitiveArray(ref array) => Ok(Some(ValueType::Reference(
                array.element().array_descriptor().to_string(),
            ))),
            ObjectKind::StringBuilder(_) => Ok(Some(ValueType::Reference(
                "java/lang/StringBuilder".to_string(),
            ))),
        }
    }

    pub(super) fn type_name(&self, reference: ObjectRef) -> JayResult<String> {
        match self.object(reference)?.kind {
            ObjectKind::String(_) => Ok("String".to_string()),
            ObjectKind::Instance { ref class_name, .. } => Ok(class_name.replace('/', ".")),
            ObjectKind::ObjectArray { ref descriptor, .. } => Ok(reference_array_name(descriptor)),
            ObjectKind::PrimitiveArray(ref array) => Ok(array.element().array_name().to_string()),
            ObjectKind::StringBuilder(_) => Ok("java.lang.StringBuilder".to_string()),
        }
    }

    pub(super) fn instance_class_name(&self, reference: ObjectRef) -> JayResult<&str> {
        match self.object(reference)?.kind {
            ObjectKind::Instance { ref class_name, .. } => Ok(class_name),
            ObjectKind::String(_) => {
                Err(JayError::new("expected instance reference, found String"))
            }
            ObjectKind::ObjectArray { ref descriptor, .. } => Err(JayError::new(format!(
                "expected instance reference, found {}",
                reference_array_name(descriptor)
            ))),
            ObjectKind::PrimitiveArray(ref array) => Err(JayError::new(format!(
                "expected instance reference, found {}",
                array.element().array_name()
            ))),
            ObjectKind::StringBuilder(_) => Ok("java/lang/StringBuilder"),
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
            ObjectKind::ObjectArray { ref descriptor, .. } => Err(JayError::new(format!(
                "expected instance reference for putfield, found {}",
                reference_array_name(descriptor)
            ))),
            ObjectKind::PrimitiveArray(ref array) => Err(JayError::new(format!(
                "expected instance reference for putfield, found {}",
                array.element().array_name()
            ))),
            ObjectKind::StringBuilder(_) => Err(JayError::new(
                "expected instance reference for putfield, found java.lang.StringBuilder",
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
            ObjectKind::ObjectArray { ref descriptor, .. } => Err(JayError::new(format!(
                "expected instance reference for getfield, found {}",
                reference_array_name(descriptor)
            ))),
            ObjectKind::PrimitiveArray(ref array) => Err(JayError::new(format!(
                "expected instance reference for getfield, found {}",
                array.element().array_name()
            ))),
            ObjectKind::StringBuilder(_) => Err(JayError::new(
                "expected instance reference for getfield, found java.lang.StringBuilder",
            )),
        }
    }

    pub(super) fn array_length(&self, reference: ObjectRef) -> JayResult<usize> {
        match self.object(reference)?.kind {
            ObjectKind::ObjectArray { ref elements, .. } => Ok(elements.len()),
            ObjectKind::PrimitiveArray(ref array) => Ok(array.len()),
            _ => Err(JayError::new(format!(
                "expected array reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    /// Loads one element of a primitive array, widened to `Int`, `Long`, or `Float`.
    pub(super) fn load_primitive(&self, reference: ObjectRef, index: usize) -> JayResult<Value> {
        match self.object(reference)?.kind {
            ObjectKind::PrimitiveArray(ref array) => array
                .load(index)
                .ok_or_else(|| array_index_fault(index, array.len())),
            _ => Err(JayError::new(format!(
                "expected primitive array reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    /// Stores one element into a primitive array, narrowing to the element width.
    pub(super) fn store_primitive(
        &mut self,
        reference: ObjectRef,
        index: usize,
        value: Value,
    ) -> JayResult<()> {
        let type_name = self.type_name(reference)?;
        match self.object_mut(reference)?.kind {
            ObjectKind::PrimitiveArray(ref mut array) => {
                if array.store(index, &value)? {
                    Ok(())
                } else {
                    Err(array_index_fault(index, array.len()))
                }
            }
            _ => Err(JayError::new(format!(
                "expected primitive array reference, found {type_name}"
            ))),
        }
    }

    pub(super) fn load_array_reference(
        &self,
        reference: ObjectRef,
        index: usize,
    ) -> JayResult<Value> {
        match self.object(reference)?.kind {
            ObjectKind::ObjectArray { ref elements, .. } => {
                let Some(value) = elements.get(index) else {
                    return Err(array_index_fault(index, elements.len()));
                };
                Ok(value.clone())
            }
            _ => Err(JayError::new(format!(
                "expected object array reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    pub(super) fn array_descriptor(&self, reference: ObjectRef) -> JayResult<&str> {
        match self.object(reference)?.kind {
            ObjectKind::ObjectArray { ref descriptor, .. } => Ok(descriptor),
            ObjectKind::PrimitiveArray(ref array) => Ok(array.element().array_descriptor()),
            _ => Err(JayError::new(format!(
                "expected object array reference, found {}",
                self.type_name(reference)?
            ))),
        }
    }

    pub(super) fn object_identity(&self, reference: ObjectRef) -> JayResult<usize> {
        self.object(reference)?;
        Ok(reference.0)
    }

    pub(super) fn store_array_reference(
        &mut self,
        reference: ObjectRef,
        index: usize,
        value: Value,
    ) -> JayResult<()> {
        if !matches!(value, Value::Reference(_) | Value::Null) {
            return Err(JayError::new(format!(
                "expected reference for object array store, found {}",
                value.type_name(self)?
            )));
        }

        let descriptor = match self.object(reference)?.kind {
            ObjectKind::ObjectArray { ref descriptor, .. } => descriptor.clone(),
            _ => {
                return Err(JayError::new(format!(
                    "expected object array reference, found {}",
                    self.type_name(reference)?
                )));
            }
        };

        if let Value::Reference(stored_reference) = value {
            let actual = match self.value_type(stored_reference)? {
                Some(ValueType::Reference(reference_type)) => reference_type,
                Some(other) => {
                    return Err(JayError::new(format!(
                        "array store value had unexpected type {}",
                        other.name()
                    )));
                }
                None => return Err(JayError::new("array store value type is unavailable")),
            };
            if !is_reference_store_compatible(&actual, &descriptor) {
                return Err(JayError::fault(
                    "java/lang/ArrayStoreException",
                    Some(self.type_name(stored_reference)?),
                ));
            }
        }

        match self.object_mut(reference)?.kind {
            ObjectKind::ObjectArray {
                ref mut elements, ..
            } => {
                let length = elements.len();
                let Some(slot) = elements.get_mut(index) else {
                    return Err(array_index_fault(index, length));
                };
                *slot = value;
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
                ObjectKind::ObjectArray { ref elements, .. } => {
                    elements.iter().filter_map(Value::object_ref).collect()
                }
                ObjectKind::PrimitiveArray(_) | ObjectKind::StringBuilder(_) => Vec::new(),
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

/// The `ArrayIndexOutOfBoundsException` fault for `index` in an array of `length`.
pub(super) fn array_index_fault(index: impl std::fmt::Display, length: usize) -> JayError {
    JayError::fault(
        "java/lang/ArrayIndexOutOfBoundsException",
        Some(format!("Index {index} out of bounds for length {length}")),
    )
}

/// Renders an array descriptor such as `[Ljava/lang/String;` or `[[I` as Java source spelling.
pub(super) fn reference_array_name(descriptor: &str) -> String {
    let Some(component) = descriptor.strip_prefix('[') else {
        return descriptor.replace('/', ".");
    };
    let component_name = match component {
        "Z" => "boolean".to_string(),
        "B" => "byte".to_string(),
        "C" => "char".to_string(),
        "S" => "short".to_string(),
        "I" => "int".to_string(),
        "J" => "long".to_string(),
        "F" => "float".to_string(),
        "D" => "double".to_string(),
        _ => match component
            .strip_prefix('L')
            .and_then(|c| c.strip_suffix(';'))
        {
            Some(class_name) => class_name.replace('/', "."),
            None => reference_array_name(component),
        },
    };
    format!("{component_name}[]")
}

fn reference_array_component(descriptor: &str) -> Option<&str> {
    descriptor.strip_prefix('[')
}

fn is_reference_store_compatible(actual: &str, array_descriptor: &str) -> bool {
    let Some(expected_component) = reference_array_component(array_descriptor) else {
        return false;
    };
    if expected_component == "Ljava/lang/Object;" {
        return true;
    }
    if expected_component.starts_with('[') {
        return actual == expected_component;
    }

    !actual.starts_with('[')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_arrays_round_trip_each_element_kind() {
        let mut heap = Heap::new();
        let cases = [
            (PrimitiveElement::Boolean, Value::Int(1), Value::Int(1)),
            (PrimitiveElement::Byte, Value::Int(300), Value::Int(44)),
            (PrimitiveElement::Char, Value::Int(-1), Value::Int(65535)),
            (
                PrimitiveElement::Short,
                Value::Int(90000),
                Value::Int(24464),
            ),
            (PrimitiveElement::Int, Value::Int(-7), Value::Int(-7)),
            (
                PrimitiveElement::Long,
                Value::Long(1 << 40),
                Value::Long(1 << 40),
            ),
            (
                PrimitiveElement::Float,
                Value::Float(1.5),
                Value::Float(1.5),
            ),
        ];

        for (element, stored, expected) in cases {
            let array = heap.allocate_primitive_array(element, 2);
            heap.store_primitive(array, 1, stored).unwrap();
            assert_eq!(heap.load_primitive(array, 1).unwrap(), expected);
            assert_eq!(heap.array_length(array).unwrap(), 2);
            assert_eq!(
                heap.value_type(array).unwrap(),
                Some(ValueType::Reference(element.array_descriptor().to_string()))
            );
        }
    }

    #[test]
    fn boolean_arrays_keep_only_the_low_bit() {
        let mut heap = Heap::new();
        let array = heap.allocate_primitive_array(PrimitiveElement::Boolean, 1);
        heap.store_primitive(array, 0, Value::Int(2)).unwrap();
        assert_eq!(heap.load_primitive(array, 0).unwrap(), Value::Int(0));
    }

    #[test]
    fn primitive_arrays_report_bounds_and_kind_errors() {
        let mut heap = Heap::new();
        let array = heap.allocate_primitive_array(PrimitiveElement::Int, 3);

        let error = heap.load_primitive(array, 3).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Index 3 out of bounds for length 3")
        );

        let error = heap.store_primitive(array, 0, Value::Long(1)).unwrap_err();
        assert!(error.to_string().contains("expected int element for int[]"));

        assert_eq!(heap.type_name(array).unwrap(), "int[]");
        assert!(
            heap.string(array)
                .unwrap_err()
                .to_string()
                .contains("int[]")
        );
    }

    #[test]
    fn newarray_atype_decoding_rejects_double() {
        assert_eq!(
            PrimitiveElement::from_atype(10).unwrap(),
            PrimitiveElement::Int
        );
        assert!(
            PrimitiveElement::from_atype(7)
                .unwrap_err()
                .to_string()
                .contains("double")
        );
        assert!(PrimitiveElement::from_atype(12).is_err());
    }

    #[test]
    fn garbage_collection_treats_primitive_arrays_as_leaves() {
        let mut heap = Heap::new();
        let kept = heap.allocate_primitive_array(PrimitiveElement::Int, 1);
        let dropped = heap.allocate_primitive_array(PrimitiveElement::Long, 1);

        heap.collect([Value::Reference(kept)].iter());

        assert_eq!(heap.array_length(kept).unwrap(), 1);
        assert!(heap.array_length(dropped).is_err());
    }

    #[test]
    fn array_names_render_primitive_and_nested_components() {
        assert_eq!(reference_array_name("[I"), "int[]");
        assert_eq!(reference_array_name("[[I"), "int[][]");
        assert_eq!(
            reference_array_name("[Ljava/lang/String;"),
            "java.lang.String[]"
        );
        assert_eq!(
            reference_array_name("[[Ljava/lang/String;"),
            "java.lang.String[][]"
        );
    }

    #[test]
    fn string_builder_replaces_instance_and_reports_its_class() {
        let mut heap = Heap::new();
        let builder = heap.allocate_instance("java/lang/StringBuilder");
        heap.replace_with_string_builder(builder, "ab").unwrap();
        heap.string_builder_mut(builder).unwrap().push('c');

        assert_eq!(heap.string_builder(builder).unwrap(), "abc");
        assert_eq!(
            heap.instance_class_name(builder).unwrap(),
            "java/lang/StringBuilder"
        );
        assert_eq!(heap.type_name(builder).unwrap(), "java.lang.StringBuilder");
        assert!(heap.string(builder).is_err());

        let plain = heap.allocate_instance("example/Empty");
        assert!(heap.replace_with_string_builder(plain, "").is_err());
    }

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
        let array = heap.allocate_reference_array("[Ljava/lang/Object;", 2);
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
    fn heap_allows_reference_stores_for_class_components() {
        let mut heap = Heap::new();
        let array = heap.allocate_reference_array("[Ljava/lang/String;", 1);
        let value = heap.allocate_instance("java/lang/Integer");

        heap.store_array_reference(array, 0, Value::Reference(value))
            .unwrap();
    }

    #[test]
    fn heap_loads_unset_array_reference_slots_as_nulls() {
        let mut heap = Heap::new();
        let array = heap.allocate_reference_array("[Ljava/lang/Object;", 1);

        assert_eq!(heap.load_array_reference(array, 0).unwrap(), Value::Null);
    }

    #[test]
    fn heap_reports_typed_reference_arrays() {
        let mut heap = Heap::new();
        let array = heap.allocate_reference_array("[Ljava/util/HashMap$Node;", 1);

        assert_eq!(
            heap.value_type(array).unwrap(),
            Some(ValueType::Reference(
                "[Ljava/util/HashMap$Node;".to_string()
            ))
        );
        assert_eq!(heap.type_name(array).unwrap(), "java.util.HashMap$Node[]");
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
