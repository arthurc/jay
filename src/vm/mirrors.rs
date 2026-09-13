//! `java.lang.Class` mirrors for classes, arrays, and primitive types.
//!
//! A mirror is an ordinary `java/lang/Class` instance on the heap, created at
//! most once per type and kept alive in `Interpreter::class_mirrors`. The
//! fields HotSpot injects into `Class` (`name`, `componentType`, `primitive`,
//! `modifiers`) are populated eagerly so the JDK's own `Class.getName()`,
//! `isArray()`, `isPrimitive()`, `getComponentType()`, and `isInterface()`
//! bytecode reads them directly; the JDK 21 native variants of those methods
//! read the same information through `natives.rs`.
//!
//! Mirrors are keyed by the type's runtime name: an internal class name such
//! as `java/lang/String`, an array descriptor such as `[I`, or a primitive
//! keyword such as `int`.

use std::io::Write;

use super::heap::{FieldKey, ObjectRef};
use super::interpreter::Interpreter;
use super::value::Value;
use crate::{JayError, JayResult};

/// The primitive type keywords, in the order `Class.getPrimitiveClass` accepts them.
const PRIMITIVE_NAMES: [&str; 9] = [
    "boolean", "byte", "char", "short", "int", "long", "float", "double", "void",
];

/// `ACC_PUBLIC | ACC_FINAL | ACC_ABSTRACT`, the modifiers of array and primitive classes.
const SYNTHETIC_TYPE_MODIFIERS: i32 = 0x0001 | 0x0010 | 0x0400;

const ACC_INTERFACE: u16 = 0x0200;

impl<'a, W: Write> Interpreter<'a, W> {
    /// Returns the mirror for a runtime type name, creating it on first use.
    ///
    /// Class mirrors load (but never initialize) the represented class to read
    /// its modifiers. Array mirrors create their component mirror recursively.
    pub(super) fn class_mirror(&mut self, type_name: &str) -> JayResult<ObjectRef> {
        if let Some(reference) = self.class_mirrors.get(type_name) {
            return Ok(*reference);
        }

        let (modifiers, component) = if let Some(component) = type_name.strip_prefix('[') {
            let component_name = match component.strip_prefix('L') {
                Some(class) => class
                    .strip_suffix(';')
                    .ok_or_else(|| JayError::new(format!("malformed array type {type_name}")))?
                    .to_string(),
                None if component.starts_with('[') => component.to_string(),
                None => primitive_name_for_descriptor(component)
                    .ok_or_else(|| JayError::new(format!("malformed array type {type_name}")))?
                    .to_string(),
            };
            (
                SYNTHETIC_TYPE_MODIFIERS,
                Some(self.class_mirror(&component_name)?),
            )
        } else if PRIMITIVE_NAMES.contains(&type_name) {
            (SYNTHETIC_TYPE_MODIFIERS, None)
        } else {
            let class_file = self.load_class_file(type_name)?;
            (class_file.access_flags as i32, None)
        };

        // The mirror is rooted through `class_mirrors` before anything else
        // can allocate, and the name string is rooted through the mirror.
        let mirror = self.heap.allocate_instance("java/lang/Class");
        self.class_mirrors.insert(type_name.to_string(), mirror);
        let name = self.new_java_string(binary_name(type_name))?;
        self.heap.put_instance_field(
            mirror,
            class_field("name", "Ljava/lang/String;"),
            Value::Reference(name),
        )?;
        self.heap.put_instance_field(
            mirror,
            class_field("componentType", "Ljava/lang/Class;"),
            component.map_or(Value::Null, Value::Reference),
        )?;
        self.heap.put_instance_field(
            mirror,
            class_field("primitive", "Z"),
            Value::Int(PRIMITIVE_NAMES.contains(&type_name) as i32),
        )?;
        self.heap.put_instance_field(
            mirror,
            class_field("modifiers", "C"),
            Value::Int(modifiers),
        )?;
        Ok(mirror)
    }

    /// Finds the runtime type name represented by a `java.lang.Class` mirror.
    pub(super) fn mirrored_class_name(&self, mirror: ObjectRef) -> JayResult<String> {
        self.class_mirrors
            .iter()
            .find(|(_, reference)| **reference == mirror)
            .map(|(class_name, _)| class_name.clone())
            .ok_or_else(|| {
                JayError::new(format!(
                    "expected Class mirror, found {}",
                    self.heap.type_name(mirror).unwrap_or_default()
                ))
            })
    }

