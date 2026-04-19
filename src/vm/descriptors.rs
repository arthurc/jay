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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReturnType {
    Void,
    Type(ValueType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ValueType {
    Int,
    String,
}

impl ValueType {
    pub(super) fn name(self) -> &'static str {
        match self {
            ValueType::Int => "int",
            ValueType::String => "String",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldType {
    Int,
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
    if descriptor == "I" {
        return Ok(FieldType::Int);
    }

    if descriptor.starts_with('L') && descriptor.ends_with(';') && descriptor.len() > 2 {
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
    if let Some(remaining) = input.strip_prefix('I') {
        return Ok((ValueType::Int, remaining));
    }

    if let Some(remaining) = input.strip_prefix("Ljava/lang/String;") {
        return Ok((ValueType::String, remaining));
    }

    if input.starts_with('[') {
        return Err(JayError::new(format!(
            "unsupported array type in method descriptor {descriptor}"
        )));
    }

    if input.starts_with('L') {
        return Err(JayError::new(format!(
            "unsupported object type in method descriptor {descriptor}"
        )));
    }

    Err(JayError::new(format!(
        "unsupported method descriptor type in {descriptor}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_field_descriptors() {
        assert_eq!(parse_field_descriptor("I").unwrap(), FieldType::Int);
        assert_eq!(
            parse_field_descriptor("Ljava/lang/String;").unwrap(),
            FieldType::Reference
        );
        assert_eq!(
            parse_field_descriptor("Lexample/Car;").unwrap(),
            FieldType::Reference
        );
    }

    #[test]
    fn rejects_unsupported_field_descriptors() {
        let array_error = parse_field_descriptor("[I").unwrap_err();
        assert!(
            array_error
                .to_string()
                .contains("unsupported array field descriptor")
        );

        let long_error = parse_field_descriptor("J").unwrap_err();
        assert!(
            long_error
                .to_string()
                .contains("unsupported field descriptor J")
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
    fn parses_void_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(I)V").unwrap();

        assert_eq!(descriptor.parameter_types, vec![ValueType::Int]);
        assert_eq!(descriptor.return_type, ReturnType::Void);
    }

    #[test]
    fn parses_string_returning_method_descriptors() {
        let descriptor = MethodDescriptor::parse("()Ljava/lang/String;").unwrap();

        assert_eq!(descriptor.parameter_types, Vec::new());
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::String));
    }

    #[test]
    fn parses_string_parameter_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(Ljava/lang/String;)V").unwrap();

        assert_eq!(descriptor.parameter_types, vec![ValueType::String]);
        assert_eq!(descriptor.return_type, ReturnType::Void);
    }

    #[test]
    fn parses_mixed_supported_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(ILjava/lang/String;)V").unwrap();

        assert_eq!(
            descriptor.parameter_types,
            vec![ValueType::Int, ValueType::String]
        );
        assert_eq!(descriptor.return_type, ReturnType::Void);
    }

    #[test]
    fn rejects_unsupported_object_method_descriptors() {
        let error = MethodDescriptor::parse("(Ljava/lang/Object;)V").unwrap_err();

        assert!(error.to_string().contains("unsupported object type"));
    }

    #[test]
    fn rejects_array_method_descriptors() {
        let error = MethodDescriptor::parse("([Ljava/lang/String;)V").unwrap_err();

        assert!(error.to_string().contains("unsupported array type"));
    }
}
