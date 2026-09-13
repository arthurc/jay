//! `java.lang.String` shims.
//!
//! Strings are native Rust values on the VM heap, so the JDK's own
//! `String` bytecode (which reads the `byte[] value` field) can never run.
//! Instead the common instance methods and static factories are implemented
//! here with Java's UTF-16 semantics: lengths, indexes, and comparisons all
//! operate on UTF-16 code units.

use std::io::Write;

use super::frame::Frame;
use super::heap::ObjectRef;
use super::interpreter::Interpreter;
use super::value::Value;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    /// Handles an `invokevirtual` on a `String` receiver.
    ///
    /// Returns `Ok(false)` when the method is not shimmed so the caller can
    /// fall through to its normal error path. Arguments have already been
    /// popped; results are pushed onto `frame` before any collection runs.
    pub(super) fn try_invoke_string_method(
        &mut self,
        frame: &mut Frame,
        name: &str,
        descriptor: &str,
        receiver: ObjectRef,
        arguments: &[Value],
    ) -> JayResult<bool> {
        let text = self.java_string(receiver)?;
        let units = utf16(&text);

        let result = match (name, descriptor) {
            ("length", "()I") => Value::Int(units.len() as i32),
            ("isEmpty", "()Z") => bool_value(units.is_empty()),
            ("toString", "()Ljava/lang/String;") => Value::Reference(receiver),
            ("hashCode", "()I") => Value::Int(string_hash(&units)),
            ("charAt", "(I)C") => {
                let index = int_argument(arguments, 0, "String.charAt")?;
                let unit = checked_unit(&units, index)?;
                Value::Int(unit as i32)
            }
            ("equals", "(Ljava/lang/Object;)Z") => {
                let equal = match arguments.first() {
                    Some(Value::Reference(other)) => {
                        self.java_string(*other).ok().as_deref() == Some(text.as_str())
                    }
                    _ => false,
                };
                bool_value(equal)
            }
            ("equalsIgnoreCase", "(Ljava/lang/String;)Z") => {
                let equal = match self.string_argument(arguments, 0, "String.equalsIgnoreCase")? {
                    Some(other) => other.to_lowercase() == text.to_lowercase(),
                    None => false,
                };
                bool_value(equal)
            }
            ("compareTo", "(Ljava/lang/String;)I") => {
                let other = self.required_string_argument(arguments, 0, "String.compareTo")?;
                Value::Int(compare_units(&units, &utf16(&other)))
            }
            ("contains", "(Ljava/lang/CharSequence;)Z") => {
                let other = self.required_string_argument(arguments, 0, "String.contains")?;
                bool_value(find_units(&units, &utf16(&other), 0).is_some())
            }
            ("startsWith", "(Ljava/lang/String;)Z") => {
                let other = self.required_string_argument(arguments, 0, "String.startsWith")?;
                bool_value(units.starts_with(&utf16(&other)))
            }
            ("endsWith", "(Ljava/lang/String;)Z") => {
                let other = self.required_string_argument(arguments, 0, "String.endsWith")?;
                bool_value(units.ends_with(&utf16(&other)))
            }
            ("indexOf", "(I)I") => {
                let unit = int_argument(arguments, 0, "String.indexOf")?;
                Value::Int(index_of_unit(&units, unit, 0))
            }
            ("indexOf", "(II)I") => {
                let unit = int_argument(arguments, 0, "String.indexOf")?;
                let from = int_argument(arguments, 1, "String.indexOf")?;
                Value::Int(index_of_unit(&units, unit, from))
            }
            ("indexOf", "(Ljava/lang/String;)I") => {
                let other = self.required_string_argument(arguments, 0, "String.indexOf")?;
                Value::Int(index_of_units(&units, &utf16(&other), 0))
            }
            ("indexOf", "(Ljava/lang/String;I)I") => {
                let other = self.required_string_argument(arguments, 0, "String.indexOf")?;
                let from = int_argument(arguments, 1, "String.indexOf")?;
                Value::Int(index_of_units(&units, &utf16(&other), from))
            }
            ("lastIndexOf", "(I)I") => {
                let unit = int_argument(arguments, 0, "String.lastIndexOf")?;
                let index = units
                    .iter()
                    .rposition(|candidate| *candidate as i32 == unit);
                Value::Int(index.map_or(-1, |index| index as i32))
            }
            ("lastIndexOf", "(Ljava/lang/String;)I") => {
                let other = self.required_string_argument(arguments, 0, "String.lastIndexOf")?;
                Value::Int(last_index_of_units(&units, &utf16(&other)))
            }
            ("substring", "(I)Ljava/lang/String;") => {
                let begin = int_argument(arguments, 0, "String.substring")?;
                let slice = checked_slice(&units, begin, units.len() as i32)?;
                self.allocate_units(slice)
            }
            ("substring", "(II)Ljava/lang/String;") => {
                let begin = int_argument(arguments, 0, "String.substring")?;
                let end = int_argument(arguments, 1, "String.substring")?;
                let slice = checked_slice(&units, begin, end)?;
                self.allocate_units(slice)
            }
            ("trim", "()Ljava/lang/String;") => {
                self.allocate_text(text.trim_matches(|c: char| c <= ' '))
            }
            ("toUpperCase", "()Ljava/lang/String;") => self.allocate_text(&text.to_uppercase()),
            ("toLowerCase", "()Ljava/lang/String;") => self.allocate_text(&text.to_lowercase()),
            ("concat", "(Ljava/lang/String;)Ljava/lang/String;") => {
                let other = self.required_string_argument(arguments, 0, "String.concat")?;
                self.allocate_text(&format!("{text}{other}"))
            }
            ("replace", "(CC)Ljava/lang/String;") => {
                let from = int_argument(arguments, 0, "String.replace")? as u16;
                let to = int_argument(arguments, 1, "String.replace")? as u16;
                let replaced = units
                    .iter()
                    .map(|unit| if *unit == from { to } else { *unit })
                    .collect::<Vec<_>>();
                self.allocate_units(&replaced)
            }
            ("replace", "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;") => {
                let from = self.required_string_argument(arguments, 0, "String.replace")?;
                let to = self.required_string_argument(arguments, 1, "String.replace")?;
                self.allocate_text(&text.replace(&from, &to))
            }
            ("toCharArray", "()[C") => Value::Reference(self.heap.allocate_char_array(&units)),
            _ => return Ok(false),
        };

        frame.stack.push(result);
        self.collect_if_needed(frame);
        Ok(true)
    }

    /// Handles `String` static factories invoked through `invokestatic`.
    ///
    /// Returns `Ok(false)` when the method is not shimmed. Arguments are
    /// popped here only when the method is handled.
    pub(super) fn try_invoke_string_static(
        &mut self,
        frame: &mut Frame,
        class_name: &str,
        name: &str,
        descriptor: &str,
    ) -> JayResult<bool> {
        let text = match (class_name, name, descriptor) {
            ("java/lang/String", "valueOf", "(I)Ljava/lang/String;")
            | ("java/lang/Integer", "toString", "(I)Ljava/lang/String;") => {
                frame.pop_int()?.to_string()
            }
            ("java/lang/String", "valueOf", "(J)Ljava/lang/String;")
            | ("java/lang/Long", "toString", "(J)Ljava/lang/String;") => {
                frame.pop_long()?.to_string()
            }
            ("java/lang/String", "valueOf", "(C)Ljava/lang/String;") => {
                super::native::char_to_string(frame.pop_int()?)
            }
            ("java/lang/String", "valueOf", "(Z)Ljava/lang/String;") => if frame.pop_int()? == 0 {
                "false"
            } else {
                "true"
            }
            .to_string(),
            ("java/lang/String", "valueOf", "([C)Ljava/lang/String;") => {
                let array = frame.pop_object_ref()?;
                from_utf16(self.heap.char_array_units(array)?)
            }
            ("java/lang/Integer", "parseInt", "(Ljava/lang/String;)I") => {
                let reference = self.pop_java_string(frame)?;
                let digits = self.java_string(reference)?;
                let value = parse_java_int(&digits)?;
                frame.stack.push(Value::Int(value));
                return Ok(true);
            }
            _ => return Ok(false),
        };

        let reference = self.new_java_string(text);
        frame.stack.push(Value::Reference(reference));
        self.collect_if_needed(frame);
        Ok(true)
    }

    /// Handles `new String(...)` constructors by replacing the freshly allocated
    /// instance with a native string value.
    ///
    /// Returns `Ok(false)` when the constructor is not shimmed.
    pub(super) fn try_invoke_string_constructor(
        &mut self,
        frame: &mut Frame,
        descriptor: &str,
    ) -> JayResult<bool> {
        let text = match descriptor {
            "()V" => String::new(),
            "(Ljava/lang/String;)V" => {
                let reference = self.pop_java_string(frame)?;
                self.java_string(reference)?
            }
            "([C)V" => {
                let array = frame.pop_object_ref()?;
                from_utf16(self.heap.char_array_units(array)?)
            }
            _ => return Ok(false),
        };
        let receiver = frame.pop_object_ref()?;
        self.heap.replace_with_string(receiver, text)?;
        Ok(true)
    }

    fn allocate_text(&mut self, text: &str) -> Value {
        Value::Reference(self.new_java_string(text))
    }

    fn allocate_units(&mut self, units: &[u16]) -> Value {
        Value::Reference(self.new_java_string(from_utf16(units)))
    }

    /// Reads a `String` argument, returning `None` for `null`.
    fn string_argument(
        &self,
        arguments: &[Value],
        index: usize,
        target: &str,
    ) -> JayResult<Option<String>> {
        match arguments.get(index) {
            Some(Value::Null) => Ok(None),
            Some(Value::Reference(reference)) => Ok(Some(self.java_string(*reference)?)),
            other => Err(JayError::new(format!(
                "{target} expected a String argument, found {other:?}"
            ))),
        }
    }

    fn required_string_argument(
        &self,
        arguments: &[Value],
        index: usize,
        target: &str,
    ) -> JayResult<String> {
        self.string_argument(arguments, index, target)?
            .ok_or_else(|| JayError::new(format!("{target} received null")))
    }
}

