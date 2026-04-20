mod bytecode;
mod descriptors;
mod frame;
mod heap;
mod value;

use std::io::{self, Write};
use std::path::PathBuf;

use bytecode::{
    branch_target, int_branch_taken, int_compare_branch_taken, read_i2, read_u1, read_u2,
};
use descriptors::{FieldType, MethodDescriptor, ReturnType, parse_field_descriptor};
use frame::Frame;
use heap::{FieldKey, Heap};
use value::Value;

use crate::classfile::{ClassFile, Code, Method};
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
    heap: Heap,
    saved_roots: Vec<Vec<Value>>,
}

impl<'a, W: Write> Interpreter<'a, W> {
    fn new(classes: &'a ClassResolver, output: &'a mut W) -> Self {
        Self {
            classes,
            output,
            heap: Heap::new(),
            saved_roots: Vec::new(),
        }
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
                0x15 => {
                    let index = read_u1(&code.bytes, &mut pc)? as usize;
                    frame.load_int_local(index)?;
                }
                0x19 => {
                    let index = read_u1(&code.bytes, &mut pc)? as usize;
                    frame.load_reference_local(index)?;
                }
                0x1a..=0x1d => frame.load_int_local((opcode - 0x1a) as usize)?,
                0x2a..=0x2d => frame.load_reference_local((opcode - 0x2a) as usize)?,
                0x36 => {
                    let index = read_u1(&code.bytes, &mut pc)? as usize;
                    frame.store_int_local(index)?;
                }
                0x3a => {
                    let index = read_u1(&code.bytes, &mut pc)? as usize;
                    frame.store_reference_local(index)?;
                }
                0x3b..=0x3e => frame.store_int_local((opcode - 0x3b) as usize)?,
                0x4b..=0x4e => frame.store_reference_local((opcode - 0x4b) as usize)?,
                0x57 => {
                    let _ = frame.pop()?;
                }
                0x59 => frame.duplicate_top()?,
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
                0xb0 => return Ok(Some(frame.pop_reference()?)),
                0xb1 => return Ok(None),
                0xb2 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.get_static(class_file, frame, index)?;
                }
                0xb4 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.get_field(class_file, frame, index)?;
                }
                0xb5 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.put_field(class_file, frame, index)?;
                }
                0xb6 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.invoke_virtual(class_file, frame, index)?;
                }
                0xb7 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.invoke_special(class_file, frame, index)?;
                }
                0xb8 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.invoke_static(class_file, frame, index)?;
                }
                0xba => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    let zero = read_u2(&code.bytes, &mut pc)?;
                    if zero != 0 {
                        return Err(JayError::new(format!(
                            "invokedynamic at pc {opcode_pc} has nonzero padding"
                        )));
                    }
                    self.invoke_dynamic(class_file, frame, index)?;
                }
                0xbb => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.new_object(class_file, frame, index)?;
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
            self.resolve_instance_field_class(field.class_name, field.name, field.descriptor)?;
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
            (FieldType::Reference, Some(value @ Value::Reference(_))) => {
                frame.stack.push(value);
                Ok(())
            }
            (FieldType::Reference, None) => Err(JayError::new(format!(
                "getfield {}.{}:{} is unset; null references are unsupported",
                field.class_name.replace('/', "."),
                field.name,
                field.descriptor
            ))),
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
            self.resolve_instance_field_class(field.class_name, field.name, field.descriptor)?;
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
        let result = self.execute(&target_class_file, &code, &mut callee);
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
        let result = self.execute(target_class_file, &code, &mut callee);
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

        let arguments = self.pop_method_arguments(
            caller,
            &descriptor,
            &format!("invokestatic target {target_name}"),
        )?;
        let mut callee = Frame::with_arguments(code.max_locals, arguments)?;
        self.saved_roots
            .push(caller.roots().cloned().collect::<Vec<_>>());
        let result = self.execute(target_class_file, &code, &mut callee);
        self.saved_roots.pop();
        self.complete_call(
            caller,
            descriptor.return_type,
            result?,
            &format!("invokestatic target {target_name}"),
        )
    }

    fn load_class_file(&self, internal_class_name: &str) -> JayResult<ClassFile> {
        let binary_name = internal_class_name.replace('/', ".");
        let bytes = self.classes.load_class_bytes(&binary_name)?;
        ClassFile::parse(&bytes)
    }

    fn resolve_instance_method_class(
        &self,
        receiver_class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> JayResult<ClassFile> {
        let mut next_class_name = Some(receiver_class_name.to_string());
        while let Some(class_name) = next_class_name {
            let class_file = self.load_class_file(&class_name)?;
            if class_file.find_method(method_name, descriptor).is_some() {
                return Ok(class_file);
            }
            next_class_name = class_file.super_class.clone();
        }

        Err(JayError::new(format!(
            "invokevirtual target {}.{}{} not found",
            receiver_class_name.replace('/', "."),
            method_name,
            descriptor
        )))
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

    fn resolve_instance_field_class(
        &self,
        class_name: &str,
        field_name: &str,
        field_descriptor: &str,
    ) -> JayResult<String> {
        let mut next_class_name = Some(class_name.to_string());
        while let Some(candidate_class_name) = next_class_name {
            let class_file = self.load_class_file(&candidate_class_name)?;
            if class_file.has_field(field_name, field_descriptor) {
                return Ok(class_file.this_class);
            }
            next_class_name = class_file.super_class;
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
            Value::Int(value) => Ok(value.to_string()),
            Value::Reference(reference) => Ok(self.heap.string(reference)?.to_string()),
            other => Err(JayError::new(format!(
                "unsupported string concat argument {}",
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

        let mut next_class_name = Some(actual_class.to_string());
        while let Some(class_name) = next_class_name {
            let class_file = self.load_class_file(&class_name)?;
            if class_file.this_class == expected_class {
                return Ok(true);
            }
            next_class_name = class_file.super_class;
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
            .chain(current_frame.roots().cloned())
            .collect::<Vec<_>>();
        self.heap.collect(roots.iter());
    }
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
