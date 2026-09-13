//! Core bytecode dispatch loop for the VM interpreter.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::rc::Rc;

use super::arithmetic;
use super::bytecode::{
    branch_target, int_branch_taken, int_compare_branch_taken, lookupswitch_target, read_i2,
    read_i4, read_u1, read_u2, tableswitch_target,
};
use super::frame::Frame;
use super::heap::{FieldKey, Heap, ObjectRef};
use super::value::Value;
use crate::classfile::{ClassFile, Code, Method};
use crate::classpath::ClassResolver;
use crate::{JavaStackFrame, JayError, JayResult};

pub(super) struct Interpreter<'a, W: Write> {
    pub(super) classes: &'a ClassResolver,
    pub(super) output: &'a mut W,
    pub(super) heap: Heap,
    pub(super) saved_roots: Vec<Vec<Value>>,
    pub(super) static_fields: HashMap<FieldKey, Value>,
    /// Heap-allocated `java.lang.Class` mirrors loaded by class literals.
    pub(super) class_mirrors: HashMap<String, ObjectRef>,
    /// Canonical `String` objects for literals and `String.intern()`, keyed by text.
    pub(super) interned_strings: HashMap<String, ObjectRef>,
    pub(super) initialized_classes: HashSet<String>,
    pub(super) initializing_classes: HashSet<String>,
    /// Parsed class files keyed by internal name, so each class is parsed once per run.
    pub(super) class_cache: RefCell<HashMap<String, Rc<ClassFile>>>,
}

struct MethodContext<'a> {
    class_name: &'a str,
    method_name: &'a str,
    descriptor: &'a str,
}

impl<'a> MethodContext<'a> {
    pub(super) fn new(class_file: &'a ClassFile, method: &'a Method) -> Self {
        Self {
            class_name: &class_file.this_class,
            method_name: &method.name,
            descriptor: &method.descriptor,
        }
    }

    fn stack_frame(&self, pc: usize) -> JavaStackFrame {
        JavaStackFrame::new(
            self.class_name.replace('/', "."),
            self.method_name,
            self.descriptor,
            pc,
        )
    }
}

enum InstructionResult {
    Continue,
    Return(Option<Value>),
}

impl<'a, W: Write> Interpreter<'a, W> {
    pub(super) fn new(classes: &'a ClassResolver, output: &'a mut W) -> Self {
        Self {
            classes,
            output,
            heap: Heap::new(),
            saved_roots: Vec::new(),
            static_fields: HashMap::new(),
            class_mirrors: HashMap::new(),
            interned_strings: HashMap::new(),
            initialized_classes: HashSet::new(),
            initializing_classes: HashSet::new(),
            class_cache: RefCell::new(HashMap::new()),
        }
    }

    pub(super) fn execute(
        &mut self,
        class_file: &ClassFile,
        method: &Method,
        code: &Code,
        frame: &mut Frame,
    ) -> JayResult<Option<Value>> {
        let context = MethodContext::new(class_file, method);
        let mut pc = 0usize;
        while pc < code.bytes.len() {
            let opcode_pc = pc;
            let opcode = read_u1(&code.bytes, &mut pc)
                .map_err(|error| error.with_java_stack_frame(context.stack_frame(opcode_pc)))?;
            match self.execute_instruction(class_file, code, frame, &mut pc, opcode_pc, opcode) {
                Ok(InstructionResult::Continue) => {}
                Ok(InstructionResult::Return(value)) => return Ok(value),
                Err(error) if error.is_java_exception() => {
                    let (exception, error) = self.exception_object(error)?;
                    match self.find_handler(code, opcode_pc, exception)? {
                        Some(handler_pc) => {
                            // The handler starts with only the exception on the stack.
                            frame.stack.clear();
                            frame.stack.push(Value::Reference(exception));
                            pc = handler_pc;
                        }
                        None => {
                            return Err(error.with_java_stack_frame(context.stack_frame(opcode_pc)));
                        }
                    }
                }
                Err(error) => {
                    return Err(error.with_java_stack_frame(context.stack_frame(opcode_pc)));
                }
            }
        }

        Err(JayError::new("main method completed without return")
            .with_java_stack_frame(context.stack_frame(pc)))
    }