fn int_argument(arguments: &[Value], index: usize, target: &str) -> JayResult<i32> {
    match arguments.get(index) {
        Some(Value::Int(value)) => Ok(*value),
        other => Err(JayError::new(format!(
            "{target} expected an int argument, found {other:?}"
        ))),
    }
}

fn bool_value(value: bool) -> Value {
    Value::Int(if value { 1 } else { 0 })
}

/// Encodes text as UTF-16 code units, the unit `String` indexes operate on.
pub(super) fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Decodes UTF-16 code units; unpaired surrogates become U+FFFD.
pub(super) fn from_utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// Computes `String.hashCode()` over UTF-16 code units.
pub(super) fn string_hash(units: &[u16]) -> i32 {
    units.iter().fold(0i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(*unit as i32)
    })
}

fn checked_unit(units: &[u16], index: i32) -> JayResult<u16> {
    usize::try_from(index)
        .ok()
        .and_then(|index| units.get(index).copied())
        .ok_or_else(|| {
            JayError::fault(
                "java/lang/StringIndexOutOfBoundsException",
                Some(format!(
                    "Index {index} out of bounds for length {}",
                    units.len()
                )),
            )
        })
}

fn checked_slice(units: &[u16], begin: i32, end: i32) -> JayResult<&[u16]> {
    let length = units.len() as i32;
    if begin < 0 || end > length || begin > end {
        return Err(JayError::fault(
            "java/lang/StringIndexOutOfBoundsException",
            Some(format!("begin {begin}, end {end}, length {length}")),
        ));
    }
    Ok(&units[begin as usize..end as usize])
}

