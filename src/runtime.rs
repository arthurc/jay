use std::cell::RefCell;

use log::trace;

use crate::{
    class_file::{constant_pool, ClassFile},
    class_path::ClassPath,
    JayError,
};

pub struct Runtime<CP> {
    class_path: CP,
    classes: RefCell<Vec<Class>>,
}

impl<CP: ClassPath> Runtime<CP> {
    pub fn new(class_path: CP) -> Self {
        Self {
            class_path,
            classes: RefCell::new(Vec::new()),
        }
    }

    pub fn run_with_main(&self, main_class_name: &str) -> Result<(), JayError> {
        let class = self.load_class(main_class_name)?;

        let x = &self.classes.borrow()[class];

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

        self.add_class(Class { name, super_class })
    }

    fn add_class(&self, class: Class) -> Result<ClassId, JayError> {
        self.classes.borrow_mut().push(dbg!(class));

        Ok(self.classes.borrow().len() - 1)
    }
}

type ClassId = usize;

#[derive(Debug)]
struct Class {
    name: String,
    super_class: Option<ClassId>,
}
