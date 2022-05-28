use std::{env, fs::File, path::PathBuf};

use jay::{jimage, JayError, Runtime};
use memmap::Mmap;

fn main() -> Result<(), JayError> {
    let path = env::var("JAVA_HOME")
        .map(|s| PathBuf::from(s).join("lib/modules"))
        .unwrap();
    let file = File::open(path).unwrap();
    let mmap = unsafe { Mmap::map(&file).unwrap() };

    let runtime = Runtime::new(jimage::Archive::parse(&mmap)?);
    runtime.run_with_main("com.example.Test")?;
    Ok(())
}
