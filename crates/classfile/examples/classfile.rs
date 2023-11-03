use std::io::Cursor;

use jay_classfile as classfile;

const CLASS_BYTES: &'static [u8] = include_bytes!("../../../classes/com/example/Main.class");

fn main() {
    let f = Cursor::new(CLASS_BYTES);
    let classfile = classfile::ClassFile::parse(f).unwrap();

    println!("Class");
    println!("  Name: {}", classfile.class_name().unwrap());
}