    fn execute_instruction(
        &mut self,
        class_file: &ClassFile,
        code: &Code,
        frame: &mut Frame,
        pc: &mut usize,
        opcode_pc: usize,
        opcode: u8,
    ) -> JayResult<InstructionResult> {
        match opcode {
            0x00 => {}
            0x01 => frame.stack.push(Value::Null),
            0x02 => frame.stack.push(Value::Int(-1)),
            0x03 => frame.stack.push(Value::Int(0)),
            0x04 => frame.stack.push(Value::Int(1)),
            0x05 => frame.stack.push(Value::Int(2)),
            0x06 => frame.stack.push(Value::Int(3)),
            0x07 => frame.stack.push(Value::Int(4)),
            0x08 => frame.stack.push(Value::Int(5)),
            0x09 => frame.stack.push(Value::Long(0)),
            0x0a => frame.stack.push(Value::Long(1)),
            0x10 => {
                let value = read_u1(&code.bytes, pc)? as i8 as i32;
                frame.stack.push(Value::Int(value));
            }
            0x11 => {
                let value = read_u2(&code.bytes, pc)? as i16 as i32;
                frame.stack.push(Value::Int(value));
            }
            0x12 => {
                let index = read_u1(&code.bytes, pc)? as u16;
                self.load_constant(class_file, frame, index)?;
            }
            0x13 => {
                let index = read_u2(&code.bytes, pc)?;
                self.load_constant(class_file, frame, index)?;
            }
            0x14 => {
                let index = read_u2(&code.bytes, pc)?;
                self.load_wide_constant(class_file, frame, index)?;
            }
            0x15 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.load_int_local(index)?;
            }
            0x17 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.load_float_local(index)?;
            }
            0x16 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.load_long_local(index)?;
            }
            0x19 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.load_reference_local(index)?;
            }
            0x1a..=0x1d => frame.load_int_local((opcode - 0x1a) as usize)?,
            0x22..=0x25 => frame.load_float_local((opcode - 0x22) as usize)?,
            0x1e..=0x21 => frame.load_long_local((opcode - 0x1e) as usize)?,
            0x2a..=0x2d => frame.load_reference_local((opcode - 0x2a) as usize)?,
            0x36 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.store_int_local(index)?;
            }
            0x38 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.store_float_local(index)?;
            }
            0x37 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.store_long_local(index)?;
            }
            0x3a => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.store_reference_local(index)?;
            }
            0x3b..=0x3e => frame.store_int_local((opcode - 0x3b) as usize)?,
            0x43..=0x46 => frame.store_float_local((opcode - 0x43) as usize)?,
            0x3f..=0x42 => frame.store_long_local((opcode - 0x3f) as usize)?,
            0x4b..=0x4e => frame.store_reference_local((opcode - 0x4b) as usize)?,
            0x57 => {
                let _ = frame.pop()?;
            }
            0x58 => self.pop_two_words(frame)?,
            0x59 => frame.duplicate_top()?,
            0x5a => frame.duplicate_top_insert_two_down()?,
            0x5b => frame.duplicate_top_insert_three_down()?,
            0x5c => frame.duplicate_top_two()?,
            0x5d => frame.duplicate_top_two_insert_three_down()?,
            0x5f => frame.swap_top_two()?,
            0x60 | 0x64 | 0x68 | 0x6c | 0x70 | 0x78 | 0x7a | 0x7c | 0x7e | 0x80 | 0x82 => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                frame
                    .stack
                    .push(Value::Int(arithmetic::int_binary(opcode, left, right)?));
            }
            0x61 | 0x65 | 0x69 | 0x6d | 0x71 | 0x7f | 0x81 | 0x83 => {
                let right = frame.pop_long()?;
                let left = frame.pop_long()?;
                frame
                    .stack
                    .push(Value::Long(arithmetic::long_binary(opcode, left, right)?));
            }
            0x79 | 0x7b | 0x7d => {
                let count = frame.pop_int()?;
                let value = frame.pop_long()?;
                frame
                    .stack
                    .push(Value::Long(arithmetic::long_shift(opcode, value, count)?));
            }
            0x6a => {
                let right = frame.pop_float()?;
                let left = frame.pop_float()?;
                frame.stack.push(Value::Float(left * right));
            }
            0x74 => {
                let value = frame.pop_int()?;
                frame.stack.push(Value::Int(value.wrapping_neg()));
            }
            0x75 => {
                let value = frame.pop_long()?;
                frame.stack.push(Value::Long(value.wrapping_neg()));
            }
            0x85 => {
                let value = frame.pop_int()?;
                frame.stack.push(Value::Long(value as i64));
            }
            0x86 => {
                let value = frame.pop_int()?;
                frame.stack.push(Value::Float(value as f32));
            }
            0x88 => {
                let value = frame.pop_long()?;
                frame.stack.push(Value::Int(value as i32));
            }
            0x89 => {
                let value = frame.pop_long()?;
                frame.stack.push(Value::Float(value as f32));
            }
            0x8b => {
                let value = frame.pop_float()?;
                frame.stack.push(Value::Int(value as i32));
            }
            0x91..=0x93 => {
                let value = frame.pop_int()?;
                frame
                    .stack
                    .push(Value::Int(arithmetic::narrow_int(opcode, value)?));
            }
            0x94 => {
                let right = frame.pop_long()?;
                let left = frame.pop_long()?;
                frame
                    .stack
                    .push(Value::Int(arithmetic::long_compare(left, right)));
            }
            0x95 | 0x96 => {
                let right = frame.pop_float()?;
                let left = frame.pop_float()?;
                let nan_result = if opcode == 0x95 { -1 } else { 1 };
                frame.stack.push(Value::Int(arithmetic::float_compare(
                    left, right, nan_result,
                )));
            }
            0x84 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                let value = read_u1(&code.bytes, pc)? as i8 as i32;
                frame.increment_int_local(index, value)?;
            }
            0x99..=0x9e => {
                let offset = read_i2(&code.bytes, pc)?;
                let value = frame.pop_int()?;
                if int_branch_taken(opcode, value)? {
                    *pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                }
            }
            0x9f..=0xa4 => {
                let offset = read_i2(&code.bytes, pc)?;
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                if int_compare_branch_taken(opcode, left, right)? {
                    *pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                }
            }
            0xa5 | 0xa6 => {
                let offset = read_i2(&code.bytes, pc)?;
                let right = frame.pop_reference()?;
                let left = frame.pop_reference()?;
                let equal = frame.references_equal(&left, &right)?;
                if (opcode == 0xa5 && equal) || (opcode == 0xa6 && !equal) {
                    *pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                }
            }
            0xa7 => {
                let offset = read_i2(&code.bytes, pc)?;
                *pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
            }
            0xaa => {
                let key = frame.pop_int()?;
                *pc = tableswitch_target(&code.bytes, opcode_pc, pc, key)?;
            }
            0xab => {
                let key = frame.pop_int()?;
                *pc = lookupswitch_target(&code.bytes, opcode_pc, pc, key)?;
            }
            0xc8 => {
                let offset = read_i4(&code.bytes, pc)?;
                *pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
            }
            0xac => {
                return Ok(InstructionResult::Return(Some(Value::Int(
                    frame.pop_int()?,
                ))));
            }
            0xad => {
                return Ok(InstructionResult::Return(Some(Value::Long(
                    frame.pop_long()?,
                ))));
            }
            0xae => {
                return Ok(InstructionResult::Return(Some(Value::Float(
                    frame.pop_float()?,
                ))));
            }
            0xb0 => return Ok(InstructionResult::Return(Some(frame.pop_reference()?))),
            0xb1 => return Ok(InstructionResult::Return(None)),
            0xbf => {
                let exception = frame.pop_object_ref()?;
                let display = self.exception_display(exception)?;
                return Err(JayError::thrown(exception.index(), display));
            }
            0xb2 => {
                let index = read_u2(&code.bytes, pc)?;
                self.get_static(class_file, frame, index)?;
            }
            0xb3 => {
                let index = read_u2(&code.bytes, pc)?;
                self.put_static(class_file, frame, index)?;
            }
            0xb4 => {
                let index = read_u2(&code.bytes, pc)?;
                self.get_field(class_file, frame, index)?;
            }
            0xb5 => {
                let index = read_u2(&code.bytes, pc)?;
                self.put_field(class_file, frame, index)?;
            }
            0xb6 => {
                let index = read_u2(&code.bytes, pc)?;
                self.invoke_virtual(class_file, frame, index)?;
            }
            0xb7 => {
                let index = read_u2(&code.bytes, pc)?;
                self.invoke_special(class_file, frame, index)?;
            }
            0xb8 => {
                let index = read_u2(&code.bytes, pc)?;
                self.invoke_static(class_file, frame, index)?;
            }
            0xb9 => {
                let index = read_u2(&code.bytes, pc)?;
                let count = read_u1(&code.bytes, pc)?;
                let zero = read_u1(&code.bytes, pc)?;
                if zero != 0 {
                    return Err(JayError::new(format!(
                        "invokeinterface at pc {opcode_pc} has nonzero padding"
                    )));
                }
                self.invoke_interface(class_file, frame, index, count)?;
            }
            0xba => {
                let index = read_u2(&code.bytes, pc)?;
                let zero = read_u2(&code.bytes, pc)?;
                if zero != 0 {
                    return Err(JayError::new(format!(
                        "invokedynamic at pc {opcode_pc} has nonzero padding"
                    )));
                }
                self.invoke_dynamic(class_file, frame, index)?;
            }
            0xbb => {
                let index = read_u2(&code.bytes, pc)?;
                self.new_object(class_file, frame, index)?;
            }
            0xbc => {
                let atype = read_u1(&code.bytes, pc)?;
                self.new_primitive_array(frame, atype)?;
            }
            0xbd => {
                let index = read_u2(&code.bytes, pc)?;
                self.new_object_array(class_file, frame, index)?;
            }
            0xbe => {
                let reference = frame.pop_object_ref()?;
                let length = self.heap.array_length(reference)?;
                let length = i32::try_from(length)
                    .map_err(|_| JayError::new("array length exceeds int range"))?;
                frame.stack.push(Value::Int(length));
            }
            0xc0 => {
                let index = read_u2(&code.bytes, pc)?;
                self.check_cast(class_file, frame, index)?;
            }
            0xc1 => {
                let index = read_u2(&code.bytes, pc)?;
                self.instance_of(class_file, frame, index)?;
            }
            0xc6 | 0xc7 => {
                let offset = read_i2(&code.bytes, pc)?;
                let reference = frame.pop_reference()?;
                let is_null = matches!(reference, Value::Null);
                if (opcode == 0xc6 && is_null) || (opcode == 0xc7 && !is_null) {
                    *pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                }
            }
            0x2e | 0x2f | 0x30 | 0x33 | 0x34 | 0x35 => {
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                let index = self.checked_array_index(reference, index)?;
                let value = self.heap.load_primitive(reference, index)?;
                frame.stack.push(value);
            }
            0x32 => {
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                let index = self.checked_array_index(reference, index)?;
                let value = self.heap.load_array_reference(reference, index)?;
                frame.stack.push(value);
            }
            0x4f | 0x54 | 0x55 | 0x56 => {
                let value = frame.pop_int()?;
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                let index = self.checked_array_index(reference, index)?;
                self.heap
                    .store_primitive(reference, index, Value::Int(value))?;
            }
            0x50 => {
                let value = frame.pop_long()?;
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                let index = self.checked_array_index(reference, index)?;
                self.heap
                    .store_primitive(reference, index, Value::Long(value))?;
            }
            0x51 => {
                let value = frame.pop_float()?;
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                let index = self.checked_array_index(reference, index)?;
                self.heap
                    .store_primitive(reference, index, Value::Float(value))?;
            }
            0x53 => {
                let value = frame.pop_reference()?;
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                let index = self.checked_array_index(reference, index)?;
                self.validate_reference_array_store(reference, &value)?;
                self.heap.store_array_reference(reference, index, value)?;
            }
            _ => {
                return Err(JayError::new(format!(
                    "unsupported bytecode 0x{opcode:02x} at pc {opcode_pc}"
                )));
            }
        }
        Ok(InstructionResult::Continue)
    }
}