/// Implements `String.compareTo`: the difference of the first differing units,
/// or the length difference when one string is a prefix of the other.
fn compare_units(left: &[u16], right: &[u16]) -> i32 {
    for (l, r) in left.iter().zip(right.iter()) {
        if l != r {
            return *l as i32 - *r as i32;
        }
    }
    left.len() as i32 - right.len() as i32
}

fn find_units(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return (from <= haystack.len()).then_some(from);
    }
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + from)
}

fn index_of_units(haystack: &[u16], needle: &[u16], from: i32) -> i32 {
    let from = from.max(0) as usize;
    find_units(haystack, needle, from).map_or(-1, |index| index as i32)
}

fn last_index_of_units(haystack: &[u16], needle: &[u16]) -> i32 {
    if needle.is_empty() {
        return haystack.len() as i32;
    }
    if needle.len() > haystack.len() {
        return -1;
    }
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
        .map_or(-1, |index| index as i32)
}

fn index_of_unit(units: &[u16], unit: i32, from: i32) -> i32 {
    let from = from.max(0) as usize;
    if from >= units.len() {
        return -1;
    }
    units[from..]
        .iter()
        .position(|candidate| *candidate as i32 == unit)
        .map_or(-1, |index| (index + from) as i32)
}

/// Parses an `int` the way `Integer.parseInt` does: optional sign, decimal digits.
fn parse_java_int(digits: &str) -> JayResult<i32> {
    let body = digits
        .strip_prefix('-')
        .or_else(|| digits.strip_prefix('+'))
        .unwrap_or(digits);
    let invalid = || {
        JayError::fault(
            "java/lang/NumberFormatException",
            Some(format!("For input string: \"{digits}\"")),
        )
    };
    if body.is_empty() || !body.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    digits.parse::<i32>().map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_java_for_ascii_and_supplementary_text() {
        assert_eq!(string_hash(&utf16("hello")), 99162322);
        assert_eq!(string_hash(&utf16("")), 0);
        // U+1F600 is two UTF-16 units; Java hashes both.
        assert_eq!(string_hash(&utf16("\u{1F600}")), 55357 * 31 + 56832);
    }

    #[test]
    fn compare_units_uses_first_difference_then_length() {
        assert_eq!(compare_units(&utf16("apple"), &utf16("banana")), -1);
        assert_eq!(compare_units(&utf16("b"), &utf16("a")), 1);
        assert_eq!(compare_units(&utf16("ab"), &utf16("abc")), -1);
        assert_eq!(compare_units(&utf16("same"), &utf16("same")), 0);
    }

    #[test]
    fn index_searches_follow_java_edge_cases() {
        let text = utf16("hello world");
        assert_eq!(index_of_units(&text, &utf16("o"), 0), 4);
        assert_eq!(index_of_units(&text, &utf16("o"), 5), 7);
        assert_eq!(index_of_units(&text, &utf16(""), 3), 3);
        assert_eq!(index_of_units(&text, &utf16("zzz"), 0), -1);
        assert_eq!(index_of_units(&text, &utf16("h"), -5), 0);
        assert_eq!(index_of_units(&text, &utf16("h"), 50), -1);
        assert_eq!(last_index_of_units(&text, &utf16("o")), 7);
        assert_eq!(last_index_of_units(&text, &utf16("")), 11);
        assert_eq!(index_of_unit(&text, 'w' as i32, 0), 6);
        assert_eq!(index_of_unit(&text, 'w' as i32, 7), -1);
    }

    #[test]
    fn slices_and_units_are_bounds_checked() {
        let text = utf16("abc");
        assert_eq!(checked_unit(&text, 2).unwrap(), 'c' as u16);
        assert!(
            checked_unit(&text, 3)
                .unwrap_err()
                .to_string()
                .contains("Index 3 out of bounds for length 3")
        );
        assert!(checked_unit(&text, -1).is_err());
        assert_eq!(checked_slice(&text, 1, 3).unwrap(), &utf16("bc")[..]);
        assert!(
            checked_slice(&text, 2, 1)
                .unwrap_err()
                .to_string()
                .contains("begin 2, end 1, length 3")
        );
        assert!(checked_slice(&text, 0, 4).is_err());
    }

    #[test]
    fn parse_int_accepts_signs_and_rejects_garbage() {
        assert_eq!(parse_java_int("-123").unwrap(), -123);
        assert_eq!(parse_java_int("+7").unwrap(), 7);
        assert_eq!(parse_java_int("2147483647").unwrap(), i32::MAX);
        for input in ["", "-", "12a", " 1", "2147483648"] {
            assert!(
                parse_java_int(input)
                    .unwrap_err()
                    .to_string()
                    .contains(&format!("For input string: \"{input}\"")),
                "{input}"
            );
        }
    }
}
