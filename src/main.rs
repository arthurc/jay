use std::{env, fs::File, path::PathBuf};

use jay::{
    class_path::{ClassPath, DirClassPath},
    jimage, Result, Runtime,
};
use memmap::Mmap;

fn main() -> Result<()> {
    pretty_env_logger::init();

    let path = env::var("JAVA_HOME")
        .map(|s| PathBuf::from(s).join("lib/modules"))
        .unwrap();
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    let jimage = jimage::Archive::parse(&mmap)?;

    let mut class_paths: Vec<Box<dyn ClassPath>> = Vec::new();
    class_paths.push(Box::new(jimage));
    if let Some(classes) = DirClassPath::new("classes".into()).take() {
        class_paths.push(Box::new(classes));
    }

    let runtime = Runtime::new(Box::new(class_paths));
    runtime.run_with_main("com.example.Main")?;
    Ok(())
}