impl<'a, W: Write> Interpreter<'a, W> {
    fn validate_reference_array_store(&self, array: ObjectRef, value: &Value) -> JayResult<()> {
        let Value::Reference(stored_reference) = value else {
            return Ok(());
        };

        if self.is_array_store_compatible(array, *stored_reference)? {
            return Ok(());
        }

        Err(JayError::fault(
            "java/lang/ArrayStoreException",
            Some(self.heap.type_name(*stored_reference)?),
        ))
    }

    /// Checks whether `stored_reference` may be stored into the reference array `array`.
    pub(super) fn is_array_store_compatible(
        &self,
        array: ObjectRef,
        stored_reference: ObjectRef,
    ) -> JayResult<bool> {
        let actual = self.reference_type_name(stored_reference)?;
        let descriptor = self.heap.array_descriptor(array)?.to_string();
        let Some(expected_component) = descriptor.strip_prefix('[') else {
            return Err(JayError::new(format!(
                "malformed reference array descriptor {descriptor}"
            )));
        };
        let expected = expected_component
            .strip_prefix('L')
            .and_then(|component| component.strip_suffix(';'))
            .unwrap_or(expected_component);

        self.is_reference_compatible(&actual, expected)
    }

    /// Implements `instanceof`: pushes 1 when the popped reference is non-null
    /// and assignable to the constant-pool class, otherwise 0.
    pub(super) fn instance_of(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let expected = class_file.constant_pool.class_name(index)?;
        let value = frame.pop_reference()?;
        let result = match value {
            Value::Reference(reference) => {
                let actual = self.reference_type_name(reference)?;
                self.is_reference_compatible(&actual, expected)?
            }
            _ => false,
        };
        frame.stack.push(Value::Int(if result { 1 } else { 0 }));
        Ok(())
    }

