use std::{env, fs::File, path::PathBuf};

use jay::{
    jimage,
    runtime::{ClassPath, Runtime},
};
use memmap::Mmap;

fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let path = env::var("JAVA_HOME")
        .map(|s| PathBuf::from(s).join("lib/modules"))
        .unwrap();
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    let jimage = jimage::Archive::parse(&mmap)?;

    let runtime = Runtime::new(ClassPath::Composite(vec![
        ClassPath::JImage(jimage),
        ClassPath::Directory("classes".into()),
    ]));
    runtime.run_with_main("com.example.Main")?;

    Ok(())
}
