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
        }
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