    /// Returns the runtime type of a heap reference as an internal class name or array descriptor.
    pub(super) fn reference_type_name(&self, reference: ObjectRef) -> JayResult<String> {
        match self.heap.value_type(reference)? {
            Some(super::descriptors::ValueType::Reference(reference_type)) => Ok(reference_type),
            Some(other) => Err(JayError::new(format!(
                "reference had unexpected type {}",
                other.name()
            ))),
            None => Err(JayError::new("reference type is unavailable")),
        }
    }

    /// Checks whether a runtime type (class name or array descriptor) is assignable
    /// to a constant-pool type (class name or array descriptor).
    ///
    /// Arrays are assignable to `Object`, `Cloneable`, `Serializable`, and to an
    /// array type whose component the element type is assignable to. Class types
    /// are never assignable to array types.
    pub(super) fn is_reference_compatible(&self, actual: &str, expected: &str) -> JayResult<bool> {
        match (actual.strip_prefix('['), expected.strip_prefix('[')) {
            (None, None) => self.is_assignable_reference(actual, expected),
            (Some(_), None) => Ok(matches!(
                expected,
                "java/lang/Object" | "java/lang/Cloneable" | "java/io/Serializable"
            )),
            (None, Some(_)) => Ok(false),
            (Some(actual_component), Some(expected_component)) => {
                if actual_component == expected_component {
                    return Ok(true);
                }
                match (
                    strip_class_component(actual_component),
                    strip_class_component(expected_component),
                ) {
                    (Some(actual_class), Some(expected_class)) => {
                        self.is_assignable_reference(actual_class, expected_class)
                    }
                    (Some(_), None) | (None, Some(_)) => {
                        // Mixed primitive/class components never match, but a nested
                        // array component may still be assignable to Object[].
                        if actual_component.starts_with('[') {
                            self.is_reference_compatible(actual_component, expected_component)
                        } else {
                            Ok(false)
                        }
                    }
                    (None, None) => Ok(false),
                }
            }
        }
    }
}

/// Extracts `java/lang/String` from the array component `Ljava/lang/String;`.
fn strip_class_component(component: &str) -> Option<&str> {
    component.strip_prefix('L')?.strip_suffix(';')
}
