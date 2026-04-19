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

        Interpreter::new(&class_file, output).execute(code)
    }
}

struct Interpreter<'a, W: Write> {
    class_file: &'a ClassFile,
    output: &'a mut W,
    stack: Vec<Value>,
}

impl<'a, W: Write> Interpreter<'a, W> {
    fn new(class_file: &'a ClassFile, output: &'a mut W) -> Self {
        Self {
            class_file,
            output,
            stack: Vec::new(),
        }
    }

    fn execute(&mut self, code: &Code) -> JayResult<()> {
        let mut pc = 0usize;
        while pc < code.bytes.len() {
            let opcode_pc = pc;
            let opcode = read_u1(&code.bytes, &mut pc)?;
            match opcode {
                0x00 => {}
                0x02 => self.stack.push(Value::Int(-1)),
                0x03 => self.stack.push(Value::Int(0)),
                0x04 => self.stack.push(Value::Int(1)),
                0x05 => self.stack.push(Value::Int(2)),
                0x06 => self.stack.push(Value::Int(3)),
                0x07 => self.stack.push(Value::Int(4)),
                0x08 => self.stack.push(Value::Int(5)),
                0x10 => {
                    let value = read_u1(&code.bytes, &mut pc)? as i8 as i32;
                    self.stack.push(Value::Int(value));
                }
                0x11 => {
                    let value = read_u2(&code.bytes, &mut pc)? as i16 as i32;
                    self.stack.push(Value::Int(value));
                }
                0x12 => {
                    let index = read_u1(&code.bytes, &mut pc)? as u16;
                    self.load_constant(index)?;
                }
                0x13 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.load_constant(index)?;
                }
                0xb1 => return Ok(()),
                0xb2 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.get_static(index)?;
                }
                0xb6 => {
                    let index = read_u2(&code.bytes, &mut pc)?;
                    self.invoke_virtual(index)?;
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

    fn load_constant(&mut self, index: u16) -> JayResult<()> {
        let constant_pool = &self.class_file.constant_pool;
        if let Ok(value) = constant_pool.string(index) {
            self.stack.push(Value::String(value.to_string()));
            return Ok(());
        }

        if let Ok(value) = constant_pool.integer(index) {
            self.stack.push(Value::Int(value));
            return Ok(());
        }

        Err(JayError::new(format!(
            "unsupported ldc constant at pool index #{index}"
        )))
    }

    fn get_static(&mut self, index: u16) -> JayResult<()> {
        let field = self.class_file.constant_pool.field_ref(index)?;
        if field.class_name == "java/lang/System"
            && field.name == "out"
            && field.descriptor == "Ljava/io/PrintStream;"
        {
            self.stack.push(Value::PrintStream);
            Ok(())
        } else {
            Err(JayError::new(format!(
                "unsupported getstatic {}.{}:{}",
                field.class_name, field.name, field.descriptor
            )))
        }
    }

    fn invoke_virtual(&mut self, index: u16) -> JayResult<()> {
        let method = self.class_file.constant_pool.method_ref(index)?;
        if method.class_name == "java/io/PrintStream" && method.name == "println" {
            return match method.descriptor {
                "(Ljava/lang/String;)V" => {
                    let value = self.pop_string()?;
                    self.pop_print_stream()?;
                    writeln!(self.output, "{value}")?;
                    Ok(())
                }
                "(I)V" => {
                    let value = self.pop_int()?;
                    self.pop_print_stream()?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
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
