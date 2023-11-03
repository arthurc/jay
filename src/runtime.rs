use std::cell::RefCell;

use constant_pool::ConstantPool;
use log::trace;

use crate::{
    bytecode::{Bytecode, BytecodeStream},
    class_path::ClassPath,
    classfile::{constant_pool, AccessFlags, ClassFile, CodeAttribute, MethodInfo},
    JayError,
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

    pub fn run_with_main(&self, main_class_name: &str) -> Result<(), JayError> {
        let class_id = self.load_class(main_class_name)?;

        let classes = self.classes.borrow();
        let main = classes[class_id]
            .find_method("main", &[])
            .ok_or_else(|| JayError::NoSuchMethod("main".to_owned()))?;

        main.invoke(self)?;

        Ok(())
    }

    fn load_class(&self, class_name: &str) -> Result<ClassId, JayError> {
        trace!("Loading class. name={}", class_name);

        let resource_name = class_name.replace(".", "/") + ".class";
        let bytes = self
            .class_path
            .find_resource(&resource_name)
            .ok_or_else(|| JayError::NotFound(String::from(resource_name)))?;
        let class_file = ClassFile::parse(bytes)?;

        let constant_pool::ClassInfo { name_index } =
            class_file.constant_pool[class_file.this_class].to_class_info()?;

        let name = class_file.constant_pool[*name_index].to_utf8()?.to_string();

        let super_class = if class_file.super_class != 0 {
            let super_class = class_file.constant_pool[class_file.super_class].to_class_info()?;
            let super_class = class_file.constant_pool[super_class.name_index].to_utf8()?;
            Some(self.load_class(super_class)?)
        } else {
            None
        };

        let methods = class_file
            .methods
            .iter()
            .map(
                |MethodInfo {
                     name_index,
                     attributes,
                     access_flags,
                     ..
                 }| {
                    let name = class_file.constant_pool[*name_index].to_utf8()?;
                    let code_attribute = attributes.code_attribute(&class_file.constant_pool);
                    let body = code_attribute
                        .map(|code_attribute| -> Box<dyn MethodBody> { Box::new(code_attribute) })
                        .or_else(|| {
                            if access_flags.contains(AccessFlags::NATIVE) {
                                Some(Box::new(NativeMethodBody))
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| {
                            JayError::ClassLoadError(format!("No method body found for {}", name))
                        })?;

                    Ok(Method {
                        name: name.to_owned(),
                        body,
                    })
                },
            )
            .collect::<Result<_, JayError>>()?;

        let class_id = self.add_class(Class {
            name,
            super_class,
            constant_pool: class_file.constant_pool,
            methods,
        })?;

        let classes = self.classes.borrow();
        if let Some(clinit) = classes[class_id].find_method("<clinit>", &[]) {
            clinit.invoke(self)?;
        }

        Ok(class_id)
    }

    fn add_class(&self, class: Class) -> Result<ClassId, JayError> {
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
    fn invoke(&self, runtime: &Runtime) -> Result<(), JayError> {
        trace!("Invoking method {}", self.method.name);
        trace!("Constant pool: {:?}", self.class.constant_pool);

        self.method.body.invoke(self, runtime)
    }
}

struct Method {
    name: String,
    body: Box<dyn MethodBody>,
}

struct Frame<'a> {
    pc: usize,
    class: &'a Class,
    code_attribute: &'a CodeAttribute,
}
impl BytecodeStream for Frame<'_> {
    fn readb(&mut self) -> u8 {
        let b = self.code_attribute.code[self.pc];
        self.pc += 1;
        b
    }
}

trait MethodBody {
    fn invoke(&self, method: &MethodHandle, runtime: &Runtime) -> Result<(), JayError>;
}

struct NativeMethodBody;
impl MethodBody for NativeMethodBody {
    fn invoke(&self, _method: &MethodHandle, _runtime: &Runtime) -> Result<(), JayError> {
        todo!()
    }
}

impl MethodBody for CodeAttribute {
    fn invoke(&self, method: &MethodHandle, _runtime: &Runtime) -> Result<(), JayError> {
        let mut frame = Frame {
            pc: 0,
            class: &method.class,
            code_attribute: self,
        };

        loop {
            let pc = frame.pc;
            let bytecode = Bytecode::from_stream(&mut frame)?;
            trace!("{:>4}: {}", pc, bytecode);

            bytecode.handle();
        }

        Ok(())
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
