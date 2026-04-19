use std::io::{self, Write};
use std::path::PathBuf;

use crate::classfile::{ClassFile, Code};
use crate::classpath::ClassResolver;
use crate::{JayError, JayResult};

#[derive(Debug, Clone)]
pub struct Vm {
    classes: ClassResolver,
}

impl Vm {
    pub fn new(classpath: PathBuf) -> JayResult<Self> {
        Ok(Self {
            classes: ClassResolver::new(classpath)?,
        })
    }

    pub fn run_main(&self, main_class: &str) -> JayResult<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        self.run_main_to_writer(main_class, &mut handle)
    }

    pub fn run_main_to_writer<W: Write>(&self, main_class: &str, output: &mut W) -> JayResult<()> {
        let bytes = self.classes.load_class_bytes(main_class)?;
        let class_file = ClassFile::parse(&bytes)?;
        let main = class_file
            .find_method("main", "([Ljava/lang/String;)V")
            .or_else(|| class_file.find_method("main", "()V"))
            .ok_or_else(|| JayError::new(format!("main method not found in {main_class}")))?;

        if !main.is_public() || !main.is_static() {
            return Err(JayError::new(format!(
                "main method in {main_class} must be public static"
            )));
        }

        let code = main
            .code
            .as_ref()
            .ok_or_else(|| JayError::new(format!("main method in {main_class} has no Code")))?;

        let mut interpreter = Interpreter::new(&self.classes, output);
        let mut frame = Frame::new(code.max_locals);
        match interpreter.execute(&class_file, code, &mut frame)? {
            None => Ok(()),
            Some(_) => Err(JayError::new(format!(
                "main method in {main_class} returned a value"
            ))),
        }
    }
}

struct Interpreter<'a, W: Write> {
    classes: &'a ClassResolver,
    output: &'a mut W,
}

struct Frame {
    stack: Vec<Value>,
    locals: Vec<Value>,
}

impl<'a, W: Write> Interpreter<'a, W> {
    fn new(classes: &'a ClassResolver, output: &'a mut W) -> Self {
        Self { classes, output }
    }

