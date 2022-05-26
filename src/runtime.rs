use crate::JayError;

pub struct Runtime {}

impl Runtime {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run_with_main(&self, _main_class_name: &str) -> Result<(), JayError> {
        Ok(())
    }
}
