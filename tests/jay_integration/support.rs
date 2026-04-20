//! Shared helpers for Jay integration tests.
//!
//! The tests compile tiny Java programs on the fly and then execute them
//! through the compiled `jay` binary. Keeping the helpers in one module makes
//! each behavior-focused test module independent without duplicating process
//! setup or class-file patching code.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "jay-integration-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

pub(crate) fn compile_java(root: &Path, relative_source_path: &str, source: &str) {
    let source_path = root.join(relative_source_path);
    std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    std::fs::write(&source_path, source).unwrap();

    let output = Command::new("javac")
        .arg("--release")
        .arg("21")
        .arg("-d")
        .arg(root)
        .arg(&source_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "javac failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn compile_java_sources(root: &Path, sources: &[(&str, &str)]) {
    let mut source_paths = Vec::new();
    for (relative_source_path, source) in sources {
        let source_path = root.join(relative_source_path);
        std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        std::fs::write(&source_path, source).unwrap();
        source_paths.push(source_path);
    }

    let output = Command::new("javac")
        .arg("--release")
        .arg("21")
        .arg("-d")
        .arg(root)
        .args(&source_paths)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "javac failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn jay(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jay"))
        .args(args)
        .output()
        .unwrap()
}

pub(crate) fn make_method_non_static(root: &Path, relative_class_path: &str, method_name: &str) {
    let class_path = root.join(relative_class_path);
    let mut bytes = std::fs::read(&class_path).unwrap();
    let mut cursor = ClassEditor::new(&mut bytes);
    cursor.clear_method_access_flag(method_name, 0x0008);
    std::fs::write(class_path, bytes).unwrap();
}

struct ClassEditor<'a> {
    bytes: &'a mut [u8],
    offset: usize,
    utf8_entries: Vec<Option<String>>,
}

impl<'a> ClassEditor<'a> {
    fn new(bytes: &'a mut [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            utf8_entries: Vec::new(),
        }
    }

    fn clear_method_access_flag(&mut self, method_name: &str, flag: u16) {
        assert_eq!(self.read_u4(), 0xCAFEBABE);
        self.skip(4);
        self.read_constant_pool();
        self.skip(6);
        self.skip_table(2);
        self.skip_members();

        let methods_count = self.read_u2();
        for _ in 0..methods_count {
            let access_flags_offset = self.offset;
            let access_flags = self.read_u2();
            let name_index = self.read_u2() as usize;
            self.skip(2);
            let attributes_count = self.read_u2();

            if self.utf8_entries[name_index].as_deref() == Some(method_name) {
                let new_access_flags = access_flags & !flag;
                self.bytes[access_flags_offset..access_flags_offset + 2]
                    .copy_from_slice(&new_access_flags.to_be_bytes());
                return;
            }

            for _ in 0..attributes_count {
                self.skip(2);
                let length = self.read_u4() as usize;
                self.skip(length);
            }
        }

        panic!("method {method_name} not found");
    }

    fn read_constant_pool(&mut self) {
        let count = self.read_u2() as usize;
        self.utf8_entries = vec![None; count];
        let mut index = 1;
        while index < count {
            let tag = self.read_u1();
            match tag {
                1 => {
                    let length = self.read_u2() as usize;
                    let value =
                        String::from_utf8(self.bytes[self.offset..self.offset + length].to_vec())
                            .unwrap();
                    self.utf8_entries[index] = Some(value);
                    self.skip(length);
                }
                3 | 4 => self.skip(4),
                5 | 6 => {
                    self.skip(8);
                    index += 1;
                }
                7 | 8 | 16 | 19 | 20 => self.skip(2),
                9 | 10 | 11 | 12 | 17 | 18 => self.skip(4),
                15 => self.skip(3),
                other => panic!("unsupported test constant pool tag {other}"),
            }
            index += 1;
        }
    }

    fn skip_members(&mut self) {
        let count = self.read_u2();
        for _ in 0..count {
            self.skip(6);
            let attributes_count = self.read_u2();
            for _ in 0..attributes_count {
                self.skip(2);
                let length = self.read_u4() as usize;
                self.skip(length);
            }
        }
    }

    fn skip_table(&mut self, entry_size: usize) {
        let count = self.read_u2() as usize;
        self.skip(count * entry_size);
    }

    fn read_u1(&mut self) -> u8 {
        let value = self.bytes[self.offset];
        self.offset += 1;
        value
    }

    fn read_u2(&mut self) -> u16 {
        let value = u16::from_be_bytes([self.bytes[self.offset], self.bytes[self.offset + 1]]);
        self.offset += 2;
        value
    }

    fn read_u4(&mut self) -> u32 {
        let value = u32::from_be_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
        ]);
        self.offset += 4;
        value
    }

    fn skip(&mut self, length: usize) {
        self.offset += length;
    }
}
