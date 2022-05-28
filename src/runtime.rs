use crate::{jimage, JayError};

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

pub trait ClassPath {
    fn find_resource(&self, name: &str) -> Option<Box<[u8]>>;
}

impl ClassPath for jimage::Archive<'_> {
    fn find_resource(&self, name: &str) -> Option<Box<[u8]>> {
        self.by_name(name).map(|r| r.bytes().into())
    }
}
