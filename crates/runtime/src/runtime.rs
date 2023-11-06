use std::{cell::RefCell, io, rc::Rc};

use log::trace;

use crate::{
    bytecode::BytecodeStream,
    class_path::{ClassPath, SeekRead},
    classfile::{ClassFile, CodeAttribute, ConstantPool},
    Error, Result,
};

pub struct Runtime<'a> {
    class_path: ClassPath<'a>,
    classes: RefCell<Vec<Class>>,
}

impl Runtime<'_> {
    pub fn new(class_path: ClassPath) -> Runtime {
        Runtime {
            class_path,
            classes: RefCell::new(Vec::new()),
        }
    }

    pub fn load_class(&self, class_name: &str) -> Result<Rc<Class>> {
        trace!("Loading class {}", class_name);

        let bytes = self.resolve_class(class_name)?;
        let mut class = self.define_class(class_name, bytes)?;
        self.initialize_class(&mut class)?;

        Ok(class)
    }

    fn resolve_class(&self, class_name: &str) -> Result<Box<dyn SeekRead + '_>> {
        trace!("Resolving class {}", class_name);

        let resource_name = class_name.replace(".", "/") + ".class";

        self.class_path
            .find_resource(&resource_name)
            .ok_or_else(|| Error::NotFound(resource_name.into()))
    }

    fn define_class(&self, class_name: &str, bytes: impl SeekRead) -> Result<Rc<Class>> {
        trace!("Defining class {}", class_name);
        let class_file = ClassFile::parse(bytes)?;

        let name = class_file.class_name()?;
        let super_class = if let Some(super_class_name) = class_file.super_class_name()? {
            let bytes = self.resolve_class(&super_class_name)?;
            Some(self.define_class(super_class_name, bytes)?)
        } else {
            None
        };

        let methods = class_file
            .methods()
            .map(|m| {
                let name = m.name()?;
                let code = m.attributes().code()?;

                Ok(Method {
                    name: name.to_owned(),
                    code,
                })
            })
            .collect::<Result<_, Error>>()?;

        let class = Rc::new(Class {
            name: name.to_string(),
            super_class,
            constant_pool: class_file.constant_pool,
            methods,
            initialized: RefCell::new(false),
        });

        Ok(class)
    }

    fn initialize_class(&self, class: &Rc<Class>) -> Result<()> {
        if let Some(super_class) = &class.super_class {
            self.initialize_class(super_class)?;
        }

        if *class.initialized.borrow() {
            return Ok(());
        }

        trace!("Initializing class {}", class.name);

        if let Some(clinit) = class.find_method("<clinit>") {
            self.invoke(&clinit)?;
        }

        *class.initialized.borrow_mut() = true;

        Ok(())
    }

    pub fn invoke(&self, method: &ClassMethod) -> Result<()> {
        trace!("Invoking method {}", method.method.name);

        if let Some(code) = &method.method.code {
            let mut frame = Frame {
                pc: 0,
                class: &method.class,
                code,
            };

            loop {
                let pc = frame.pc;

                match frame.next() {
                    Some(bytecode) => trace!("{pc:>4}: {bytecode}"),
                    None => break,
                }
            }
        }

        Ok(())
    }
}

pub struct Class {
    name: String,
    super_class: Option<Rc<Class>>,
    constant_pool: ConstantPool,
    methods: Vec<Method>,
    initialized: RefCell<bool>,
}
impl Class {
    pub fn find_method(&self, name: &str) -> Option<ClassMethod> {
        self.methods
            .iter()
            .find(|m| m.name == name)
            .map(|method| ClassMethod {
                method,
                class: self,
            })
    }
}

pub struct ClassMethod<'a> {
    class: &'a Class,
    method: &'a Method,
}

struct Method {
    name: String,
    code: Option<CodeAttribute>,
}

struct Frame<'a, 'b> {
    pc: usize,
    class: &'a Class,
    code: &'b CodeAttribute,
}
impl BytecodeStream for Frame<'_, '_> {
    fn read_u8(&mut self) -> io::Result<u8> {
        let b = self
            .code
            .code
            .get(self.pc)
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        self.pc += 1;
        Ok(*b)
    }
}
