use std::{env, fs::File, path::PathBuf, process::Command};

use jay::jimage::Archive;
use memmap::Mmap;

#[test]
fn archive_parse() {
    let path = env::var("JAVA_HOME")
        .map(|s| PathBuf::from(s).join("lib/modules"))
        .unwrap();
    let file = File::open(path.clone()).unwrap();
    let mmap = unsafe { Mmap::map(&file).unwrap() };

    let archive = Archive::parse(&mmap).unwrap();

    let jimage_command = Command::new("jimage")
        .arg("info")
        .arg(path)
        .output()
        .unwrap();

    assert_eq!(
        std::str::from_utf8(&jimage_command.stdout).unwrap(),
        format!("{}", archive.header())
    );

    // FIXME: Test resource parsing aswell
}
