//! Core bytecode dispatch loop for the VM interpreter.

use std::collections::{HashMap, HashSet};
use std::io::Write;

use super::bytecode::{
    branch_target, int_branch_taken, int_compare_branch_taken, read_i2, read_u1, read_u2,
};
use super::frame::Frame;
use super::heap::{FieldKey, Heap, ObjectRef};
use super::runtime::checked_array_index;
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
    pub(super) initialized_classes: HashSet<String>,
    pub(super) initializing_classes: HashSet<String>,
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
            initialized_classes: HashSet::new(),
            initializing_classes: HashSet::new(),
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
            let result = self
                .execute_instruction(class_file, code, frame, &mut pc, opcode_pc, opcode)
                .map_err(|error| error.with_java_stack_frame(context.stack_frame(opcode_pc)))?;
            match result {
                InstructionResult::Continue => {}
                InstructionResult::Return(value) => return Ok(value),
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
            0x60 => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                frame.stack.push(Value::Int(left.wrapping_add(right)));
            }
            0x64 => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                frame.stack.push(Value::Int(left.wrapping_sub(right)));
            }
            0x68 => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                frame.stack.push(Value::Int(left.wrapping_mul(right)));
            }
            0x6a => {
                let right = frame.pop_float()?;
                let left = frame.pop_float()?;
                frame.stack.push(Value::Float(left * right));
            }
            0x6c => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                if right == 0 {
                    return Err(JayError::new("integer division by zero"));
                }
                frame.stack.push(Value::Int(left.wrapping_div(right)));
            }
            0x7c => {
                let right = frame.pop_int()? as u32;
                let left = frame.pop_int()? as u32;
                frame
                    .stack
                    .push(Value::Int((left >> (right & 0x1f)) as i32));
            }
            0x7e => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                frame.stack.push(Value::Int(left & right));
            }
            0x82 => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                frame.stack.push(Value::Int(left ^ right));
            }
            0x86 => {
                let value = frame.pop_int()?;
                frame.stack.push(Value::Float(value as f32));
            }
            0x8b => {
                let value = frame.pop_float()?;
                frame.stack.push(Value::Int(value as i32));
            }
            0x96 => {
                let right = frame.pop_float()?;
                let left = frame.pop_float()?;
                let result = if left.is_nan() || right.is_nan() {
                    1
                } else if left > right {
                    1
                } else if left == right {
                    0
                } else {
                    -1
                };
                frame.stack.push(Value::Int(result));
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
            0xb0 => return Ok(InstructionResult::Return(Some(frame.pop_reference()?))),
            0xb1 => return Ok(InstructionResult::Return(None)),
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
            0xc6 | 0xc7 => {
                let offset = read_i2(&code.bytes, pc)?;
                let reference = frame.pop_reference()?;
                let is_null = matches!(reference, Value::Null);
                if (opcode == 0xc6 && is_null) || (opcode == 0xc7 && !is_null) {
                    *pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                }
            }
            0x32 => {
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                let value = self
                    .heap
                    .load_array_reference(reference, checked_array_index(index)?)?;
                frame.stack.push(value);
            }
            0x53 => {
                let value = frame.pop_reference()?;
                let index = frame.pop_int()?;
                let reference = frame.pop_object_ref()?;
                self.heap
                    .store_array_reference(reference, checked_array_index(index)?, value)?;
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
