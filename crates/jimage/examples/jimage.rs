use std::{env, fs::File, path::PathBuf};

use jay_jimage::Archive;
use memmap::Mmap;

fn main() {
    pretty_env_logger::init();

    let path = env::var("JAVA_HOME")
        .map(|s| PathBuf::from(s).join("lib/modules"))
        .unwrap();
    let file = File::open(path).unwrap();
    let mmap = unsafe { Mmap::map(&file).unwrap() };

    let archive = Archive::parse(&mmap).unwrap();

    println!("Header:");
    print!("{}", archive.header());
    println!();
    println!("Resources:");
    for resource in archive.resources() {
        println!(
            "{}/{}/{}.{} (offset: {})",
            resource.module(),
            resource.package(),
            resource.base(),
            resource.extension(),
            resource.offset()
        );
    }
    println!();

    let resource = archive
        .by_name("/java.base/java/lang/Object.class")
        .unwrap();
    println!(
        "/java.base/java/lang/Object.class = /{}/{}/{}.{}",
        resource.module(),
        resource.package(),
        resource.base(),
        resource.extension()
    );

    let module_resource = archive.by_name("/modules/java.base").unwrap();
    println!("/modules/java.base = {}", module_resource.base());

    let package_resource = archive.by_name("/packages/java.lang").unwrap();
    println!("/packages/java.lang = {}", package_resource.base());
}
