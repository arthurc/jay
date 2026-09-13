//! The residual `java.lang.String` shims.
//!
//! `String` itself runs from the JDK's own bytecode (see `strings.rs` for the
//! heap layout). The handful of methods intercepted here are the ones whose
//! JDK implementations bottom out in machinery the VM does not have:
//!
//! - `toUpperCase()` / `toLowerCase()` go through `Locale.getDefault()`, and
//!   `Locale.<clinit>` needs `ConcurrentHashMap`, `SoftReference`, and system
//!   properties. The no-argument overloads are served natively with Unicode
//!   default case mapping.
//! - `String.format(String, Object...)` builds a `java.util.Formatter`, which
//!   needs `Locale`, regular expressions, and `double`. A minimal formatter for
//!   `%s`, `%d`, `%%`, and `%n` is provided because `jdk.internal.util.Preconditions`
//!   uses it to build `IndexOutOfBoundsException` messages.
//!
//! Everything else — `length`, `charAt`, `equals`, `hashCode`, `indexOf`,
//! `substring`, `concat`, `replace`, `valueOf`, `intern`, constructors — is
//! interpreted from the JDK class files.

use std::io::Write;

use super::descriptors::ValueType;
use super::frame::Frame;
use super::heap::ObjectRef;
use super::interpreter::Interpreter;
use super::value::Value;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    /// Handles an `invokevirtual` on a `String` receiver for the shimmed
    /// case-mapping overloads.
    ///
    /// Returns `Ok(false)` when the method is not shimmed so the caller falls
    /// through to the JDK bytecode. Arguments have already been popped.
    pub(super) fn try_invoke_string_method(
        &mut self,
        frame: &mut Frame,
        name: &str,
        descriptor: &str,
        receiver: ObjectRef,
    ) -> JayResult<bool> {
        let mapped = match (name, descriptor) {
            ("toUpperCase", "()Ljava/lang/String;") => self.java_string(receiver)?.to_uppercase(),
            ("toLowerCase", "()Ljava/lang/String;") => self.java_string(receiver)?.to_lowercase(),
            _ => return Ok(false),
        };
        let result = self.new_java_string(mapped)?;
        frame.stack.push(Value::Reference(result));
        self.collect_if_needed(frame);
        Ok(true)
    }

    /// Implements `String.format(String, Object...)` for `%s`, `%d`, `%%`, and `%n`.
    ///
    /// Arguments are formatted while still rooted on the caller's stack, since
    /// `%s` may run an interpreted `toString()`.
    pub(super) fn invoke_string_format(&mut self, caller: &mut Frame) -> JayResult<()> {
        let stack_len = caller.stack.len();
        let [Value::Reference(format), arguments] =
            &caller.stack[stack_len.checked_sub(2).ok_or_else(|| {
                JayError::new("String.format expected a format string and an argument array")
            })?..]
        else {
            return Err(JayError::new(
                "String.format expected a format string and an argument array",
            ));
        };
        let format = self.java_string(*format)?;
        let arguments = match arguments {
            Value::Reference(array) => (0..self.heap.array_length(*array)?)
                .map(|index| self.heap.load_array_reference(*array, index))
                .collect::<JayResult<Vec<_>>>()?,
            Value::Null => Vec::new(),
            other => {
                return Err(JayError::new(format!(
                    "String.format expected an Object[] argument, found {other:?}"
                )));
            }
        };

        let object_type = ValueType::Reference("java/lang/Object".to_string());
        let mut output = String::new();
        let mut next_argument = 0;
        let mut characters = format.chars();
        while let Some(character) = characters.next() {
            if character != '%' {
                output.push(character);
                continue;
            }
            match characters.next() {
                Some('%') => output.push('%'),
                Some('n') => output.push('\n'),
                Some(conversion @ ('s' | 'd')) => {
                    let argument = arguments.get(next_argument).ok_or_else(|| {
                        JayError::fault(
                            "java/util/MissingFormatArgumentException",
                            Some(format!("Format specifier '%{conversion}'")),
                        )
                    })?;
                    next_argument += 1;
                    output.push_str(&self.value_to_text(caller, &object_type, argument)?);
                }
                other => {
                    return Err(JayError::new(format!(
                        "unsupported String.format conversion {}",
                        other.map_or("at end of format".to_string(), |c| format!("%{c}"))
                    )));
                }
            }
        }

        caller.pop()?;
        caller.pop()?;
        let result = self.new_java_string(output)?;
        caller.stack.push(Value::Reference(result));
        self.collect_if_needed(caller);
        Ok(())
    }
}
