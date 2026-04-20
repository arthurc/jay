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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ValueType {
    Int,
    Long,
    /// A reference type name or descriptor, such as `java/lang/String` or `[Ljava/lang/Object;`.
    Reference(String),
}

impl ValueType {
    pub(super) fn name(&self) -> String {
        match self {
            ValueType::Int => "int".to_string(),
            ValueType::Long => "long".to_string(),
            ValueType::Reference(class_name) => class_name.replace('/', "."),
        }
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
    if descriptor == "I" || descriptor == "Z" {
        return Ok(FieldType::Int);
    }

    if descriptor == "J" {
        return Ok(FieldType::Long);
    }

    if descriptor.starts_with('L') && descriptor.ends_with(';') && descriptor.len() > 2 {
        return Ok(FieldType::Reference);
    }

    if is_supported_object_array_descriptor(descriptor) {
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

    if let Some(remaining) = input.strip_prefix('Z') {
        return Ok((ValueType::Int, remaining));
    }

    if let Some(remaining) = input.strip_prefix('J') {
        return Ok((ValueType::Long, remaining));
    }

    if let Some(array_type) = input.strip_prefix('[') {
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

fn is_supported_object_array_descriptor(descriptor: &str) -> bool {
    let Some(element_type) = descriptor.strip_prefix('[') else {
        return false;
    };
    element_type.starts_with('L') && element_type.ends_with(';') && element_type.len() > 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_field_descriptors() {
        assert_eq!(parse_field_descriptor("I").unwrap(), FieldType::Int);
        assert_eq!(parse_field_descriptor("Z").unwrap(), FieldType::Int);
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
    }

    #[test]
    fn rejects_unsupported_field_descriptors() {
        let array_error = parse_field_descriptor("[I").unwrap_err();
        assert!(
            array_error
                .to_string()
                .contains("unsupported array field descriptor")
        );

        let long_error = parse_field_descriptor("F").unwrap_err();
        assert!(
            long_error
                .to_string()
                .contains("unsupported field descriptor F")
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

        assert_eq!(descriptor.parameter_types, vec![ValueType::Int]);
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::Int));
    }

    #[test]
    fn parses_long_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(J)J").unwrap();

        assert_eq!(descriptor.parameter_types, vec![ValueType::Long]);
        assert_eq!(descriptor.return_type, ReturnType::Type(ValueType::Long));
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
    fn rejects_primitive_array_method_descriptors() {
        let error = MethodDescriptor::parse("([I)V").unwrap_err();

        assert!(error.to_string().contains("unsupported array type"));
    }
}
