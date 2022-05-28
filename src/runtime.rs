use crate::{class_path::ClassPath, JayError};

pub struct Runtime<CP: ClassPath> {
    class_path: CP,
}

impl<CP: ClassPath> Runtime<CP> {
    pub fn new(class_path: CP) -> Self {
        Self { class_path }
    }

    pub fn run_with_main(&self, main_class_name: &str) -> Result<(), JayError> {
        let main_resource = main_class_name.replace(".", "/") + ".class";
        let _bytes = self
            .class_path
            .find_resource(&main_resource)
            .ok_or_else(|| JayError::NotFound(String::from(main_resource)))?;
        Ok(())
    }
}
