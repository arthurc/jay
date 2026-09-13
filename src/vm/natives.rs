//! Native method implementations.
//!
//! When method resolution lands on a method flagged `ACC_NATIVE`, the
//! interpreter looks it up here by class, name, and descriptor — the VM's
//! equivalent of a JNI registration table. Only natives that JDK bytecode
//! actually reaches are implemented; anything else fails with an explicit
//! `unsupported native method` error.
//!
//! Arguments arrive already popped from the caller's operand stack. Results
//! are pushed onto `frame` before any collection runs, and no native holds an
//! unrooted reference across an allocation.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use super::frame::Frame;
use super::heap::{ArrayKind, ObjectRef, PrimitiveElement};
use super::interpreter::Interpreter;
use super::mirrors::{MirrorKind, descriptor_for_primitive_name};
use super::value::Value;
use crate::{JayError, JayResult};

mod unsafe_memory;

impl<'a, W: Write> Interpreter<'a, W> {
    /// Runs the native method `class_name.name descriptor`.
    ///
    /// `receiver` is `None` for static natives. `arguments` excludes the receiver.
    pub(super) fn invoke_native(
        &mut self,
        frame: &mut Frame,
        class_name: &str,
        name: &str,
        descriptor: &str,
        receiver: Option<ObjectRef>,
        arguments: &[Value],
    ) -> JayResult<()> {
        if class_name == "jdk/internal/misc/Unsafe"
            && let Some(result) = self.try_invoke_unsafe_native(name, descriptor, arguments)?
        {
            if let Some(value) = result {
                frame.stack.push(value);
                self.collect_if_needed(frame);
            }
            return Ok(());
        }

        let result = match (class_name, name, descriptor) {
            // HotSpot uses these to register VM natives; Jay dispatches
            // natives through this table, so there is nothing to populate.
            ("java/lang/System", "registerNatives", "()V") => None,
            ("java/lang/System", "arraycopy", "(Ljava/lang/Object;ILjava/lang/Object;II)V") => {
                self.system_arraycopy(arguments)?;
                None
            }
            ("java/lang/System", "nanoTime", "()J") => Some(Value::Long(nano_time()?)),
            ("java/lang/Object", "clone", "()Ljava/lang/Object;") => Some(Value::Reference(
                self.object_clone(instance_receiver(receiver)?)?,
            )),
            ("java/lang/Object", "getClass", "()Ljava/lang/Class;") => {
                let class_name = self.reference_type_name(instance_receiver(receiver)?)?;
                Some(Value::Reference(self.class_mirror(&class_name)?))
            }
            // HotSpot captures the native backtrace here; Jay reports Java
            // frames through JayError instead, so the receiver is returned as is.
            ("java/lang/Throwable", "fillInStackTrace", "(I)Ljava/lang/Throwable;") => {
                Some(Value::Reference(instance_receiver(receiver)?))
            }
            ("java/lang/Class", "registerNatives", "()V") => None,
            ("java/lang/Class", "getPrimitiveClass", "(Ljava/lang/String;)Ljava/lang/Class;") => {
                let name = match arguments {
                    [Value::Reference(name)] => self.java_string(*name)?,
                    _ => return Err(JayError::new("Class.getPrimitiveClass expected a name")),
                };
                if descriptor_for_primitive_name(&name).is_none() {
                    return Err(JayError::new(format!(
                        "Class.getPrimitiveClass received unknown type {name}"
                    )));
                }
                Some(Value::Reference(self.class_mirror(&name)?))
            }
            // Assertions are always disabled.
            ("java/lang/Class", "desiredAssertionStatus0", "(Ljava/lang/Class;)Z") => {
                Some(Value::Int(0))
            }
            // JDK 21 answers these natively; JDK 27 reads the injected fields instead.
            ("java/lang/Class", "isArray", "()Z") => {
                let kind = self.mirror_kind(instance_receiver(receiver)?)?;
                Some(Value::Int((kind == MirrorKind::Array) as i32))
            }
            ("java/lang/Class", "isPrimitive", "()Z") => {
                let kind = self.mirror_kind(instance_receiver(receiver)?)?;
                Some(Value::Int((kind == MirrorKind::Primitive) as i32))
            }
            ("java/lang/Class", "isInterface", "()Z") => {
                let kind = self.mirror_kind(instance_receiver(receiver)?)?;
                Some(Value::Int((kind == MirrorKind::Interface) as i32))
            }
            ("java/lang/reflect/Array", "newArray", "(Ljava/lang/Class;I)Ljava/lang/Object;") => {
                let (component, length) = match arguments {
                    [Value::Reference(component), Value::Int(length)] => (*component, *length),
                    [Value::Null, _] => {
                        return Err(JayError::fault("java/lang/NullPointerException", None));
                    }
                    _ => return Err(JayError::new("Array.newArray expected a Class and an int")),
                };
                Some(Value::Reference(self.reflect_new_array(component, length)?))
            }
            // Class-data sharing is a HotSpot start-up optimization; reporting
            // it as disabled makes the JDK take its ordinary initialization paths.
            ("jdk/internal/misc/CDS", "initializeFromArchive", "(Ljava/lang/Class;)V")
            | ("jdk/internal/misc/CDS", "logLambdaFormInvoker", "(Ljava/lang/String;)V")
            | (
                "jdk/internal/misc/CDS",
                "defineArchivedModules",
                "(Ljava/lang/ClassLoader;Ljava/lang/ClassLoader;)V",
            ) => None,
            ("jdk/internal/misc/CDS", "getRandomSeedForDumping", "()J") => Some(Value::Long(0)),
            ("jdk/internal/misc/CDS", "getCDSConfigStatus", "()I")
            | ("jdk/internal/misc/CDS", "isDumpingClassList0", "()Z")
            | ("jdk/internal/misc/CDS", "isDumpingArchive0", "()Z")
            | ("jdk/internal/misc/CDS", "isSharingEnabled0", "()Z") => Some(Value::Int(0)),
            _ => {
                return Err(JayError::new(format!(
                    "unsupported native method {}.{name}{descriptor}",
                    class_name.replace('/', ".")
                )));
            }
        };

        if let Some(value) = result {
            frame.stack.push(value);
            self.collect_if_needed(frame);
        }
        Ok(())
    }

