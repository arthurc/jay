//! Class, member, and reference-type resolution helpers.

use std::collections::HashSet;
use std::io::Write;

use super::descriptors;
use super::interpreter::Interpreter;
use super::value::Value;
use crate::classfile::{ClassFile, Method};
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    pub(super) fn load_class_file(&self, internal_class_name: &str) -> JayResult<ClassFile> {
        let binary_name = internal_class_name.replace('/', ".");
        let bytes = self.classes.load_class_bytes(&binary_name)?;
        ClassFile::parse(&bytes)
    }

    pub(super) fn resolve_instance_method_class(
        &self,
        receiver_class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> JayResult<ClassFile> {
        self.find_instance_method_class(receiver_class_name, method_name, descriptor)?
            .ok_or_else(|| {
                JayError::new(format!(
                    "invokevirtual target {}.{}{} not found",
                    receiver_class_name.replace('/', "."),
                    method_name,
                    descriptor
                ))
            })
    }

    pub(super) fn find_instance_method_class(
        &self,
        receiver_class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> JayResult<Option<ClassFile>> {
        let mut next_class_name = Some(receiver_class_name.to_string());
        while let Some(class_name) = next_class_name {
            let class_file = self.load_class_file(&class_name)?;
            if class_file.find_method(method_name, descriptor).is_some() {
                return Ok(Some(class_file));
            }
            next_class_name = class_file.super_class.clone();
        }
        Ok(None)
    }

    /// Resolves an instance method reference against the symbolic owner class hierarchy.
    pub(super) fn resolve_instance_method(
        &self,
        owner_class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> JayResult<(ClassFile, Method)> {
        let class_file =
            self.resolve_instance_method_class(owner_class_name, method_name, descriptor)?;
        let method = class_file
            .find_method(method_name, descriptor)
            .ok_or_else(|| {
                JayError::new(format!(
                    "invokevirtual target {}.{}{} not found",
                    owner_class_name.replace('/', "."),
                    method_name,
                    descriptor
                ))
            })?
            .clone();
        Ok((class_file, method))
    }

    /// Resolves an interface method reference against the symbolic owner interface hierarchy.
    pub(super) fn resolve_interface_method(
        &self,
        owner_interface_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> JayResult<(ClassFile, Method)> {
        let mut pending = vec![owner_interface_name.to_string()];
        let mut visited = HashSet::new();
        while let Some(interface_name) = pending.pop() {
            if !visited.insert(interface_name.clone()) {
                continue;
            }

            let class_file = self.load_class_file(&interface_name)?;
            if let Some(method) = class_file.find_method(method_name, descriptor) {
                let method = method.clone();
                return Ok((class_file, method));
            }

            for super_interface in class_file.interfaces.iter().rev() {
                pending.push(super_interface.to_string());
            }
        }

        Err(JayError::new(format!(
            "invokeinterface target {}.{}{} not found",
            owner_interface_name.replace('/', "."),
            method_name,
            descriptor
        )))
    }

    pub(super) fn resolve_field_class(
        &self,
        class_name: &str,
        field_name: &str,
        field_descriptor: &str,
    ) -> JayResult<String> {
        let mut pending_class_names = vec![class_name.to_string()];
        let mut visited = HashSet::new();
        while let Some(candidate_class_name) = pending_class_names.pop() {
            if !visited.insert(candidate_class_name.clone()) {
                continue;
            }
            let class_file = self.load_class_file(&candidate_class_name)?;
            if class_file.has_field(field_name, field_descriptor) {
                return Ok(class_file.this_class);
            }

            if let Some(super_class) = class_file.super_class {
                pending_class_names.push(super_class);
            }

            for interface in class_file.interfaces.iter().rev() {
                pending_class_names.push(interface.to_string());
            }
        }

        Err(JayError::new(format!(
            "field {}.{}:{} not found",
            class_name.replace('/', "."),
            field_name,
            field_descriptor
        )))
    }

    pub(super) fn validate_value_type(
        &self,
        value: &Value,
        expected_type: &descriptors::ValueType,
        target_description: &str,
        action: &str,
    ) -> JayResult<()> {
        if matches!(value, Value::Null)
            && matches!(expected_type, descriptors::ValueType::Reference(_))
        {
            return Ok(());
        }

        if let Some(actual_type) = value.value_type(&self.heap)?
            && self.is_assignable_type(&actual_type, expected_type)?
        {
            return Ok(());
        }

        Err(JayError::new(format!(
            "{target_description} {action} {}, expected {}",
            value.type_name(&self.heap)?,
            expected_type.name()
        )))
    }

    pub(super) fn is_assignable_type(
        &self,
        actual: &descriptors::ValueType,
        expected: &descriptors::ValueType,
    ) -> JayResult<bool> {
        match (actual, expected) {
            (descriptors::ValueType::Int, descriptors::ValueType::Int) => Ok(true),
            (descriptors::ValueType::Float, descriptors::ValueType::Float) => Ok(true),
            (descriptors::ValueType::Long, descriptors::ValueType::Long) => Ok(true),
            (
                descriptors::ValueType::Reference(actual_class),
                descriptors::ValueType::Reference(expected_class),
            ) => self.is_assignable_reference(actual_class, expected_class),
            _ => Ok(false),
        }
    }

    pub(super) fn is_assignable_reference(
        &self,
        actual_class: &str,
        expected_class: &str,
    ) -> JayResult<bool> {
        if actual_class == expected_class || expected_class == "java/lang/Object" {
            return Ok(true);
        }

        if actual_class == "java/lang/String" {
            return Ok(false);
        }

        self.reference_matches_type(actual_class, expected_class, &mut HashSet::new())
    }

    pub(super) fn reference_matches_type(
        &self,
        class_name: &str,
        expected_class: &str,
        visited: &mut HashSet<String>,
    ) -> JayResult<bool> {
        if !visited.insert(class_name.to_string()) {
            return Ok(false);
        }

        let class_file = self.load_class_file(class_name)?;
        if class_file.this_class == expected_class {
            return Ok(true);
        }

        for interface in &class_file.interfaces {
            if interface == expected_class
                || self.reference_matches_type(interface, expected_class, visited)?
            {
                return Ok(true);
            }
        }

        if let Some(super_class) = class_file.super_class {
            return self.reference_matches_type(&super_class, expected_class, visited);
        }

        Ok(false)
    }
}
