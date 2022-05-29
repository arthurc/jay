use crate::{class_file, class_path::ClassPath, JayError};

pub struct Runtime<CP> {
    class_path: CP,
}

impl<CP: ClassPath> Runtime<CP> {
    pub fn new(class_path: CP) -> Self {
        Self { class_path }
    }

    pub fn run_with_main(&self, main_class_name: &str) -> Result<(), JayError> {
        let class_name = main_class_name;

        let resource_name = class_name.replace(".", "/") + ".class";
        let bytes = self
            .class_path
            .find_resource(&resource_name)
            .ok_or_else(|| JayError::NotFound(String::from(resource_name)))?;

        class_file::parse(bytes, |event| {
            dbg!(event);
        })?;

        Ok(())
    }
}