    /// Answers `Class.isArray()`, `isPrimitive()`, and `isInterface()` for a mirror.
    pub(super) fn mirror_kind(&self, mirror: ObjectRef) -> JayResult<MirrorKind> {
        let type_name = self.mirrored_class_name(mirror)?;
        if type_name.starts_with('[') {
            return Ok(MirrorKind::Array);
        }
        if PRIMITIVE_NAMES.contains(&type_name.as_str()) {
            return Ok(MirrorKind::Primitive);
        }
        let class_file = self.load_class_file(&type_name)?;
        Ok(if class_file.access_flags & ACC_INTERFACE != 0 {
            MirrorKind::Interface
        } else {
            MirrorKind::Class
        })
    }
}

/// What a mirror stands for, as `Class.isArray()`/`isPrimitive()`/`isInterface()` report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MirrorKind {
    Class,
    Interface,
    Array,
    Primitive,
}

fn class_field(name: &str, descriptor: &str) -> FieldKey {
    FieldKey::new("java/lang/Class", name, descriptor)
}

/// The `Class.getName()` spelling: dots for classes, descriptors for arrays,
/// keywords for primitives.
fn binary_name(type_name: &str) -> String {
    type_name.replace('/', ".")
}

/// Maps a primitive array component descriptor (`I`) to its keyword (`int`).
pub(super) fn primitive_name_for_descriptor(descriptor: &str) -> Option<&'static str> {
    Some(match descriptor {
        "Z" => "boolean",
        "B" => "byte",
        "C" => "char",
        "S" => "short",
        "I" => "int",
        "J" => "long",
        "F" => "float",
        "D" => "double",
        "V" => "void",
        _ => return None,
    })
}

/// Maps a primitive keyword (`int`) to its descriptor (`I`).
pub(super) fn descriptor_for_primitive_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "boolean" => "Z",
        "byte" => "B",
        "char" => "C",
        "short" => "S",
        "int" => "I",
        "long" => "J",
        "float" => "F",
        "double" => "D",
        "void" => "V",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::classpath::ClassResolver;

    fn test_classes() -> ClassResolver {
        let root = std::env::temp_dir().join(format!(
            "jay-mirrors-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        ClassResolver::new(PathBuf::from(&root)).unwrap()
    }

    fn field(
        interpreter: &Interpreter<Vec<u8>>,
        mirror: ObjectRef,
        name: &str,
        descriptor: &str,
    ) -> Option<Value> {
        interpreter
            .heap
            .get_instance_field(mirror, &class_field(name, descriptor))
            .unwrap()
    }

    #[test]
    fn mirrors_are_created_once_and_describe_their_type() {
        let classes = test_classes();
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);

        let int = interpreter.class_mirror("int").unwrap();
        assert_eq!(interpreter.class_mirror("int").unwrap(), int);
        assert_eq!(interpreter.mirror_kind(int).unwrap(), MirrorKind::Primitive);
        assert_eq!(
            field(&interpreter, int, "primitive", "Z"),
            Some(Value::Int(1))
        );
        assert_eq!(
            field(&interpreter, int, "componentType", "Ljava/lang/Class;"),
            Some(Value::Null)
        );

        let ints = interpreter.class_mirror("[I").unwrap();
        assert_eq!(interpreter.mirror_kind(ints).unwrap(), MirrorKind::Array);
        assert_eq!(
            field(&interpreter, ints, "componentType", "Ljava/lang/Class;"),
            Some(Value::Reference(int))
        );
        assert_eq!(
            field(&interpreter, ints, "modifiers", "C"),
            Some(Value::Int(0x411))
        );

        let strings = interpreter.class_mirror("[Ljava/lang/String;").unwrap();
        let string = interpreter.class_mirror("java/lang/String").unwrap();
        assert_eq!(
            field(&interpreter, strings, "componentType", "Ljava/lang/Class;"),
            Some(Value::Reference(string))
        );
        assert_eq!(interpreter.mirror_kind(string).unwrap(), MirrorKind::Class);
        let Some(Value::Reference(name)) =
            field(&interpreter, string, "name", "Ljava/lang/String;")
        else {
            panic!("missing name");
        };
        assert_eq!(interpreter.java_string(name).unwrap(), "java.lang.String");
        assert_eq!(
            interpreter.mirrored_class_name(strings).unwrap(),
            "[Ljava/lang/String;"
        );

        let runnable = interpreter.class_mirror("java/lang/Runnable").unwrap();
        assert_eq!(
            interpreter.mirror_kind(runnable).unwrap(),
            MirrorKind::Interface
        );
        assert!(interpreter.class_mirror("[X").is_err());
    }

    #[test]
    fn primitive_names_and_descriptors_round_trip() {
        for name in PRIMITIVE_NAMES {
            let descriptor = descriptor_for_primitive_name(name).unwrap();
            assert_eq!(primitive_name_for_descriptor(descriptor), Some(name));
        }
        assert_eq!(primitive_name_for_descriptor("L"), None);
        assert_eq!(descriptor_for_primitive_name("string"), None);
    }
}