    /// Implements `System.arraycopy` with HotSpot's checks and messages:
    /// null arrays raise `NullPointerException`, kind mismatches raise
    /// `ArrayStoreException`, and bad ranges raise
    /// `ArrayIndexOutOfBoundsException` before anything is copied. Reference
    /// copies check each element against the destination component type and
    /// stop at the first incompatible one, leaving earlier elements copied.
    fn system_arraycopy(&mut self, arguments: &[Value]) -> JayResult<()> {
        let [
            source,
            Value::Int(source_pos),
            destination,
            Value::Int(destination_pos),
            Value::Int(length),
        ] = arguments
        else {
            return Err(JayError::new(format!(
                "System.arraycopy received unexpected arguments {arguments:?}"
            )));
        };
        let (Value::Reference(source), Value::Reference(destination)) = (source, destination)
        else {
            return Err(JayError::fault("java/lang/NullPointerException", None));
        };
        let (source, destination) = (*source, *destination);

        let source_kind = self.heap.array_kind(source)?.ok_or_else(|| {
            JayError::fault(
                "java/lang/ArrayStoreException",
                Some(format!(
                    "arraycopy: source type {} is not an array",
                    self.heap.type_name(source).unwrap_or_default()
                )),
            )
        })?;
        let destination_kind = self.heap.array_kind(destination)?.ok_or_else(|| {
            JayError::fault(
                "java/lang/ArrayStoreException",
                Some(format!(
                    "arraycopy: destination type {} is not an array",
                    self.heap.type_name(destination).unwrap_or_default()
                )),
            )
        })?;
        let same_kind = match (&source_kind, &destination_kind) {
            (ArrayKind::Primitive(left), ArrayKind::Primitive(right)) => left == right,
            (ArrayKind::Reference(_), ArrayKind::Reference(_)) => true,
            _ => false,
        };
        if !same_kind {
            return Err(JayError::fault(
                "java/lang/ArrayStoreException",
                Some(format!(
                    "arraycopy: type mismatch: can not copy {} into {}",
                    self.arraycopy_type_name(source)?,
                    self.arraycopy_type_name(destination)?
                )),
            ));
        }

        let source_length = self.heap.array_length(source)?;
        let destination_length = self.heap.array_length(destination)?;
        let bounds_fault = |message: String| {
            JayError::fault("java/lang/ArrayIndexOutOfBoundsException", Some(message))
        };
        if *source_pos < 0 {
            return Err(bounds_fault(format!(
                "arraycopy: source index {source_pos} out of bounds for {}",
                self.arraycopy_bounds_name(source, source_length)?
            )));
        }
        if *destination_pos < 0 {
            return Err(bounds_fault(format!(
                "arraycopy: destination index {destination_pos} out of bounds for {}",
                self.arraycopy_bounds_name(destination, destination_length)?
            )));
        }
        if *length < 0 {
            return Err(bounds_fault(format!(
                "arraycopy: length {length} is negative"
            )));
        }
        let (source_pos, destination_pos, length) = (
            *source_pos as usize,
            *destination_pos as usize,
            *length as usize,
        );
        if source_pos + length > source_length {
            return Err(bounds_fault(format!(
                "arraycopy: last source index {} out of bounds for {}",
                source_pos + length,
                self.arraycopy_bounds_name(source, source_length)?
            )));
        }
        if destination_pos + length > destination_length {
            return Err(bounds_fault(format!(
                "arraycopy: last destination index {} out of bounds for {}",
                destination_pos + length,
                self.arraycopy_bounds_name(destination, destination_length)?
            )));
        }

        match source_kind {
            ArrayKind::Primitive(_) => self.heap.copy_primitive_array(
                source,
                source_pos,
                destination,
                destination_pos,
                length,
            ),
            ArrayKind::Reference(_) => {
                // Snapshot first so overlapping ranges within one array copy correctly.
                let elements = (source_pos..source_pos + length)
                    .map(|index| self.heap.load_array_reference(source, index))
                    .collect::<JayResult<Vec<_>>>()?;
                for (offset, element) in elements.into_iter().enumerate() {
                    if let Value::Reference(reference) = &element
                        && !self.is_array_store_compatible(destination, *reference)?
                    {
                        return Err(JayError::fault(
                            "java/lang/ArrayStoreException",
                            Some(format!(
                                "arraycopy: element type mismatch: can not cast one of the elements of {} to the type of the destination array, {}",
                                self.heap.type_name(source)?,
                                self.arraycopy_component_name(destination)?
                            )),
                        ));
                    }
                    self.heap.store_array_reference(
                        destination,
                        destination_pos + offset,
                        element,
                    )?;
                }
                Ok(())
            }
        }
    }

