//! Descriptor parsing for the subset of JVM types supported by the VM.

use crate::{JayError, JayResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MethodDescriptor {
    pub(super) parameter_types: Vec<ValueType>,
    pub(super) return_type: ReturnType,
}

impl MethodDescriptor {
    pub(super) fn parse(descriptor: &str) -> JayResult<Self> {
        let Some(parameters) = descriptor.strip_prefix('(') else {
            return Err(JayError::new(format!(
                "invalid method descriptor {descriptor}"
            )));
        };
        let Some((parameters, return_type)) = parameters.split_once(')') else {
            return Err(JayError::new(format!(
                "invalid method descriptor {descriptor}"
            )));
        };

        let mut parameter_types = Vec::new();
        let mut remaining_parameters = parameters;
        while !remaining_parameters.is_empty() {
            let (parameter_type, remaining) = parse_value_type(remaining_parameters, descriptor)?;
            parameter_types.push(parameter_type);
            remaining_parameters = remaining;
        }

        let return_type = match return_type {
            "V" => ReturnType::Void,
            _ => ReturnType::Type(parse_complete_value_type(return_type, descriptor)?),
        };

        Ok(Self {
            parameter_types,
            return_type,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReturnType {
    Void,
    Type(ValueType),
}

/// Runtime values currently accepted in method descriptors.
///
/// `Boolean`, `Byte`, `Char`, and `Short` are carried as `Value::Int` at
/// runtime exactly as the JVM does; they are kept distinct here so callers
/// that format values (string concatenation, `println`) can tell them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ValueType {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Float,
    Long,
    /// A reference type name or descriptor, such as `java/lang/String` or `[Ljava/lang/Object;`.
    Reference(String),
}

impl ValueType {
    pub(super) fn name(&self) -> String {
        match self {
            ValueType::Boolean => "boolean".to_string(),
            ValueType::Byte => "byte".to_string(),
            ValueType::Char => "char".to_string(),
            ValueType::Short => "short".to_string(),
            ValueType::Int => "int".to_string(),
            ValueType::Float => "float".to_string(),
            ValueType::Long => "long".to_string(),
            ValueType::Reference(class_name) => class_name.replace('/', "."),
        }
    }

    /// True for every type the JVM stores in an `int` operand slot.
    pub(super) fn is_int_like(&self) -> bool {
        matches!(
            self,
            ValueType::Boolean
                | ValueType::Byte
                | ValueType::Char
                | ValueType::Short
                | ValueType::Int
        )
    }

    pub(super) fn is_reference_to(&self, class_name: &str) -> bool {
        matches!(self, ValueType::Reference(reference) if reference == class_name)
    }
}

impl ReturnType {
    pub(super) fn is_reference_to(&self, class_name: &str) -> bool {
        match self {
            ReturnType::Void => false,
            ReturnType::Type(value_type) => value_type.is_reference_to(class_name),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldType {
    Int,
    Float,
    Long,
    Reference,
}

fn parse_complete_value_type(input: &str, descriptor: &str) -> JayResult<ValueType> {
    let (value_type, remaining) = parse_value_type(input, descriptor)?;
    if !remaining.is_empty() {
        return Err(JayError::new(format!(
            "invalid method descriptor {descriptor}"
        )));
    }
    Ok(value_type)
}

pub(super) fn parse_field_descriptor(descriptor: &str) -> JayResult<FieldType> {
    if matches!(descriptor, "I" | "Z" | "B" | "C" | "S") {
        return Ok(FieldType::Int);
    }

    if descriptor == "F" {
        return Ok(FieldType::Float);
    }

    if descriptor == "J" {
        return Ok(FieldType::Long);
    }

    if descriptor.starts_with('L') && descriptor.ends_with(';') && descriptor.len() > 2 {
        return Ok(FieldType::Reference);
    }

    if is_supported_array_descriptor(descriptor) {
        return Ok(FieldType::Reference);
    }

    if descriptor.starts_with('[') {
        return Err(JayError::new(format!(
            "unsupported array field descriptor {descriptor}"
        )));
    }

    Err(JayError::new(format!(
        "unsupported field descriptor {descriptor}"
    )))
}

fn parse_value_type<'a>(input: &'a str, descriptor: &str) -> JayResult<(ValueType, &'a str)> {
    let int_like = [
        ('I', ValueType::Int),
        ('Z', ValueType::Boolean),
        ('B', ValueType::Byte),
        ('C', ValueType::Char),
        ('S', ValueType::Short),
    ];
    for (letter, value_type) in int_like {
        if let Some(remaining) = input.strip_prefix(letter) {
            return Ok((value_type, remaining));
        }
    }

    if let Some(remaining) = input.strip_prefix('F') {
        return Ok((ValueType::Float, remaining));
    }

    if let Some(remaining) = input.strip_prefix('J') {
        return Ok((ValueType::Long, remaining));
    }

    if let Some(array_type) = input.strip_prefix('[') {
        if let Some(remaining) = array_type.strip_prefix(PRIMITIVE_ARRAY_ELEMENTS) {
            return Ok((ValueType::Reference(input[..2].to_string()), remaining));
        }

        if let Some(reference_type) = array_type.strip_prefix('L') {
            let Some(end_index) = reference_type.find(';') else {
                return Err(JayError::new(format!(
                    "invalid method descriptor {descriptor}"
                )));
            };
            if end_index == 0 {
                return Err(JayError::new(format!(
                    "invalid method descriptor {descriptor}"
                )));
            }

            let array_descriptor = input[..end_index + 3].to_string();
            let remaining = &reference_type[end_index + 1..];
            return Ok((ValueType::Reference(array_descriptor), remaining));
        }

        return Err(JayError::new(format!(
            "unsupported array type in method descriptor {descriptor}"
        )));
    }

    if let Some(reference_type) = input.strip_prefix('L') {
        let Some(end_index) = reference_type.find(';') else {
            return Err(JayError::new(format!(
                "invalid method descriptor {descriptor}"
            )));
        };
        if end_index == 0 {
            return Err(JayError::new(format!(
                "invalid method descriptor {descriptor}"
            )));
        }

        let class_name = reference_type[..end_index].to_string();
        let remaining = &reference_type[end_index + 1..];
        return Ok((ValueType::Reference(class_name), remaining));
    }

    Err(JayError::new(format!(
        "unsupported method descriptor type in {descriptor}"
    )))
}

/// Primitive element letters accepted in array descriptors (`double` is excluded).
const PRIMITIVE_ARRAY_ELEMENTS: [char; 7] = ['Z', 'B', 'C', 'S', 'I', 'J', 'F'];

/// Accepts one-dimensional class arrays (`[Ljava/lang/Object;`) and primitive
/// arrays other than `double[]`. Nested arrays stay rejected.
fn is_supported_array_descriptor(descriptor: &str) -> bool {
    let Some(element_type) = descriptor.strip_prefix('[') else {
        return false;
    };
    if element_type.len() == 1 {
        return element_type
            .chars()
            .all(|letter| PRIMITIVE_ARRAY_ELEMENTS.contains(&letter));
    }
    element_type.starts_with('L') && element_type.ends_with(';') && element_type.len() > 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_field_descriptors() {
        assert_eq!(parse_field_descriptor("I").unwrap(), FieldType::Int);
        assert_eq!(parse_field_descriptor("Z").unwrap(), FieldType::Int);
        assert_eq!(parse_field_descriptor("B").unwrap(), FieldType::Int);
        assert_eq!(parse_field_descriptor("C").unwrap(), FieldType::Int);
        assert_eq!(parse_field_descriptor("S").unwrap(), FieldType::Int);
        assert_eq!(parse_field_descriptor("F").unwrap(), FieldType::Float);
        assert_eq!(parse_field_descriptor("J").unwrap(), FieldType::Long);
        assert_eq!(
            parse_field_descriptor("Ljava/lang/String;").unwrap(),
            FieldType::Reference
        );
        assert_eq!(
            parse_field_descriptor("Lexample/Car;").unwrap(),
            FieldType::Reference
        );
        assert_eq!(
            parse_field_descriptor("[Ljava/lang/Object;").unwrap(),
            FieldType::Reference
        );
        for descriptor in ["[Z", "[B", "[C", "[S", "[I", "[J", "[F"] {
            assert_eq!(
                parse_field_descriptor(descriptor).unwrap(),
                FieldType::Reference,
                "{descriptor}"
            );
        }
    }

    #[test]
    fn parses_primitive_array_method_descriptors() {
        let descriptor = MethodDescriptor::parse("([I[C)[B").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![
                ValueType::Reference("[I".to_string()),
                ValueType::Reference("[C".to_string())
            ]
        );
        assert_eq!(
            descriptor.return_type,
            ReturnType::Type(ValueType::Reference("[B".to_string()))
        );
    }

    #[test]
    fn rejects_unsupported_field_descriptors() {
        for descriptor in ["[D", "[[I", "[[Ljava/lang/String;"] {
            let array_error = parse_field_descriptor(descriptor).unwrap_err();
            assert!(
                array_error
                    .to_string()
                    .contains("unsupported array field descriptor"),
                "{descriptor}: {array_error}"
            );
        }
        assert!(
            MethodDescriptor::parse("([D)V")
                .unwrap_err()
                .to_string()
                .contains("unsupported array type")
        );

        let long_error = parse_field_descriptor("D").unwrap_err();
        assert!(
            long_error
                .to_string()
                .contains("unsupported field descriptor D")
        );
    }

    #[test]
    fn parses_int_returning_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(II)I").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![ValueType::Int, ValueType::Int]
        );
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::Int));
    }

    #[test]
    fn parses_boolean_method_descriptors_as_int_values() {
        let descriptor = MethodDescriptor::parse("(Z)Z").unwrap();

        assert_eq!(descriptor.parameter_types, vec![ValueType::Boolean]);
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::Boolean));

        let descriptor = MethodDescriptor::parse("(BCS)C").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![ValueType::Byte, ValueType::Char, ValueType::Short]
        );
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::Char));
        assert!(
            descriptor
                .parameter_types
                .iter()
                .all(ValueType::is_int_like)
        );
        assert!(!ValueType::Long.is_int_like());
    }

    #[test]
    fn parses_long_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(J)J").unwrap();

        assert_eq!(descriptor.parameter_types, vec![ValueType::Long]);
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::Long));
    }

    #[test]
    fn parses_float_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(F)F").unwrap();

        assert_eq!(descriptor.parameter_types, vec![ValueType::Float]);
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::Float));
    }

    #[test]
    fn parses_void_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(I)V").unwrap();

        assert_eq!(descriptor.parameter_types, vec![ValueType::Int]);
        assert_eq!(descriptor.return_type, ReturnType::Void);
    }

    #[test]
    fn parses_string_returning_method_descriptors() {
        let descriptor = MethodDescriptor::parse("()Ljava/lang/String;").unwrap();

        assert_eq!(descriptor.parameter_types, Vec::new());
        assert_eq!(
            descriptor.return_type,
            ReturnType::Type(ValueType::Reference("java/lang/String".to_string()))
        );
    }

    #[test]
    fn parses_string_parameter_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(Ljava/lang/String;)V").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![ValueType::Reference("java/lang/String".to_string())]
        );
        assert_eq!(descriptor.return_type, ReturnType::Void);
    }

    #[test]
    fn parses_mixed_supported_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(ILjava/lang/String;)V").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![
                ValueType::Int,
                ValueType::Reference("java/lang/String".to_string())
            ]
        );
        assert_eq!(descriptor.return_type, ReturnType::Void);
    }

    #[test]
    fn parses_object_reference_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(Lexample/Box;)Lexample/Box;").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![ValueType::Reference("example/Box".to_string())]
        );
        assert_eq!(
            descriptor.return_type,
            ReturnType::Type(ValueType::Reference("example/Box".to_string()))
        );
    }

    #[test]
    fn parses_object_array_method_descriptors_as_references() {
        let descriptor =
            MethodDescriptor::parse("([Ljava/lang/Object;)[Ljava/lang/Object;").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![ValueType::Reference("[Ljava/lang/Object;".to_string())]
        );
        assert_eq!(
            descriptor.return_type,
            ReturnType::Type(ValueType::Reference("[Ljava/lang/Object;".to_string()))
        );
    }

    #[test]
    fn rejects_double_and_nested_array_method_descriptors() {
        for descriptor in ["([D)V", "([[I)V", "()[[Ljava/lang/String;"] {
            let error = MethodDescriptor::parse(descriptor).unwrap_err();

            assert!(
                error.to_string().contains("unsupported array type"),
                "{descriptor}: {error}"
            );
        }
    }
}
