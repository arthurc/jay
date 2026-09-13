//! Java exception dispatch: materializing VM faults, locating handlers, and
//! formatting exceptions the way the JVM prints them.
//!
//! A thrown exception travels as a `JayError` carrying the heap slot of the
//! exception object. VM faults (null dereferences, bad indexes, division by
//! zero, ...) travel as `JayError::fault` values and become real Java objects
//! in the frame where they are first seen, so a `catch` block observes an
//! ordinary instance with a `detailMessage`.

use std::io::Write;

use super::heap::{FieldKey, ObjectRef};
use super::interpreter::Interpreter;
use super::value::Value;
use crate::classfile::Code;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    /// Resolves the exception object behind a Java-exception error, allocating
    /// it for a pending fault. Returns the object and the error to propagate
    /// if no handler is found.
    ///
    /// GC safety: `materialize_fault` allocates without collecting, and the
    /// caller either roots the object on a frame stack or returns the error
    /// immediately, so the in-flight object is never exposed to a collection.
    pub(super) fn exception_object(&mut self, error: JayError) -> JayResult<(ObjectRef, JayError)> {
        if let Some(slot) = error.thrown_object() {
            return Ok((ObjectRef::from_index(slot), error));
        }

        let exception = self.materialize_fault(&error)?;
        let display = self.exception_display(exception)?;
        Ok((exception, error.into_thrown(exception.index(), display)))
    }

    /// Allocates the Java object for a VM fault without running the JDK
    /// constructor: the class is instantiated and `Throwable.detailMessage`
    /// is set directly. Performs at most two allocations and never collects.
    fn materialize_fault(&mut self, error: &JayError) -> JayResult<ObjectRef> {
        let class_name = error
            .fault_class()
            .ok_or_else(|| JayError::new("expected a VM fault to materialize"))?;
        let exception = self.heap.allocate_instance(class_name);
        if let Some(message) = error.fault_message() {
            let message = self.heap.allocate_string(message);
            self.heap.put_instance_field(
                exception,
                detail_message_field(),
                Value::Reference(message),
            )?;
        }
        Ok(exception)
    }

    /// Formats an exception as `Throwable.toString()` does:
    /// `java.lang.IllegalStateException: boom`, or just the class name when
    /// there is no detail message.
    pub(super) fn exception_display(&self, exception: ObjectRef) -> JayResult<String> {
        let class_name = self.heap.instance_class_name(exception)?.replace('/', ".");
        match self
            .heap
            .get_instance_field(exception, &detail_message_field())?
        {
            Some(Value::Reference(message)) => {
                Ok(format!("{class_name}: {}", self.heap.string(message)?))
            }
            _ => Ok(class_name),
        }
    }

    /// Finds the first handler in table order that covers `pc` and whose catch
    /// type (if any) the exception is assignable to. Returns the handler pc.
    pub(super) fn find_handler(
        &self,
        code: &Code,
        pc: usize,
        exception: ObjectRef,
    ) -> JayResult<Option<usize>> {
        let exception_class = self.heap.instance_class_name(exception)?.to_string();
        for handler in &code.exception_table {
            if !handler.covers(pc) {
                continue;
            }
            let applies = match &handler.catch_type {
                None => true,
                Some(catch_type) => self.is_assignable_reference(&exception_class, catch_type)?,
            };
            if applies {
                return Ok(Some(handler.handler_pc as usize));
            }
        }
        Ok(None)
    }
}

/// The `Throwable.detailMessage` field read by `getMessage()`.
fn detail_message_field() -> FieldKey {
    FieldKey::new("java/lang/Throwable", "detailMessage", "Ljava/lang/String;")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::classfile::ExceptionHandler;
    use crate::classpath::ClassResolver;

    fn test_classes(name: &str) -> ClassResolver {
        let root = std::env::temp_dir().join(format!(
            "jay-exceptions-test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        ClassResolver::new(PathBuf::from(&root)).unwrap()
    }

    fn handler(
        start_pc: u16,
        end_pc: u16,
        handler_pc: u16,
        catch_type: Option<&str>,
    ) -> ExceptionHandler {
        ExceptionHandler {
            start_pc,
            end_pc,
            handler_pc,
            catch_type: catch_type.map(str::to_string),
        }
    }

    #[test]
    fn find_handler_respects_ranges_order_and_catch_types() {
        let classes = test_classes("find-handler");
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);
        let exception = interpreter
            .heap
            .allocate_instance("java/lang/IllegalStateException");
        let code = Code {
            max_stack: 1,
            max_locals: 0,
            bytes: vec![0; 40],
            exception_table: vec![
                handler(0, 10, 20, Some("java/lang/IllegalArgumentException")),
                handler(0, 10, 21, Some("java/lang/RuntimeException")),
                handler(0, 10, 22, None),
                handler(10, 30, 23, None),
            ],
        };

        assert_eq!(
            interpreter.find_handler(&code, 5, exception).unwrap(),
            Some(21)
        );
        assert_eq!(
            interpreter.find_handler(&code, 10, exception).unwrap(),
            Some(23)
        );
        assert_eq!(
            interpreter.find_handler(&code, 30, exception).unwrap(),
            None
        );
    }

    #[test]
    fn faults_materialize_with_detail_message_and_java_display() {
        let classes = test_classes("materialize");
        let mut output = Vec::new();
        let mut interpreter = Interpreter::new(&classes, &mut output);

        let fault = JayError::fault(
            "java/lang/ArithmeticException",
            Some("/ by zero".to_string()),
        );
        let (exception, error) = interpreter.exception_object(fault).unwrap();
        assert_eq!(
            interpreter.exception_display(exception).unwrap(),
            "java.lang.ArithmeticException: / by zero"
        );
        assert_eq!(error.thrown_object(), Some(exception.index()));
        assert_eq!(
            error.to_string(),
            "java.lang.ArithmeticException: / by zero"
        );

        let bare = JayError::fault("java/lang/NullPointerException", None);
        let (exception, _) = interpreter.exception_object(bare).unwrap();
        assert_eq!(
            interpreter.exception_display(exception).unwrap(),
            "java.lang.NullPointerException"
        );
    }
}
