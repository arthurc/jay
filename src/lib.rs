pub mod classfile;
pub mod classpath;
pub mod cli;
mod jdk;
pub mod jimage;
pub mod vm;

use std::fmt;

pub type JayResult<T> = Result<T, JayError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JayError {
    message: String,
    java_stack_trace: Vec<JavaStackFrame>,
    /// Heap slot of the Java exception object in flight, when this error is a
    /// thrown exception rather than a VM failure.
    thrown_object: Option<usize>,
    /// Internal name of the Java exception class a VM fault should become
    /// (for example `java/lang/NullPointerException`) once the interpreter
    /// materializes it. Cleared when the object is allocated.
    fault_class: Option<&'static str>,
}

/// One interpreted Java frame active when a VM runtime error occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaStackFrame {
    /// Java binary class name in dotted form, such as `java.lang.String`.
    pub class_name: String,
    /// JVM method name, including special names like `<clinit>`.
    pub method_name: String,
    /// JVM method descriptor for the active method.
    pub descriptor: String,
    /// Bytecode program counter for the instruction that failed.
    pub pc: usize,
}

impl JavaStackFrame {
    /// Builds a stack frame for an interpreted Java method location.
    pub fn new(
        class_name: impl Into<String>,
        method_name: impl Into<String>,
        descriptor: impl Into<String>,
        pc: usize,
    ) -> Self {
        Self {
            class_name: class_name.into(),
            method_name: method_name.into(),
            descriptor: descriptor.into(),
            pc,
        }
    }
}

impl JayError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            java_stack_trace: Vec::new(),
            thrown_object: None,
            fault_class: None,
        }
    }

    /// Creates the error that carries a thrown Java exception object.
    ///
    /// `display` is the text Java prints for the exception, such as
    /// `java.lang.IllegalStateException: boom`.
    pub fn thrown(heap_slot: usize, display: impl Into<String>) -> Self {
        Self {
            thrown_object: Some(heap_slot),
            ..Self::new(display)
        }
    }

    /// Creates a VM fault that the interpreter turns into a Java exception of
    /// `class_name` with `message` as its detail message (`None` for no message).
    pub fn fault(class_name: &'static str, message: Option<String>) -> Self {
        Self {
            fault_class: Some(class_name),
            ..Self::new(message.unwrap_or_default())
        }
    }

    /// Converts a pending fault into a thrown error for `heap_slot`, keeping
    /// any Java stack frames already recorded.
    pub fn into_thrown(self, heap_slot: usize, display: impl Into<String>) -> Self {
        Self {
            message: display.into(),
            java_stack_trace: self.java_stack_trace,
            thrown_object: Some(heap_slot),
            fault_class: None,
        }
    }

    /// Whether this error represents a Java exception (thrown or a pending fault).
    pub fn is_java_exception(&self) -> bool {
        self.thrown_object.is_some() || self.fault_class.is_some()
    }

    /// Heap slot of the thrown exception object, if one has been materialized.
    pub fn thrown_object(&self) -> Option<usize> {
        self.thrown_object
    }

    /// Exception class of a VM fault that has not yet been materialized.
    pub fn fault_class(&self) -> Option<&'static str> {
        self.fault_class
    }

    /// The detail message of a fault, or `None` when the fault carries none.
    pub fn fault_message(&self) -> Option<&str> {
        (!self.message.is_empty()).then_some(self.message.as_str())
    }

    /// Adds a Java frame to the end of the stacktrace.
    pub fn push_java_stack_frame(&mut self, frame: JavaStackFrame) {
        self.java_stack_trace.push(frame);
    }

    /// Returns this error with one additional Java stack frame.
    pub fn with_java_stack_frame(mut self, frame: JavaStackFrame) -> Self {
        self.push_java_stack_frame(frame);
        self
    }

    /// Returns Java stack frames in top-frame-first order.
    pub fn java_stack_trace(&self) -> &[JavaStackFrame] {
        &self.java_stack_trace
    }
}

impl fmt::Display for JayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for JayError {}

impl From<std::io::Error> for JayError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_keeps_base_error_message_without_java_stack_trace() {
        let mut error = JayError::new("unsupported bytecode");
        error.push_java_stack_frame(JavaStackFrame::new("Main", "inner", "()V", 3));

        assert_eq!(error.to_string(), "unsupported bytecode");
    }

    #[test]
    fn thrown_errors_carry_the_object_and_java_display_text() {
        let error = JayError::thrown(7, "java.lang.IllegalStateException: boom");

        assert!(error.is_java_exception());
        assert_eq!(error.thrown_object(), Some(7));
        assert_eq!(error.fault_class(), None);
        assert_eq!(error.to_string(), "java.lang.IllegalStateException: boom");
    }

    #[test]
    fn faults_name_the_exception_class_and_optional_message() {
        let with_message = JayError::fault(
            "java/lang/ArithmeticException",
            Some("/ by zero".to_string()),
        );
        assert!(with_message.is_java_exception());
        assert_eq!(
            with_message.fault_class(),
            Some("java/lang/ArithmeticException")
        );
        assert_eq!(with_message.fault_message(), Some("/ by zero"));

        let without_message = JayError::fault("java/lang/NullPointerException", None);
        assert_eq!(without_message.fault_message(), None);
        assert_eq!(without_message.thrown_object(), None);

        assert!(!JayError::new("unsupported bytecode").is_java_exception());
    }

    #[test]
    fn java_stack_trace_keeps_top_frame_first_order() {
        let mut error = JayError::new("unsupported bytecode");
        error.push_java_stack_frame(JavaStackFrame::new("Main", "inner", "()V", 3));
        error.push_java_stack_frame(JavaStackFrame::new("Main", "outer", "()V", 7));

        assert_eq!(
            error.java_stack_trace(),
            [
                JavaStackFrame::new("Main", "inner", "()V", 3),
                JavaStackFrame::new("Main", "outer", "()V", 7),
            ]
        );
    }
}
