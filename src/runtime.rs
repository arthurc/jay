use std::{cell::RefCell, io};

use jay_bytecode::BytecodeStream;
use log::trace;

use crate::{
    class_path::ClassPath,
    classfile::{AccessFlags, ClassFile, CodeAttribute, ConstantPool},
    Error, Result,
};

pub struct Runtime<'a> {
    class_path: Box<dyn ClassPath + 'a>,
    classes: RefCell<Vec<Class>>,
}

impl<'a> Runtime<'a> {
    pub fn new(class_path: Box<dyn ClassPath + 'a>) -> Self {
        Self {
            class_path,
            classes: RefCell::new(Vec::new()),
        }
    }

    pub fn run_with_main(&self, main_class_name: &str) -> Result<()> {
        let class_id = self.load_class(main_class_name)?;

        let classes = self.classes.borrow();
        let main = classes[class_id]
            .find_method("main", &[])
            .ok_or_else(|| Error::NoSuchMethod("main".to_owned()))?;

        main.invoke(self)?;

        Ok(())
    }

    fn load_class(&self, class_name: &str) -> Result<ClassId> {
        trace!("Loading class. name={}", class_name);

        let resource_name = class_name.replace(".", "/") + ".class";
        let bytes = self
            .class_path
            .find_resource(&resource_name)
            .ok_or_else(|| Error::NotFound(resource_name.into()))?;
        let class_file = ClassFile::parse(bytes)?;

        let name = class_file.class_name()?;
        let _super_class = class_file.super_class_name()?;

        let methods = class_file
            .methods()
            .map(|m| {
                let name = m.name()?;
                let body = if m.access_flags().contains(AccessFlags::NATIVE) {
                    MethodBody::Native
                } else {
                    MethodBody::Code(m.attributes().code()?.expect("Expecting code method body"))
                };

                Ok(Method {
                    name: name.to_owned(),
                    body,
                })
            })
            .collect::<Result<_, Error>>()?;

        let class_id = self.add_class(Class {
            name: name.to_string(),
            super_class: None, // TODO
            constant_pool: class_file.constant_pool,
            methods,
        })?;

        let classes = self.classes.borrow();
        if let Some(clinit) = classes[class_id].find_method("<clinit>", &[]) {
            clinit.invoke(self)?;
        }

        Ok(class_id)
    }

    fn add_class(&self, class: Class) -> Result<ClassId> {
        self.classes.borrow_mut().push(class);

        Ok(self.classes.borrow().len() - 1)
    }
}

type ClassId = usize;

struct Class {
    name: String,
    super_class: Option<ClassId>,
    constant_pool: ConstantPool,
    methods: Vec<Method>,
}
impl Class {
    fn find_method(&self, name: &str, parameter_types: &[Type]) -> Option<MethodHandle> {
        self.methods
            .iter()
            .find(|m| m.name == name)
            .map(|method| MethodHandle {
                method,
                class: self,
            })
    }
}

struct MethodHandle<'a> {
    method: &'a Method,
    class: &'a Class,
}
impl MethodHandle<'_> {
    fn invoke(&self, runtime: &Runtime) -> Result<()> {
        trace!("Invoking method {}", self.method.name);
        trace!("Constant pool: {:?}", self.class.constant_pool);

        self.method.body.invoke(self, runtime)
    }
}

struct Method {
    name: String,
    body: MethodBody,
}

struct Frame<'a> {
    pc: usize,
    class: &'a Class,
    code_attribute: &'a CodeAttribute,
}
impl BytecodeStream for Frame<'_> {
    fn read_u8(&mut self) -> io::Result<u8> {
        let b = self
            .code_attribute
            .code
            .get(self.pc)
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        self.pc += 1;
        Ok(*b)
    }
}
enum MethodBody {
    Native,
    Code(CodeAttribute),
}
impl MethodBody {
    pub fn invoke(&self, method: &MethodHandle, _runtime: &Runtime) -> Result<()> {
        match self {
            Self::Native => todo!(),
            Self::Code(code_attribute) => {
                let mut frame = Frame {
                    pc: 0,
                    class: &method.class,
                    code_attribute,
                };

                loop {
                    let pc = frame.pc;

                    match frame.next() {
                        Some(bytecode) => trace!("{pc:>4}: {bytecode}"),
                        None => break,
                    }
                }

                Ok(())
            }
        }
    }
}

#[derive(PartialEq)]
enum Type {
    Primitive(PrimitiveType),
}

#[derive(PartialEq)]
enum PrimitiveType {
    Void,
}