    fn execute(
        &mut self,
        class_file: &ClassFile,
        code: &Code,
        frame: &mut Frame,
    ) -> JayResult<Option<Value>> {
        let mut pc = 0usize;
        while pc < code.bytes.len() {
            let opcode_pc = pc;
            let opcode = read_u1(&code.bytes, &mut pc)?;
            match opcode {
                0x00 => {}
                0x02 => frame.stack.push(Value::Int(-1)),
                0x03 => frame.stack.push(Value::Int(0)),
                0x04 => frame.stack.push(Value::Int(1)),
                0x05 => frame.stack.push(Value::Int(2)),
                0x06 => frame.stack.push(Value::Int(3)),
                0x07 => frame.stack.push(Value::Int(4)),
                0x08 => frame.stack.push(Value::Int(5)),
                0x10 => {
                    let value = read_u1(&code.bytes, &mut pc)? as i8 as i32;
                    frame.stack.push(Value::Int(value));
                }
                0x11 => {
                    let value = read_u2(&code.bytes, &mut pc)? as i16 as i32;
                    frame.stack.push(Value::Int(value));
                }
                0x12 => {
                    let index = read_u1(&code.bytes, &mut pc)? as u16;
                    self.load_constant(class_file, frame, index)?;
                }
                0x13 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.load_constant(class_file, frame, index)?;
                }
                0x1a..=0x1d => frame.load_int_local((opcode - 0x1a) as usize)?,
                0x3b..=0x3e => frame.store_int_local((opcode - 0x3b) as usize)?,
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
                0x6c => {
                    let right = frame.pop_int()?;
                    let left = frame.pop_int()?;
                    if right == 0 {
                        return Err(JayError::new("integer division by zero"));
                    }
                    frame.stack.push(Value::Int(left.wrapping_div(right)));
                }
                0x84 => {
                    let index = read_u1(&code.bytes, &mut pc)? as usize;
                    let value = read_u1(&code.bytes, &mut pc)? as i8 as i32;
                    frame.increment_int_local(index, value)?;
                }
                0x99..=0x9e => {
                    let offset = read_i2(&code.bytes, &mut pc)?;
                    let value = frame.pop_int()?;
                    if int_branch_taken(opcode, value)? {
                        pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                    }
                }
                0x9f..=0xa4 => {
                    let offset = read_i2(&code.bytes, &mut pc)?;
                    let right = frame.pop_int()?;
                    let left = frame.pop_int()?;
                    if int_compare_branch_taken(opcode, left, right)? {
                        pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                    }
                }
                0xa7 => {
                    let offset = read_i2(&code.bytes, &mut pc)?;
                    pc = branch_target(code.bytes.len(), opcode_pc, offset)?;
                }
                0xac => return Ok(Some(Value::Int(frame.pop_int()?))),
                0xb1 => return Ok(None),
                0xb2 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.get_static(class_file, frame, index)?;
                }
                0xb6 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.invoke_virtual(class_file, frame, index)?;
                }
                0xb8 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.invoke_static(class_file, frame, index)?;
                }
                _ => {
                    return Err(JayError::new(format!(
                        "unsupported bytecode 0x{opcode:02x} at pc {opcode_pc}"
                    )));
                }
            }
        }

        Err(JayError::new("main method completed without return"))
    }

    fn load_constant(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let constant_pool = &class_file.constant_pool;
        if let Ok(value) = constant_pool.string(index) {
            frame.stack.push(Value::String(value.to_string()));
            return Ok(());
        }

        if let Ok(value) = constant_pool.integer(index) {
            frame.stack.push(Value::Int(value));
            return Ok(());
        }

        Err(JayError::new(format!(
            "unsupported ldc constant at pool index #{index}"
        )))
    }

    fn get_static(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        if field.class_name == "java/lang/System"
            && field.name == "out"
            && field.descriptor == "Ljava/io/PrintStream;"
        {
            frame.stack.push(Value::PrintStream);
            Ok(())
        } else {
            Err(JayError::new(format!(
                "unsupported getstatic {}.{}:{}",
                field.class_name, field.name, field.descriptor
            )))
        }
    }

    fn invoke_virtual(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let method = class_file.constant_pool.method_ref(index)?;
        if method.class_name == "java/io/PrintStream" && method.name == "println" {
            return match method.descriptor {
                "(Ljava/lang/String;)V" => {
                    let value = frame.pop_string()?;
                    frame.pop_print_stream()?;
                    writeln!(self.output, "{value}")?;
                    Ok(())
                }
                "(I)V" => {
                    let value = frame.pop_int()?;
                    frame.pop_print_stream()?;
                    writeln!(self.output, "{value}")?;
                    Ok(())
                }
                _ => Err(JayError::new(format!(
                    "unsupported PrintStream.println descriptor {}",
                    method.descriptor
                ))),
            };
        }

        Err(JayError::new(format!(
            "unsupported invokevirtual {}.{}{}",
            method.class_name, method.name, method.descriptor
        )))
    }

    fn invoke_static(
        &mut self,
        caller_class_file: &ClassFile,
        caller: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let method_ref = caller_class_file.constant_pool.method_ref(index)?;
        let target_class_name = method_ref.class_name.to_string();
        let target_method_name = method_ref.name.to_string();
        let target_descriptor = method_ref.descriptor.to_string();
        let target_name = format!(
            "{}.{}{}",
            target_class_name.replace('/', "."),
            target_method_name,
            target_descriptor
        );

        let descriptor = MethodDescriptor::parse(&target_descriptor)?;
        let loaded_class_file;
        let target_class_file = if target_class_name == caller_class_file.this_class {
            caller_class_file
        } else {
            let binary_name = target_class_name.replace('/', ".");
            let bytes = self.classes.load_class_bytes(&binary_name)?;
            loaded_class_file = ClassFile::parse(&bytes)?;
            &loaded_class_file
        };
        let method = target_class_file
            .find_method(&target_method_name, &target_descriptor)
            .ok_or_else(|| JayError::new(format!("invokestatic target {target_name} not found")))?;

        if !method.is_static() {
            return Err(JayError::new(format!(
                "invokestatic target {target_name} must be static"
            )));
        }

        if method.access_flags & 0x0100 != 0 || method.access_flags & 0x0400 != 0 {
            return Err(JayError::new(format!(
                "invokestatic target {target_name} must not be native or abstract"
            )));
        }

        let code = method
            .code
            .as_ref()
            .ok_or_else(|| JayError::new(format!("invokestatic target {target_name} has no Code")))?
            .clone();

        let mut arguments = Vec::with_capacity(descriptor.parameter_count);
        for _ in 0..descriptor.parameter_count {
            arguments.push(Value::Int(caller.pop_int()?));
        }
        arguments.reverse();

        let mut callee = Frame::with_arguments(code.max_locals, arguments)?;
        let result = self.execute(target_class_file, &code, &mut callee)?;
        match (descriptor.return_type, result) {
            (ReturnType::Void, None) => Ok(()),
            (ReturnType::Void, Some(_)) => Err(JayError::new(format!(
                "invokestatic target {target_name} returned a value from void method"
            ))),
            (ReturnType::Int, Some(Value::Int(value))) => {
                caller.stack.push(Value::Int(value));
                Ok(())
            }
            (ReturnType::Int, Some(other)) => Err(JayError::new(format!(
                "invokestatic target {target_name} returned non-int value {other:?}"
            ))),
            (ReturnType::Int, None) => Err(JayError::new(format!(
                "invokestatic target {target_name} returned void from int method"
            ))),
        }
    }
}