    /// HotSpot spells reference arrays as `object array[]` in type-mismatch messages.
    fn arraycopy_type_name(&self, array: ObjectRef) -> JayResult<String> {
        Ok(match self.heap.array_kind(array)? {
            Some(ArrayKind::Reference(_)) => "object array[]".to_string(),
            _ => self.heap.type_name(array)?,
        })
    }

    /// HotSpot spells bounds as `int[3]` for primitives and `object array[3]` for references.
    fn arraycopy_bounds_name(&self, array: ObjectRef, length: usize) -> JayResult<String> {
        let name = self.arraycopy_type_name(array)?;
        Ok(format!("{}[{length}]", name.trim_end_matches("[]")))
    }

    /// The Java source spelling of a reference array's component type.
    fn arraycopy_component_name(&self, array: ObjectRef) -> JayResult<String> {
        let name = self.heap.type_name(array)?;
        Ok(name.trim_end_matches("[]").to_string())
    }

    /// Implements `Array.newArray`: allocates an array whose element type is
    /// the type mirrored by `component`.
    fn reflect_new_array(&mut self, component: ObjectRef, length: i32) -> JayResult<ObjectRef> {
        let length = usize::try_from(length).map_err(|_| {
            JayError::fault(
                "java/lang/NegativeArraySizeException",
                Some(length.to_string()),
            )
        })?;
        let component_name = self.mirrored_class_name(component)?;
        match descriptor_for_primitive_name(&component_name) {
            Some("V") => Err(JayError::fault("java/lang/IllegalArgumentException", None)),
            Some(descriptor) => {
                let element = PrimitiveElement::from_descriptor(descriptor)?;
                Ok(self.heap.allocate_primitive_array(element, length))
            }
            None => {
                let descriptor = if component_name.starts_with('[') {
                    format!("[{component_name}")
                } else {
                    format!("[L{component_name};")
                };
                Ok(self.heap.allocate_reference_array(descriptor, length))
            }
        }
    }

    /// Implements `Object.clone()`: arrays and `Cloneable` instances are copied
    /// shallowly; anything else raises `CloneNotSupportedException`.
    fn object_clone(&mut self, receiver: ObjectRef) -> JayResult<ObjectRef> {
        if self.heap.array_kind(receiver)?.is_some() {
            return self.heap.clone_array(receiver);
        }
        let class_name = self.reference_type_name(receiver)?;
        if !self.is_assignable_reference(&class_name, "java/lang/Cloneable")? {
            return Err(JayError::fault(
                "java/lang/CloneNotSupportedException",
                Some(class_name.replace('/', ".")),
            ));
        }
        self.heap.clone_instance(receiver)
    }
}

/// Monotonic-ish nanoseconds for `System.nanoTime`, taken from the wall clock
/// since the VM has no other high-resolution source.
fn nano_time() -> JayResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| JayError::new(format!("system time is before Unix epoch: {error}")))?;
    i64::try_from(duration.as_nanos())
        .map_err(|_| JayError::new("current time nanoseconds exceed long range"))
}

/// Unwraps the receiver of an instance native.
fn instance_receiver(receiver: Option<ObjectRef>) -> JayResult<ObjectRef> {
    receiver.ok_or_else(|| JayError::new("instance native method invoked without a receiver"))
}
