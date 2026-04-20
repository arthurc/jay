mod bytecode;
mod descriptors;
mod frame;
mod heap;
mod native;
mod value;

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use bytecode::{
    branch_target, int_branch_taken, int_compare_branch_taken, read_i2, read_u1, read_u2,
};
use descriptors::{FieldType, MethodDescriptor, ReturnType, parse_field_descriptor};
use frame::Frame;
use heap::{FieldKey, Heap, ObjectRef};
use value::Value;

use crate::classfile::{ClassFile, Code, Method};
use crate::classpath::ClassResolver;
use crate::{JavaStackFrame, JayError, JayResult};

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
        match interpreter.execute(&class_file, main, code, &mut frame)? {
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
    heap: Heap,
    saved_roots: Vec<Vec<Value>>,
    static_fields: HashMap<FieldKey, Value>,
    initialized_classes: HashSet<String>,
    initializing_classes: HashSet<String>,
}

struct MethodContext<'a> {
    class_name: &'a str,
    method_name: &'a str,
    descriptor: &'a str,
}

impl<'a> MethodContext<'a> {
    fn new(class_file: &'a ClassFile, method: &'a Method) -> Self {
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
    fn new(classes: &'a ClassResolver, output: &'a mut W) -> Self {
        Self {
            classes,
            output,
            heap: Heap::new(),
            saved_roots: Vec::new(),
            static_fields: HashMap::new(),
            initialized_classes: HashSet::new(),
            initializing_classes: HashSet::new(),
        }
    }

    fn execute(
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
            0x16 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.load_long_local(index)?;
            }
            0x19 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.load_reference_local(index)?;
            }
            0x1a..=0x1d => frame.load_int_local((opcode - 0x1a) as usize)?,
            0x1e..=0x21 => frame.load_long_local((opcode - 0x1e) as usize)?,
            0x2a..=0x2d => frame.load_reference_local((opcode - 0x2a) as usize)?,
            0x36 => {
                let index = read_u1(&code.bytes, pc)? as usize;
                frame.store_int_local(index)?;
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
            0x6c => {
                let right = frame.pop_int()?;
                let left = frame.pop_int()?;
                if right == 0 {
                    return Err(JayError::new("integer division by zero"));
                }
                frame.stack.push(Value::Int(left.wrapping_div(right)));
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

    fn new_object(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let class_name = class_file.constant_pool.class_name(index)?;
        let reference = self.heap.allocate_instance(class_name);
        frame.stack.push(Value::Reference(reference));
        self.collect_if_needed(frame);
        Ok(())
    }

    fn new_object_array(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let class_name = class_file.constant_pool.class_name(index)?;
        if class_name != "java/lang/Object" {
            return Err(JayError::new(format!(
                "unsupported anewarray component {}",
                class_name.replace('/', ".")
            )));
        }

        let length = frame.pop_int()?;
        if length < 0 {
            return Err(JayError::new(format!("negative array length {length}")));
        }

        let reference = self.heap.allocate_object_array(length as usize);
        frame.stack.push(Value::Reference(reference));
        self.collect_if_needed(frame);
        Ok(())
    }

    fn load_constant(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let constant_pool = &class_file.constant_pool;
        if let Ok(value) = constant_pool.string(index) {
            let reference = self.heap.allocate_string(value);
            frame.stack.push(Value::Reference(reference));
            self.collect_if_needed(frame);
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

    fn load_wide_constant(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        if let Ok(value) = class_file.constant_pool.long(index) {
            frame.stack.push(Value::Long(value));
            return Ok(());
        }

        Err(JayError::new(format!(
            "unsupported ldc2_w constant at pool index #{index}"
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
            let field_type = parse_field_descriptor(field.descriptor)?;
            let declaring_class_name =
                self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
            self.initialize_class(&declaring_class_name, frame)?;
            let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
            let value = self.static_fields.get(&field_key).cloned();
            match (field_type, value) {
                (FieldType::Int, Some(Value::Int(value))) => {
                    frame.stack.push(Value::Int(value));
                    Ok(())
                }
                (FieldType::Int, None) => {
                    frame.stack.push(Value::Int(0));
                    Ok(())
                }
                (FieldType::Long, Some(Value::Long(value))) => {
                    frame.stack.push(Value::Long(value));
                    Ok(())
                }
                (FieldType::Long, None) => {
                    frame.stack.push(Value::Long(0));
                    Ok(())
                }
                (FieldType::Reference, Some(value @ (Value::Reference(_) | Value::Null))) => {
                    frame.stack.push(value);
                    Ok(())
                }
                (FieldType::Reference, None) => {
                    frame.stack.push(Value::Null);
                    Ok(())
                }
                (_, Some(other)) => Err(JayError::new(format!(
                    "getstatic {}.{}:{} found {}",
                    field.class_name.replace('/', "."),
                    field.name,
                    field.descriptor,
                    other.type_name(&self.heap)?
                ))),
            }
        }
    }

    fn put_static(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        let field_type = parse_field_descriptor(field.descriptor)?;
        let declaring_class_name =
            self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
        self.initialize_class(&declaring_class_name, frame)?;
        let value = frame.pop_field_value(field_type)?;
        let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
        self.static_fields.insert(field_key, value);
        Ok(())
    }

    fn get_field(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        let field_type = parse_field_descriptor(field.descriptor)?;
        let receiver = frame.pop_object_ref()?;
        let declaring_class_name =
            self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
        let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
        let value = self.heap.get_instance_field(receiver, &field_key)?;
        match (field_type, value) {
            (FieldType::Int, Some(Value::Int(value))) => {
                frame.stack.push(Value::Int(value));
                Ok(())
            }
            (FieldType::Int, None) => {
                frame.stack.push(Value::Int(0));
                Ok(())
            }
            (FieldType::Long, Some(Value::Long(value))) => {
                frame.stack.push(Value::Long(value));
                Ok(())
            }
            (FieldType::Long, None) => {
                frame.stack.push(Value::Long(0));
                Ok(())
            }
            (FieldType::Reference, Some(value @ (Value::Reference(_) | Value::Null))) => {
                frame.stack.push(value);
                Ok(())
            }
            (FieldType::Reference, None) => {
                frame.stack.push(Value::Null);
                Ok(())
            }
            (_, Some(other)) => Err(JayError::new(format!(
                "getfield {}.{}:{} found {}",
                field.class_name.replace('/', "."),
                field.name,
                field.descriptor,
                other.type_name(&self.heap)?
            ))),
        }
    }

    fn put_field(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let field = class_file.constant_pool.field_ref(index)?;
        let field_type = parse_field_descriptor(field.descriptor)?;
        let value = frame.pop_field_value(field_type)?;
        let receiver = frame.pop_object_ref()?;
        let declaring_class_name =
            self.resolve_field_class(field.class_name, field.name, field.descriptor)?;
        let field_key = FieldKey::new(&declaring_class_name, field.name, field.descriptor);
        self.heap.put_instance_field(receiver, field_key, value)
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
                    let reference = frame.pop_string_reference(&self.heap)?;
                    frame.pop_print_stream()?;
                    let value = self.heap.string(reference)?;
                    writeln!(self.output, "{value}")?;
                    Ok(())
                }
                "(Ljava/lang/Object;)V" => {
                    let value = frame.pop_reference()?;
                    frame.pop_print_stream()?;
                    let text = self.println_object_text(value)?;
                    writeln!(self.output, "{text}")?;
                    Ok(())
                }
                "(I)V" => {
                    let value = frame.pop_int()?;
                    frame.pop_print_stream()?;
                    writeln!(self.output, "{value}")?;
                    Ok(())
                }
                "(J)V" => {
                    let value = frame.pop_long()?;
                    frame.pop_print_stream()?;
                    writeln!(self.output, "{value}")?;
                    Ok(())
                }
                "(Z)V" => {
                    let value = frame.pop_int()?;
                    frame.pop_print_stream()?;
                    let text = if value == 0 { "false" } else { "true" };
                    writeln!(self.output, "{text}")?;
                    Ok(())
                }
                _ => Err(JayError::new(format!(
                    "unsupported PrintStream.println descriptor {}",
                    method.descriptor
                ))),
            };
        }

        let target_method_name = method.name.to_string();
        let target_descriptor = method.descriptor.to_string();
        let descriptor = MethodDescriptor::parse(&target_descriptor)?;
        let mut arguments = self.pop_method_arguments(
            frame,
            &descriptor,
            &format!(
                "invokevirtual target {}.{}{}",
                method.class_name.replace('/', "."),
                target_method_name,
                target_descriptor
            ),
        )?;
        let receiver = frame.pop_object_ref()?;
        let receiver_class_name = self.heap.instance_class_name(receiver)?.to_string();
        if target_method_name == "toString"
            && target_descriptor == "()Ljava/lang/String;"
            && receiver_class_name == "java/util/Date"
        {
            return self.invoke_date_to_string(frame, receiver);
        }
        if target_method_name == "format"
            && target_descriptor == "(Ljava/util/Date;)Ljava/lang/String;"
            && receiver_class_name == "java/text/SimpleDateFormat"
        {
            return self.invoke_simple_date_format(frame, receiver, &arguments);
        }
        if target_method_name == "setTimeZone"
            && target_descriptor == "(Ljava/util/TimeZone;)V"
            && receiver_class_name == "java/text/SimpleDateFormat"
        {
            return self.invoke_simple_date_format_set_time_zone(receiver, &arguments);
        }
        let (declaring_class_file, declaring_method) = self.resolve_instance_method(
            method.class_name,
            &target_method_name,
            &target_descriptor,
        )?;
        let (target_class_file, target_method) = if declaring_method.is_private() {
            (declaring_class_file, declaring_method)
        } else {
            let class_file = self.resolve_instance_method_class(
                &receiver_class_name,
                &target_method_name,
                &target_descriptor,
            )?;
            let method = class_file
                .find_method(&target_method_name, &target_descriptor)
                .ok_or_else(|| {
                    let target_name = format!(
                        "{}.{}{}",
                        class_file.this_class.replace('/', "."),
                        target_method_name,
                        target_descriptor
                    );
                    JayError::new(format!("invokevirtual target {target_name} not found"))
                })?
                .clone();
            (class_file, method)
        };
        let target_name = format!(
            "{}.{}{}",
            target_class_file.this_class.replace('/', "."),
            target_method_name,
            target_descriptor
        );

        if target_method.is_static() {
            return Err(JayError::new(format!(
                "invokevirtual target {target_name} must not be static"
            )));
        }

        if target_method.access_flags & 0x0100 != 0 || target_method.access_flags & 0x0400 != 0 {
            return Err(JayError::new(format!(
                "invokevirtual target {target_name} must not be native or abstract"
            )));
        }

        let code = target_method
            .code
            .as_ref()
            .ok_or_else(|| {
                JayError::new(format!("invokevirtual target {target_name} has no Code"))
            })?
            .clone();

        arguments.insert(0, Value::Reference(receiver));
        let mut callee = Frame::with_arguments(code.max_locals, arguments)?;
        self.saved_roots
            .push(frame.roots().cloned().collect::<Vec<_>>());
        let result = self.execute(&target_class_file, &target_method, &code, &mut callee);
        self.saved_roots.pop();
        self.complete_call(
            frame,
            descriptor.return_type,
            result?,
            &format!("invokevirtual target {target_name}"),
        )
    }

    fn invoke_special(
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

        if target_method_name != "<init>" {
            return Err(JayError::new(format!(
                "unsupported invokespecial target {target_name}"
            )));
        }

        let descriptor = MethodDescriptor::parse(&target_descriptor)?;
        if descriptor.return_type != ReturnType::Void {
            return Err(JayError::new(format!(
                "invokespecial constructor target {target_name} must return void"
            )));
        }

        if target_class_name == "java/lang/Object" && target_descriptor == "()V" {
            self.pop_constructor_arguments(
                caller,
                &descriptor,
                &format!("invokespecial constructor target {target_name}"),
            )?;
            let _receiver = caller.pop_reference()?;
            return Ok(());
        }

        if target_class_name == "java/text/SimpleDateFormat"
            && target_descriptor == "(Ljava/lang/String;)V"
        {
            return self.invoke_simple_date_format_constructor(caller, &descriptor, &target_name);
        }

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
            .ok_or_else(|| {
                JayError::new(format!("invokespecial target {target_name} not found"))
            })?;

        if method.is_static() {
            return Err(JayError::new(format!(
                "invokespecial constructor target {target_name} must not be static"
            )));
        }

        if method.access_flags & 0x0100 != 0 || method.access_flags & 0x0400 != 0 {
            return Err(JayError::new(format!(
                "invokespecial constructor target {target_name} must not be native or abstract"
            )));
        }

        let code = method
            .code
            .as_ref()
            .ok_or_else(|| {
                JayError::new(format!("invokespecial target {target_name} has no Code"))
            })?
            .clone();

        let mut arguments = self.pop_constructor_arguments(
            caller,
            &descriptor,
            &format!("invokespecial constructor target {target_name}"),
        )?;
        let receiver = caller.pop_reference()?;
        arguments.insert(0, receiver);

        let mut callee = Frame::with_arguments(code.max_locals, arguments)?;
        self.saved_roots
            .push(caller.roots().cloned().collect::<Vec<_>>());
        let result = self.execute(target_class_file, method, &code, &mut callee);
        self.saved_roots.pop();
        match result? {
            None => Ok(()),
            Some(_) => Err(JayError::new(format!(
                "invokespecial constructor target {target_name} returned a value"
            ))),
        }
    }

    fn invoke_dynamic(
        &mut self,
        class_file: &ClassFile,
        frame: &mut Frame,
        index: u16,
    ) -> JayResult<()> {
        let dynamic = class_file.constant_pool.invoke_dynamic(index)?;
        let bootstrap = class_file
            .bootstrap_methods
            .get(dynamic.bootstrap_method_attr_index as usize)
            .ok_or_else(|| {
                JayError::new(format!(
                    "invokedynamic bootstrap method #{} not found",
                    dynamic.bootstrap_method_attr_index
                ))
            })?;
        let method_handle = class_file
            .constant_pool
            .method_handle(bootstrap.method_ref)?;
        if method_handle.reference_kind != 6 {
            return Err(JayError::new(format!(
                "unsupported invokedynamic bootstrap method handle kind {}",
                method_handle.reference_kind
            )));
        }

        let bootstrap_method = class_file
            .constant_pool
            .method_ref(method_handle.reference_index)?;
        if bootstrap_method.class_name != "java/lang/invoke/StringConcatFactory"
            || bootstrap_method.name != "makeConcatWithConstants"
        {
            return Err(JayError::new(format!(
                "unsupported invokedynamic bootstrap {}.{}{}",
                bootstrap_method.class_name, bootstrap_method.name, bootstrap_method.descriptor
            )));
        }

        if dynamic.name != "makeConcatWithConstants" {
            return Err(JayError::new(format!(
                "unsupported invokedynamic call site {}{}",
                dynamic.name, dynamic.descriptor
            )));
        }

        let descriptor = MethodDescriptor::parse(dynamic.descriptor)?;
        if !descriptor.return_type.is_reference_to("java/lang/String") {
            return Err(JayError::new(format!(
                "unsupported invokedynamic return type in {}{}",
                dynamic.name, dynamic.descriptor
            )));
        }

        let [recipe_index] = bootstrap.arguments.as_slice() else {
            return Err(JayError::new(format!(
                "unsupported StringConcatFactory bootstrap argument count {}",
                bootstrap.arguments.len()
            )));
        };
        let recipe = class_file.constant_pool.string(*recipe_index)?.to_string();
        let arguments = self.pop_method_arguments(
            frame,
            &descriptor,
            &format!(
                "invokedynamic call site {}{}",
                dynamic.name, dynamic.descriptor
            ),
        )?;
        let mut text_arguments = Vec::with_capacity(arguments.len());
        for argument in arguments {
            text_arguments.push(self.string_concat_argument(argument)?);
        }

        let value = apply_string_concat_recipe(&recipe, &text_arguments)?;
        let reference = self.heap.allocate_string(value);
        frame.stack.push(Value::Reference(reference));
        self.collect_if_needed(frame);
        Ok(())
    }

    fn invoke_interface(
        &mut self,
        caller_class_file: &ClassFile,
        caller: &mut Frame,
        index: u16,
        count: u8,
    ) -> JayResult<()> {
        if count == 0 {
            return Err(JayError::new(
                "invokeinterface argument count must be nonzero",
            ));
        }

        let method = caller_class_file.constant_pool.method_ref(index)?;
        let target_method_name = method.name.to_string();
        let target_descriptor = method.descriptor.to_string();
        let descriptor = MethodDescriptor::parse(&target_descriptor)?;
        let mut arguments = self.pop_method_arguments(
            caller,
            &descriptor,
            &format!(
                "invokeinterface target {}.{}{}",
                method.class_name.replace('/', "."),
                target_method_name,
                target_descriptor
            ),
        )?;
        let receiver = caller.pop_object_ref()?;
        let receiver_class_name = self.heap.instance_class_name(receiver)?.to_string();
        let (declaring_class_file, declaring_method) = self.resolve_interface_method(
            method.class_name,
            &target_method_name,
            &target_descriptor,
        )?;
        let (target_class_file, target_method) = if declaring_method.is_private() {
            (declaring_class_file, declaring_method)
        } else if let Some(class_file) = self.find_instance_method_class(
            &receiver_class_name,
            &target_method_name,
            &target_descriptor,
        )? {
            let method = class_file
                .find_method(&target_method_name, &target_descriptor)
                .ok_or_else(|| {
                    let target_name = format!(
                        "{}.{}{}",
                        class_file.this_class.replace('/', "."),
                        target_method_name,
                        target_descriptor
                    );
                    JayError::new(format!("invokeinterface target {target_name} not found"))
                })?
                .clone();
            (class_file, method)
        } else {
            (declaring_class_file, declaring_method)
        };
        let target_name = format!(
            "{}.{}{}",
            target_class_file.this_class.replace('/', "."),
            target_method_name,
            target_descriptor
        );

        if target_method.is_static() {
            return Err(JayError::new(format!(
                "invokeinterface target {target_name} must not be static"
            )));
        }

        if target_method.access_flags & 0x0100 != 0 || target_method.access_flags & 0x0400 != 0 {
            return Err(JayError::new(format!(
                "invokeinterface target {target_name} must not be native or abstract"
            )));
        }

        let code = target_method
            .code
            .as_ref()
            .ok_or_else(|| {
                JayError::new(format!("invokeinterface target {target_name} has no Code"))
            })?
            .clone();

        arguments.insert(0, Value::Reference(receiver));
        let mut callee = Frame::with_arguments(code.max_locals, arguments)?;
        self.saved_roots
            .push(caller.roots().cloned().collect::<Vec<_>>());
        let result = self.execute(&target_class_file, &target_method, &code, &mut callee);
        self.saved_roots.pop();
        self.complete_call(
            caller,
            descriptor.return_type,
            result?,
            &format!("invokeinterface target {target_name}"),
        )
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

        if target_class_name == "java/lang/System"
            && target_method_name == "currentTimeMillis"
            && target_descriptor == "()J"
        {
            caller.stack.push(Value::Long(current_time_millis()?));
            return Ok(());
        }
        if target_class_name == "java/util/TimeZone"
            && target_method_name == "getTimeZone"
            && target_descriptor == "(Ljava/lang/String;)Ljava/util/TimeZone;"
        {
            let descriptor = MethodDescriptor::parse(&target_descriptor)?;
            return self.invoke_time_zone_get_time_zone(caller, &descriptor, &target_name);
        }

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

        let arguments = self.pop_method_arguments(
            caller,
            &descriptor,
            &format!("invokestatic target {target_name}"),
        )?;
        let mut callee = Frame::with_arguments(code.max_locals, arguments)?;
        self.saved_roots
            .push(caller.roots().cloned().collect::<Vec<_>>());
        let result = self.execute(target_class_file, method, &code, &mut callee);
        self.saved_roots.pop();
        self.complete_call(
            caller,
            descriptor.return_type,
            result?,
            &format!("invokestatic target {target_name}"),
        )
    }

    fn check_cast(&self, class_file: &ClassFile, frame: &mut Frame, index: u16) -> JayResult<()> {
        let class_name = class_file.constant_pool.class_name(index)?;
        let expected_type = descriptors::ValueType::Reference(class_name.to_string());
        let value = frame.pop_reference()?;
        self.validate_value_type(
            &value,
            &expected_type,
            &format!("checkcast target {}", class_name.replace('/', ".")),
            "checked",
        )?;
        frame.stack.push(value);
        Ok(())
    }

    fn load_class_file(&self, internal_class_name: &str) -> JayResult<ClassFile> {
        let binary_name = internal_class_name.replace('/', ".");
        let bytes = self.classes.load_class_bytes(&binary_name)?;
        ClassFile::parse(&bytes)
    }

    fn invoke_time_zone_get_time_zone(
        &mut self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_name: &str,
    ) -> JayResult<()> {
        let arguments = self.pop_method_arguments(
            caller,
            descriptor,
            &format!("invokestatic target {target_name}"),
        )?;
        let [id] = arguments.as_slice() else {
            return Err(JayError::new(
                "TimeZone.getTimeZone expected one ID argument",
            ));
        };
        let Value::Reference(id) = id else {
            return Err(JayError::new("TimeZone.getTimeZone received null ID"));
        };

        let requested_id = self.heap.string(*id)?.to_string();
        let time_zone = native::TimeZone::from_id(&requested_id);
        let id_reference = self.heap.allocate_string(time_zone.id());
        let reference = self.heap.allocate_instance("java/util/TimeZone");

        self.heap.put_instance_field(
            reference,
            time_zone_id_field(),
            Value::Reference(id_reference),
        )?;
        self.heap.put_instance_field(
            reference,
            time_zone_offset_field(),
            Value::Long(time_zone.offset_millis()),
        )?;
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    fn invoke_date_to_string(&mut self, caller: &mut Frame, receiver: ObjectRef) -> JayResult<()> {
        let fast_time = self.date_fast_time(receiver)?;
        let reference = self.heap.allocate_string(native::date_to_string(fast_time));
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    fn invoke_simple_date_format(
        &mut self,
        caller: &mut Frame,
        receiver: ObjectRef,
        arguments: &[Value],
    ) -> JayResult<()> {
        let [date] = arguments else {
            return Err(JayError::new(
                "SimpleDateFormat.format expected one Date argument",
            ));
        };
        let Value::Reference(date) = date else {
            return Err(JayError::new("SimpleDateFormat.format received null Date"));
        };

        let pattern = self.simple_date_format_pattern(receiver)?;
        let time_zone = self.simple_date_format_time_zone(receiver)?;
        let fast_time = self.date_fast_time(*date)?;
        let output = native::format_simple_date(&pattern, fast_time, time_zone)?;
        let reference = self.heap.allocate_string(output);
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    fn invoke_simple_date_format_set_time_zone(
        &mut self,
        receiver: ObjectRef,
        arguments: &[Value],
    ) -> JayResult<()> {
        let [time_zone] = arguments else {
            return Err(JayError::new(
                "SimpleDateFormat.setTimeZone expected one TimeZone argument",
            ));
        };
        let Value::Reference(time_zone) = time_zone else {
            return Err(JayError::new(
                "SimpleDateFormat.setTimeZone received null TimeZone",
            ));
        };

        self.heap.put_instance_field(
            receiver,
            simple_date_format_time_zone_field(),
            Value::Reference(*time_zone),
        )
    }

    fn invoke_simple_date_format_constructor(
        &mut self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_name: &str,
    ) -> JayResult<()> {
        let arguments = self.pop_constructor_arguments(
            caller,
            descriptor,
            &format!("invokespecial constructor target {target_name}"),
        )?;
        let [pattern] = arguments.as_slice() else {
            return Err(JayError::new(
                "SimpleDateFormat constructor expected one pattern argument",
            ));
        };
        let Value::Reference(pattern) = pattern else {
            return Err(JayError::new(
                "SimpleDateFormat constructor received null pattern",
            ));
        };
        let receiver = caller.pop_object_ref()?;
        let field = FieldKey::new(
            "java/text/SimpleDateFormat",
            "pattern",
            "Ljava/lang/String;",
        );
        self.heap
            .put_instance_field(receiver, field, Value::Reference(*pattern))
    }

    fn date_fast_time(&self, date: ObjectRef) -> JayResult<i64> {
        let field = FieldKey::new("java/util/Date", "fastTime", "J");
        match self.heap.get_instance_field(date, &field)? {
            Some(Value::Long(value)) => Ok(value),
            None => Ok(0),
            Some(other) => Err(JayError::new(format!(
                "java.util.Date.fastTime found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    fn simple_date_format_pattern(&self, formatter: ObjectRef) -> JayResult<String> {
        let field = FieldKey::new(
            "java/text/SimpleDateFormat",
            "pattern",
            "Ljava/lang/String;",
        );
        match self.heap.get_instance_field(formatter, &field)? {
            Some(Value::Reference(reference)) => Ok(self.heap.string(reference)?.to_string()),
            Some(Value::Null) | None => Err(JayError::new(
                "SimpleDateFormat pattern has not been initialized",
            )),
            Some(other) => Err(JayError::new(format!(
                "SimpleDateFormat pattern found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    fn simple_date_format_time_zone(&self, formatter: ObjectRef) -> JayResult<native::TimeZone> {
        match self
            .heap
            .get_instance_field(formatter, &simple_date_format_time_zone_field())?
        {
            Some(Value::Reference(reference)) => self.time_zone(reference),
            Some(Value::Null) | None => Ok(native::TimeZone::gmt()),
            Some(other) => Err(JayError::new(format!(
                "SimpleDateFormat timeZone found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    fn time_zone(&self, reference: ObjectRef) -> JayResult<native::TimeZone> {
        let id = match self
            .heap
            .get_instance_field(reference, &time_zone_id_field())?
        {
            Some(Value::Reference(id)) => self.heap.string(id)?.to_string(),
            Some(Value::Null) | None => {
                return Err(JayError::new("TimeZone ID has not been initialized"));
            }
            Some(other) => {
                return Err(JayError::new(format!(
                    "TimeZone ID found {}",
                    other.type_name(&self.heap)?
                )));
            }
        };

        let offset_millis = match self
            .heap
            .get_instance_field(reference, &time_zone_offset_field())?
        {
            Some(Value::Long(value)) => value,
            None => return Err(JayError::new("TimeZone offset has not been initialized")),
            Some(other) => {
                return Err(JayError::new(format!(
                    "TimeZone offset found {}",
                    other.type_name(&self.heap)?
                )));
            }
        };

        Ok(native::TimeZone::resolved(id, offset_millis))
    }

    fn resolve_instance_method_class(
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

    fn find_instance_method_class(
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
    fn resolve_instance_method(
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
    fn resolve_interface_method(
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

    fn resolve_field_class(
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

    fn pop_method_arguments(
        &self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_description: &str,
    ) -> JayResult<Vec<Value>> {
        let mut arguments = Vec::with_capacity(descriptor.parameter_types.len());
        for parameter_type in descriptor.parameter_types.iter().rev() {
            let value = caller.pop_value_of_type(parameter_type)?;
            self.validate_value_type(&value, parameter_type, target_description, "received")?;
            arguments.push(value);
        }
        arguments.reverse();
        Ok(arguments)
    }

    /// Pops two operand-stack words, matching JVM `pop2` semantics.
    ///
    /// If the first popped value is a category-2 value (`long` in this VM), the
    /// instruction is complete. Otherwise, this pops and discards a second
    /// category-1 value.
    fn pop_two_words(&self, frame: &mut Frame) -> JayResult<()> {
        let first = frame.pop()?;
        if !matches!(first, Value::Long(_)) {
            let second = frame.pop()?;
            if matches!(second, Value::Long(_)) {
                return Err(JayError::new(
                    "invalid pop2 operand shape: category-1 value over category-2 value",
                ));
            }
        }
        Ok(())
    }

    fn complete_call(
        &self,
        caller: &mut Frame,
        return_type: ReturnType,
        result: Option<Value>,
        target_description: &str,
    ) -> JayResult<()> {
        match (return_type, result) {
            (ReturnType::Void, None) => Ok(()),
            (ReturnType::Void, Some(_)) => Err(JayError::new(format!(
                "{target_description} returned a value from void method"
            ))),
            (ReturnType::Type(descriptors::ValueType::Reference(_)), Some(Value::Null)) => {
                caller.stack.push(Value::Null);
                Ok(())
            }
            (ReturnType::Type(return_type), Some(value)) => {
                if let Some(actual_type) = value.value_type(&self.heap)?
                    && self.is_assignable_type(&actual_type, &return_type)?
                {
                    caller.stack.push(value);
                    Ok(())
                } else {
                    Err(JayError::new(format!(
                        "{target_description} returned {}, expected {}",
                        value.type_name(&self.heap)?,
                        return_type.name()
                    )))
                }
            }
            (ReturnType::Type(return_type), None) => Err(JayError::new(format!(
                "{target_description} returned void from {} method",
                return_type.name()
            ))),
        }
    }

    fn string_concat_argument(&self, value: Value) -> JayResult<String> {
        match value {
            Value::Null => Ok("null".to_string()),
            Value::Int(value) => Ok(value.to_string()),
            Value::Reference(reference) => Ok(self.heap.string(reference)?.to_string()),
            other => Err(JayError::new(format!(
                "unsupported string concat argument {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    /// Formats the focused subset of values supported by PrintStream.println(Object).
    fn println_object_text(&self, value: Value) -> JayResult<String> {
        match value {
            Value::Null => Ok("null".to_string()),
            Value::Reference(reference) => match self.heap.value_type(reference)? {
                Some(descriptors::ValueType::Reference(class_name)) => match class_name.as_str() {
                    "java/lang/String" => Ok(self.heap.string(reference)?.to_string()),
                    "java/util/Date" => Ok(native::date_to_string(self.date_fast_time(reference)?)),
                    _ => Err(JayError::new(format!(
                        "unsupported PrintStream.println(Object) value {}",
                        self.heap.type_name(reference)?
                    ))),
                },
                _ => Err(JayError::new(format!(
                    "unsupported PrintStream.println(Object) value {}",
                    self.heap.type_name(reference)?
                ))),
            },
            other => Err(JayError::new(format!(
                "unsupported PrintStream.println(Object) value {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    fn pop_constructor_arguments(
        &self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_description: &str,
    ) -> JayResult<Vec<Value>> {
        let mut arguments = Vec::with_capacity(descriptor.parameter_types.len());
        for parameter_type in descriptor.parameter_types.iter().rev() {
            let value = caller.pop_value_of_type(parameter_type)?;
            self.validate_value_type(&value, parameter_type, target_description, "received")?;
            arguments.push(value);
        }
        arguments.reverse();
        Ok(arguments)
    }

    fn validate_value_type(
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

    fn is_assignable_type(
        &self,
        actual: &descriptors::ValueType,
        expected: &descriptors::ValueType,
    ) -> JayResult<bool> {
        match (actual, expected) {
            (descriptors::ValueType::Int, descriptors::ValueType::Int) => Ok(true),
            (descriptors::ValueType::Long, descriptors::ValueType::Long) => Ok(true),
            (
                descriptors::ValueType::Reference(actual_class),
                descriptors::ValueType::Reference(expected_class),
            ) => self.is_assignable_reference(actual_class, expected_class),
            _ => Ok(false),
        }
    }

    fn is_assignable_reference(&self, actual_class: &str, expected_class: &str) -> JayResult<bool> {
        if actual_class == expected_class || expected_class == "java/lang/Object" {
            return Ok(true);
        }

        if actual_class == "java/lang/String" {
            return Ok(false);
        }

        self.reference_matches_type(actual_class, expected_class, &mut HashSet::new())
    }

    fn reference_matches_type(
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

    fn collect_if_needed(&mut self, current_frame: &Frame) {
        if !self.heap.should_collect() {
            return;
        }

        let roots = self
            .saved_roots
            .iter()
            .flatten()
            .cloned()
            .chain(self.static_fields.values().cloned())
            .chain(current_frame.roots().cloned())
            .collect::<Vec<_>>();
        self.heap.collect(roots.iter());
    }

    fn initialize_class(&mut self, class_name: &str, current_frame: &Frame) -> JayResult<()> {
        if self.initialized_classes.contains(class_name)
            || self.initializing_classes.contains(class_name)
        {
            return Ok(());
        }

        let class_file = self.load_class_file(class_name)?;
        self.initializing_classes.insert(class_name.to_string());
        let result = if let Some(super_class) = class_file.super_class.as_deref() {
            self.initialize_class(super_class, current_frame)
                .and_then(|_| self.execute_class_initializer(&class_file, current_frame))
        } else {
            self.execute_class_initializer(&class_file, current_frame)
        };
        self.initializing_classes.remove(class_name);
        result?;
        self.initialized_classes.insert(class_name.to_string());
        Ok(())
    }

    fn execute_class_initializer(
        &mut self,
        class_file: &ClassFile,
        current_frame: &Frame,
    ) -> JayResult<()> {
        let Some(method) = class_file.find_method("<clinit>", "()V") else {
            return Ok(());
        };

        if !method.is_static() {
            return Err(JayError::new(format!(
                "class initializer for {} must be static",
                class_file.this_class.replace('/', ".")
            )));
        }

        let code = method.code.as_ref().ok_or_else(|| {
            JayError::new(format!(
                "class initializer for {} has no Code",
                class_file.this_class.replace('/', ".")
            ))
        })?;

        let mut frame = Frame::new(code.max_locals);
        self.saved_roots
            .push(current_frame.roots().cloned().collect::<Vec<_>>());
        let result = self.execute(class_file, method, code, &mut frame);
        self.saved_roots.pop();
        match result? {
            None => Ok(()),
            Some(_) => Err(JayError::new(format!(
                "class initializer for {} returned a value",
                class_file.this_class.replace('/', ".")
            ))),
        }
    }
}

fn simple_date_format_time_zone_field() -> FieldKey {
    FieldKey::new(
        "java/text/SimpleDateFormat",
        "__jay_timeZone",
        "Ljava/util/TimeZone;",
    )
}

fn time_zone_id_field() -> FieldKey {
    FieldKey::new("java/util/TimeZone", "__jay_id", "Ljava/lang/String;")
}

fn time_zone_offset_field() -> FieldKey {
    FieldKey::new("java/util/TimeZone", "__jay_offsetMillis", "J")
}

fn checked_array_index(index: i32) -> JayResult<usize> {
    usize::try_from(index).map_err(|_| JayError::new(format!("negative array index {index}")))
}

fn current_time_millis() -> JayResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| JayError::new(format!("system time is before Unix epoch: {error}")))?;
    i64::try_from(duration.as_millis())
        .map_err(|_| JayError::new("current time milliseconds exceed long range"))
}

fn apply_string_concat_recipe(recipe: &str, arguments: &[String]) -> JayResult<String> {
    let mut output = String::new();
    let mut argument_index = 0usize;
    for character in recipe.chars() {
        match character {
            '\u{0001}' => {
                let Some(argument) = arguments.get(argument_index) else {
                    return Err(JayError::new(
                        "StringConcatFactory recipe has more placeholders than arguments",
                    ));
                };
                output.push_str(argument);
                argument_index += 1;
            }
            '\u{0002}' => {
                return Err(JayError::new(
                    "StringConcatFactory constant placeholders are unsupported",
                ));
            }
            _ => output.push(character),
        }
    }

    if argument_index != arguments.len() {
        return Err(JayError::new(
            "StringConcatFactory recipe has fewer placeholders than arguments",
        ));
    }

    Ok(output)
}
