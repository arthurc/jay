//! Method invocation bytecode handlers and call setup.

use std::io::Write;

use super::descriptors::{MethodDescriptor, ReturnType};
use super::frame::Frame;
use super::interpreter::Interpreter;
use super::native_runtime::current_time_millis;
use super::runtime::apply_string_concat_recipe;
use super::value::Value;
use crate::classfile::ClassFile;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    pub(super) fn invoke_virtual(
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

        if method.class_name == "java/lang/Class"
            && method.name == "desiredAssertionStatus"
            && method.descriptor == "()Z"
        {
            let receiver = frame.pop_object_ref()?;
            let receiver_class_name = self.heap.instance_class_name(receiver)?;
            if receiver_class_name != "java/lang/Class" {
                return Err(JayError::new(format!(
                    "Class.desiredAssertionStatus receiver was {}",
                    self.heap.type_name(receiver)?
                )));
            }

            frame.stack.push(Value::Int(0));
            return Ok(());
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

    pub(super) fn invoke_special(
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

    pub(super) fn invoke_dynamic(
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

    pub(super) fn invoke_interface(
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

    pub(super) fn invoke_static(
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
        if target_class_name == "java/time/LocalDateTime"
            && target_method_name == "now"
            && target_descriptor == "()Ljava/time/LocalDateTime;"
        {
            return self.invoke_local_date_time_now(caller);
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

        if target_class_name == "java/lang/System"
            && target_method_name == "registerNatives"
            && target_descriptor == "()V"
        {
            // HotSpot uses this to register VM natives; Jay dispatches supported
            // native behavior through explicit Rust shims, so there is no table to populate.
            return Ok(());
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
}
