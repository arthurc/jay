//! Interpreter heap-root tracking and class initialization lifecycle.

use std::io::Write;

use super::frame::Frame;
use super::heap::ObjectRef;
use super::interpreter::Interpreter;
use super::value::Value;
use crate::classfile::ClassFile;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    pub(super) fn collect_if_needed(&mut self, current_frame: &Frame) {
        if !self.heap.should_collect() {
            return;
        }

        let roots = self
            .saved_roots
            .iter()
            .flatten()
            .cloned()
            .chain(self.static_fields.values().cloned())
            .chain(self.class_mirrors.values().copied().map(Value::Reference))
            .chain(current_frame.roots().cloned())
            .collect::<Vec<_>>();
        self.heap.collect(roots.iter());
    }

    pub(super) fn class_mirror(&mut self, class_name: &str) -> ObjectRef {
        if let Some(reference) = self.class_mirrors.get(class_name) {
            return *reference;
        }

        // Class literals load a Class mirror without running the represented class initializer.
        let reference = self.heap.allocate_instance("java/lang/Class");
        self.class_mirrors.insert(class_name.to_string(), reference);
        reference
    }

    pub(super) fn initialize_class(
        &mut self,
        class_name: &str,
        current_frame: &Frame,
    ) -> JayResult<()> {
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

    pub(super) fn execute_class_initializer(
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
