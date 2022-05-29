use std::{env, fs::File, path::PathBuf};

use jay::{
    class_path::{ClassPath, DirClassPath},
    jimage, JayError, Runtime,
};
use memmap::Mmap;

fn main() -> Result<(), JayError> {
    pretty_env_logger::init();

    let path = env::var("JAVA_HOME")
        .map(|s| PathBuf::from(s).join("lib/modules"))
        .unwrap();
    let file = File::open(path).unwrap();
    let mmap = unsafe { Mmap::map(&file).unwrap() };

    let jimage = jimage::Archive::parse(&mmap)?;

    let classes = DirClassPath::new("classes".into());

    let mut class_paths: Vec<&dyn ClassPath> = Vec::new();
    class_paths.push(&jimage);
    if let Some(classes) = classes.as_ref() {
        class_paths.push(classes);
    }

    let runtime = Runtime::new(class_paths.into_boxed_slice());
    runtime.run_with_main("com.example.Main")?;
    Ok(())
}
