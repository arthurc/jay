//! Rust-side access to `java.lang.String` objects.
//!
//! Every place the VM reads or creates Java strings from Rust — printing,
//! exception messages, `invokedynamic` concatenation, native shims — goes
//! through this module, so the heap representation of a string stays a
//! private detail of the heap and can change without touching callers.
//!
//! GC contract: `new_java_string` allocates without collecting and returns an
//! unrooted reference. Callers must root it (push it onto a frame stack, store
//! it in a field, ...) before anything that may call `collect_if_needed`.

use std::io::Write;

use super::frame::Frame;
use super::heap::ObjectRef;
use super::interpreter::Interpreter;
use super::value::Value;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    /// Reads the text of a `java.lang.String` object.
    pub(super) fn java_string(&self, reference: ObjectRef) -> JayResult<String> {
        Ok(self.heap.string(reference)?.to_string())
    }

    /// Returns whether `reference` points at a `java.lang.String`.
    pub(super) fn is_java_string(&self, reference: ObjectRef) -> bool {
        self.heap.string(reference).is_ok()
    }

    /// Allocates a new, unrooted `java.lang.String` holding `text`.
    pub(super) fn new_java_string(&mut self, text: impl Into<String>) -> ObjectRef {
        self.heap.allocate_string(text)
    }

    /// Pops a non-null `String` reference from `frame`'s operand stack.
    pub(super) fn pop_java_string(&self, frame: &mut Frame) -> JayResult<ObjectRef> {
        match frame.pop()? {
            Value::Reference(reference) if self.is_java_string(reference) => Ok(reference),
            Value::Reference(reference) => Err(JayError::new(format!(
                "expected String reference, found {}",
                self.heap.type_name(reference)?
            ))),
            other => Err(JayError::new(format!(
                "expected string on stack, found {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::classpath::ClassResolver;

    fn test_classes() -> ClassResolver {
        let root = std::env::temp_dir().join(format!(
            "jay-strings-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        ClassResolver::new(PathBuf::from(&root)).unwrap()
    }

    #[test]
    fn java_strings_round_trip_through_the_heap() {
        let classes = test_classes();
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);

        let text = interpreter.new_java_string("héllo \u{1F600}");
        assert!(interpreter.is_java_string(text));
        assert_eq!(interpreter.java_string(text).unwrap(), "héllo \u{1F600}");

        let other = interpreter.heap.allocate_instance("example/Empty");
        assert!(!interpreter.is_java_string(other));
        assert!(interpreter.java_string(other).is_err());
    }

    #[test]
    fn popping_a_java_string_rejects_other_values() {
        let classes = test_classes();
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);
        let mut frame = Frame::new(0);

        let text = interpreter.new_java_string("value");
        frame.stack.push(Value::Reference(text));
        assert_eq!(interpreter.pop_java_string(&mut frame).unwrap(), text);

        let other = interpreter.heap.allocate_instance("example/Empty");
        frame.stack.push(Value::Reference(other));
        assert!(
            interpreter
                .pop_java_string(&mut frame)
                .unwrap_err()
                .to_string()
                .contains("expected String reference, found example.Empty")
        );

        frame.stack.push(Value::Int(1));
        assert!(
            interpreter
                .pop_java_string(&mut frame)
                .unwrap_err()
                .to_string()
                .contains("expected string on stack")
        );
    }
}