impl Frame {
    fn new(max_locals: u16) -> Self {
        Self {
            stack: Vec::new(),
            locals: vec![Value::Uninitialized; max_locals as usize],
        }
    }

    fn with_arguments(max_locals: u16, arguments: Vec<Value>) -> JayResult<Self> {
        let mut frame = Self::new(max_locals);
        if arguments.len() > frame.locals.len() {
            return Err(JayError::new(format!(
                "method expects {} argument locals but max locals is {}",
                arguments.len(),
                frame.locals.len()
            )));
        }

        for (index, value) in arguments.into_iter().enumerate() {
            frame.locals[index] = value;
        }

        Ok(frame)
    }

    fn load_int_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.local_int(index)?;
        self.stack.push(Value::Int(value));
        Ok(())
    }

    fn store_int_local(&mut self, index: usize) -> JayResult<()> {
        let value = self.pop_int()?;
        let slot = self.local_slot_mut(index)?;
        *slot = Value::Int(value);
        Ok(())
    }

    fn increment_int_local(&mut self, index: usize, value: i32) -> JayResult<()> {
        let current = self.local_int(index)?;
        let slot = self.local_slot_mut(index)?;
        *slot = Value::Int(current.wrapping_add(value));
        Ok(())
    }

    fn local_int(&self, index: usize) -> JayResult<i32> {
        match self.local_slot(index)? {
            Value::Int(value) => Ok(*value),
            Value::Uninitialized => Err(JayError::new(format!(
                "local variable #{index} is uninitialized"
            ))),
            other => Err(JayError::new(format!(
                "expected int in local variable #{index}, found {other:?}"
            ))),
        }
    }

    fn local_slot(&self, index: usize) -> JayResult<&Value> {
        self.locals.get(index).ok_or_else(|| {
            JayError::new(format!(
                "invalid local variable index #{index}; max locals {}",
                self.locals.len()
            ))
        })
    }

    fn local_slot_mut(&mut self, index: usize) -> JayResult<&mut Value> {
        let max_locals = self.locals.len();
        self.locals.get_mut(index).ok_or_else(|| {
            JayError::new(format!(
                "invalid local variable index #{index}; max locals {max_locals}"
            ))
        })
    }

    fn pop_print_stream(&mut self) -> JayResult<()> {
        match self.pop()? {
            Value::PrintStream => Ok(()),
            other => Err(JayError::new(format!(
                "expected PrintStream receiver on stack, found {other:?}"
            ))),
        }
    }

    fn pop_string(&mut self) -> JayResult<String> {
        match self.pop()? {
            Value::String(value) => Ok(value),
            other => Err(JayError::new(format!(
                "expected string on stack, found {other:?}"
            ))),
        }
    }

    fn pop_int(&mut self) -> JayResult<i32> {
        match self.pop()? {
            Value::Int(value) => Ok(value),
            other => Err(JayError::new(format!(
                "expected int on stack, found {other:?}"
            ))),
        }
    }

    fn pop(&mut self) -> JayResult<Value> {
        self.stack
            .pop()
            .ok_or_else(|| JayError::new("operand stack underflow"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MethodDescriptor {
    parameter_count: usize,
    return_type: ReturnType,
}

impl MethodDescriptor {
    fn parse(descriptor: &str) -> JayResult<Self> {
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

        let mut parameter_count = 0;
        for parameter in parameters.chars() {
            if parameter != 'I' {
                return Err(JayError::new(format!(
                    "unsupported method descriptor parameter {parameter} in {descriptor}"
                )));
            }
            parameter_count += 1;
        }

        let return_type = match return_type {
            "I" => ReturnType::Int,
            "V" => ReturnType::Void,
            _ => {
                return Err(JayError::new(format!(
                    "unsupported method descriptor return type {return_type} in {descriptor}"
                )));
            }
        };

        Ok(Self {
            parameter_count,
            return_type,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnType {
    Int,
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Uninitialized,
    Int(i32),
    String(String),
    PrintStream,
}

fn read_u1(bytes: &[u8], pc: &mut usize) -> JayResult<u8> {
    if *pc >= bytes.len() {
        return Err(JayError::new("unexpected end of bytecode"));
    }
    let value = bytes[*pc];
    *pc += 1;
    Ok(value)
}

fn read_u2(bytes: &[u8], pc: &mut usize) -> JayResult<u16> {
    let high = read_u1(bytes, pc)? as u16;
    let low = read_u1(bytes, pc)? as u16;
    Ok((high << 8) | low)
}

fn read_i2(bytes: &[u8], pc: &mut usize) -> JayResult<i16> {
    Ok(read_u2(bytes, pc)? as i16)
}

fn branch_target(code_len: usize, opcode_pc: usize, offset: i16) -> JayResult<usize> {
    let target = opcode_pc as i64 + offset as i64;
    if target < 0 || target >= code_len as i64 {
        return Err(JayError::new(format!(
            "branch target {target} out of bytecode range 0..{code_len}"
        )));
    }

    Ok(target as usize)
}

fn int_branch_taken(opcode: u8, value: i32) -> JayResult<bool> {
    match opcode {
        0x99 => Ok(value == 0),
        0x9a => Ok(value != 0),
        0x9b => Ok(value < 0),
        0x9c => Ok(value >= 0),
        0x9d => Ok(value > 0),
        0x9e => Ok(value <= 0),
        _ => Err(JayError::new(format!(
            "unsupported integer branch opcode 0x{opcode:02x}"
        ))),
    }
}

fn int_compare_branch_taken(opcode: u8, left: i32, right: i32) -> JayResult<bool> {
    match opcode {
        0x9f => Ok(left == right),
        0xa0 => Ok(left != right),
        0xa1 => Ok(left < right),
        0xa2 => Ok(left >= right),
        0xa3 => Ok(left > right),
        0xa4 => Ok(left <= right),
        _ => Err(JayError::new(format!(
            "unsupported integer comparison branch opcode 0x{opcode:02x}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_target_uses_opcode_pc_and_signed_offsets() {
        assert_eq!(branch_target(20, 5, 14).unwrap(), 19);
        assert_eq!(branch_target(20, 15, -10).unwrap(), 5);
    }

    #[test]
    fn branch_target_rejects_out_of_range_targets() {
        let before_start = branch_target(10, 0, -1).unwrap_err();
        assert!(
            before_start
                .to_string()
                .contains("branch target -1 out of bytecode range")
        );

        let at_end = branch_target(10, 8, 2).unwrap_err();
        assert!(
            at_end
                .to_string()
                .contains("branch target 10 out of bytecode range")
        );
    }

    #[test]
    fn integer_zero_branch_predicates_match_jvm_conditions() {
        assert!(int_branch_taken(0x99, 0).unwrap());
        assert!(!int_branch_taken(0x99, 1).unwrap());

        assert!(int_branch_taken(0x9a, 1).unwrap());
        assert!(!int_branch_taken(0x9a, 0).unwrap());

        assert!(int_branch_taken(0x9b, -1).unwrap());
        assert!(!int_branch_taken(0x9b, 0).unwrap());

        assert!(int_branch_taken(0x9c, 0).unwrap());
        assert!(!int_branch_taken(0x9c, -1).unwrap());

        assert!(int_branch_taken(0x9d, 1).unwrap());
        assert!(!int_branch_taken(0x9d, 0).unwrap());

        assert!(int_branch_taken(0x9e, 0).unwrap());
        assert!(!int_branch_taken(0x9e, 1).unwrap());
    }

    #[test]
    fn integer_comparison_branch_predicates_match_jvm_conditions() {
        assert!(int_compare_branch_taken(0x9f, 2, 2).unwrap());
        assert!(!int_compare_branch_taken(0x9f, 2, 3).unwrap());

        assert!(int_compare_branch_taken(0xa0, 2, 3).unwrap());
        assert!(!int_compare_branch_taken(0xa0, 2, 2).unwrap());

        assert!(int_compare_branch_taken(0xa1, 2, 3).unwrap());
        assert!(!int_compare_branch_taken(0xa1, 3, 2).unwrap());

        assert!(int_compare_branch_taken(0xa2, 3, 2).unwrap());
        assert!(int_compare_branch_taken(0xa2, 2, 2).unwrap());
        assert!(!int_compare_branch_taken(0xa2, 2, 3).unwrap());

        assert!(int_compare_branch_taken(0xa3, 3, 2).unwrap());
        assert!(!int_compare_branch_taken(0xa3, 2, 2).unwrap());

        assert!(int_compare_branch_taken(0xa4, 2, 3).unwrap());
        assert!(int_compare_branch_taken(0xa4, 2, 2).unwrap());
        assert!(!int_compare_branch_taken(0xa4, 3, 2).unwrap());
    }

    #[test]
    fn parses_int_returning_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(II)I").unwrap();

        assert_eq!(descriptor.parameter_count, 2);
        assert_eq!(descriptor.return_type, ReturnType::Int);
    }

    #[test]
    fn parses_void_method_descriptors() {
        let descriptor = MethodDescriptor::parse("(I)V").unwrap();

        assert_eq!(descriptor.parameter_count, 1);
        assert_eq!(descriptor.return_type, ReturnType::Void);
    }

    #[test]
    fn rejects_unsupported_method_descriptors() {
        let error = MethodDescriptor::parse("(Ljava/lang/String;)V").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unsupported method descriptor parameter")
        );
    }
}
