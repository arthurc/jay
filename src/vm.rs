mod arithmetic;
mod bytecode;
mod descriptors;
mod fields;
mod frame;
mod heap;
mod interpreter;
mod invocation;
mod lifecycle;
mod native;
mod native_runtime;
mod resolution;
mod runtime;
mod string_builder_shims;
mod string_shims;
mod value;

use std::io::{self, Write};
use std::path::PathBuf;

use frame::Frame;
use interpreter::Interpreter;
use value::Value;

use crate::classfile::ClassFile;
use crate::classpath::ClassResolver;
use crate::{JayError, JayResult};

/// Public facade for loading and executing Java class files.
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

    /// Runs `main` in `main_class` with `program_args`, writing program output to stdout.
    pub fn run_main(&self, main_class: &str, program_args: &[String]) -> JayResult<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        self.run_main_to_writer(main_class, program_args, &mut handle)
    }

    /// Runs `main` in `main_class` with `program_args`, writing program output to `output`.
    ///
    /// `main(String[] args)` receives `program_args` as a `String[]`; `main()` ignores them.
    pub fn run_main_to_writer<W: Write>(
        &self,
        main_class: &str,
        program_args: &[String],
        output: &mut W,
    ) -> JayResult<()> {
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
        let mut frame = if main.descriptor == "([Ljava/lang/String;)V" {
            let args = interpreter.allocate_program_args(program_args)?;
            Frame::with_arguments(code.max_locals, vec![Value::Reference(args)])?
        } else {
            Frame::new(code.max_locals)
        };
        match interpreter.execute(&class_file, main, code, &mut frame)? {
            None => Ok(()),
            Some(_) => Err(JayError::new(format!(
                "main method in {main_class} returned a value"
            ))),
        }
    }
}
