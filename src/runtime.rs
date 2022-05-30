use std::cell::RefCell;

use log::trace;

use crate::{
    class_file::{constant_pool, ClassFile, CodeAttribute, MethodInfo},
    class_path::ClassPath,
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
        let main = classes[class_id].find_method("main", &[])?;

        main.invoke(self);

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

        let class_id = self.add_class(Class {
            name,
            super_class,
            methods: class_file
                .methods
                .iter()
                .map(
                    |MethodInfo {
                         name_index,
                         attributes,
                         ..
                     }| {
                        class_file.constant_pool[*name_index].to_utf8().map(|name| {
                            let code_attribute =
                                attributes.code_attribute(&class_file.constant_pool);

                            Method {
                                name: name.to_owned(),
                                body: code_attribute
                                    .map(|b| -> Box<dyn MethodBody> { Box::new(b) }),
                            }
                        })
                    },
                )
                .collect::<Result<_, _>>()?,
        })?;

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
    methods: Vec<Method>,
}
impl Class {
    fn find_method(&self, name: &str, parameter_types: &[Type]) -> Result<&Method, JayError> {
        self.methods
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| JayError::NoSuchMethod(name.to_owned()))
    }
}

struct Method {
    name: String,
    body: Option<Box<dyn MethodBody>>,
}
impl Method {
    fn invoke(&self, runtime: &Runtime) {
        if let Some(body) = &self.body {
            body.invoke(runtime)
        }
    }
}

trait MethodBody {
    fn invoke(&self, runtime: &Runtime);
}
impl MethodBody for CodeAttribute {
    fn invoke(&self, runtime: &Runtime) {
        todo!()
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
