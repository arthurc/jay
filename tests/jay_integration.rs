use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(name: &str) -> PathBuf {
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

fn compile_java(root: &Path, relative_source_path: &str, source: &str) {
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

fn compile_java_sources(root: &Path, sources: &[(&str, &str)]) {
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

fn jay(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jay"))
        .args(args)
        .output()
        .unwrap()
}

fn make_method_non_static(root: &Path, relative_class_path: &str, method_name: &str) {
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

#[test]
fn runs_hello_world_from_directory_classpath() {
    let root = temp_dir("hello");
    compile_java(
        &root,
        "HelloWorld.java",
        r#"
public class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello from jay");
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "HelloWorld"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello from jay\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn resolves_fully_qualified_main_class() {
    let root = temp_dir("qualified");
    compile_java(
        &root,
        "com/example/Main.java",
        r#"
package com.example;

public class Main {
    public static void main(String[] args) {
        System.out.println(7);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "com.example.Main"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_no_argument_main_method() {
    let root = temp_dir("no-argument-main");
    compile_java(
        &root,
        "com/example/Main.java",
        r#"
package com.example;

public class Main {
    public static void main() {
        System.out.println("Hello world!");
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "com.example.Main"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello world!\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn string_array_main_takes_precedence_over_no_argument_main() {
    let root = temp_dir("main-overload-precedence");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        System.out.println("args");
    }

    public static void main() {
        System.out.println("no args");
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "args\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn rejects_missing_cp() {
    let output = jay(&["HelloWorld"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: jay -cp"));
}

#[test]
fn rejects_invalid_classpath() {
    let output = jay(&["-cp", "/definitely/not/a/jay/classpath", "HelloWorld"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("classpath is not a directory"));
}

#[test]
fn reports_missing_class() {
    let root = temp_dir("missing-class");

    let output = jay(&["-cp", root.to_str().unwrap(), "Missing"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not read class Missing"));
}

#[test]
fn reports_missing_main_method() {
    let root = temp_dir("missing-main");
    compile_java(
        &root,
        "NoMain.java",
        r#"
public class NoMain {
    public static void notMain(String[] args) {
        System.out.println("nope");
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "NoMain"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("main method not found"));
}

#[test]
fn falls_back_to_default_jimage_for_jdk_classes() {
    let root = temp_dir("jimage-fallback");

    let output = jay(&["-cp", root.to_str().unwrap(), "java.lang.Object"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("main method not found"), "{stderr}");
    assert!(
        !stderr.contains("could not read class java.lang.Object"),
        "{stderr}"
    );
}

#[test]
fn runs_integer_locals_and_addition() {
    let root = temp_dir("integer-locals-addition");
    compile_java(
        &root,
        "ArithmeticMain.java",
        r#"
public class ArithmeticMain {
    public static void main(String[] args) {
        int x = 1;
        x++;
        System.out.println(x + 4);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "ArithmeticMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "6\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_integer_locals_beyond_compact_slots() {
    let root = temp_dir("integer-locals-beyond-compact-slots");
    compile_java(
        &root,
        "ManyLocalsMain.java",
        r#"
public class ManyLocalsMain {
    public static void main(String[] args) {
        int a = 1;
        int b = 2;
        int c = 3;
        int d = 4;
        int e = 5;
        System.out.println(a + b + c + d + e);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "ManyLocalsMain"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "15\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_integer_multiplication() {
    let root = temp_dir("integer-multiplication");
    compile_java(
        &root,
        "MultiplicationMain.java",
        r#"
public class MultiplicationMain {
    public static void main(String[] args) {
        int x = 2;
        int y = 3;
        System.out.println(x * y);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "MultiplicationMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "6\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_integer_subtraction() {
    let root = temp_dir("integer-subtraction");
    compile_java(
        &root,
        "SubtractionMain.java",
        r#"
public class SubtractionMain {
    public static void main(String[] args) {
        int x = 9;
        int y = 4;
        System.out.println(x - y);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "SubtractionMain"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "5\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_integer_division() {
    let root = temp_dir("integer-division");
    compile_java(
        &root,
        "DivisionMain.java",
        r#"
public class DivisionMain {
    public static void main(String[] args) {
        int x = 6;
        int y = 3;
        System.out.println(x / y);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "DivisionMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_integer_if_else_branches() {
    let root = temp_dir("integer-if-else");
    compile_java(
        &root,
        "BranchMain.java",
        r#"
public class BranchMain {
    public static void main(String[] args) {
        int x = 7;
        if (x > 3) {
            System.out.println("large");
        } else {
            System.out.println("small");
        }

        int y = 2;
        if (y > 3) {
            System.out.println("large");
        } else {
            System.out.println("small");
        }
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "BranchMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "large\nsmall\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_simple_integer_loop() {
    let root = temp_dir("integer-loop");
    compile_java(
        &root,
        "LoopMain.java",
        r#"
public class LoopMain {
    public static void main(String[] args) {
        int sum = 0;
        for (int i = 0; i < 3; i++) {
            sum += i;
        }
        System.out.println(sum);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "LoopMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_integer_zero_comparison_branches() {
    let root = temp_dir("integer-zero-comparisons");
    compile_java(
        &root,
        "ZeroBranchMain.java",
        r#"
public class ZeroBranchMain {
    public static void main(String[] args) {
        int value = 0;
        if (value == 0) {
            System.out.println("zero");
        }

        value = 1;
        if (value != 0) {
            System.out.println("nonzero");
        }

        value = -1;
        if (value < 0) {
            System.out.println("negative");
        }

        value = 1;
        if (value > 0) {
            System.out.println("positive");
        }
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "ZeroBranchMain"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "zero\nnonzero\nnegative\npositive\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_same_class_static_int_method() {
    let root = temp_dir("static-int-method");
    compile_java(
        &root,
        "StaticIntMain.java",
        r#"
public class StaticIntMain {
    static int add(int left, int right) {
        return left + right;
    }

    public static void main(String[] args) {
        System.out.println(add(2, 3));
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "StaticIntMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "5\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_same_class_static_void_method() {
    let root = temp_dir("static-void-method");
    compile_java(
        &root,
        "StaticVoidMain.java",
        r#"
public class StaticVoidMain {
    static void printTwice(int value) {
        System.out.println(value);
        System.out.println(value);
    }

    public static void main(String[] args) {
        printTwice(4);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "StaticVoidMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "4\n4\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_static_int_method_with_more_than_four_arguments() {
    let root = temp_dir("static-int-method-more-than-four-arguments");
    compile_java(
        &root,
        "ManyArgsMain.java",
        r#"
public class ManyArgsMain {
    static int sum(int a, int b, int c, int d, int e) {
        return a + b + c + d + e;
    }

    public static void main(String[] args) {
        System.out.println(sum(1, 2, 3, 4, 5));
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "ManyArgsMain"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "15\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_static_string_method_return_and_parameter() {
    let root = temp_dir("static-string-method-return-parameter");
    compile_java(
        &root,
        "StaticStringMain.java",
        r#"
public class StaticStringMain {
    static String message() {
        return "hello";
    }

    static void print(String value) {
        System.out.println(value);
    }

    public static void main(String[] args) {
        print(message());
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "StaticStringMain"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_static_string_return_stored_in_local() {
    let root = temp_dir("static-string-return-local");
    compile_java(
        &root,
        "StaticStringLocalMain.java",
        r#"
public class StaticStringLocalMain {
    static String message() {
        return "local";
    }

    static void print(String value) {
        System.out.println(value);
    }

    public static void main(String[] args) {
        String value = message();
        print(value);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "StaticStringLocalMain"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "local\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn keeps_caller_string_local_alive_when_nested_call_triggers_gc() {
    let root = temp_dir("gc-keeps-caller-string-local");
    compile_java(
        &root,
        "GcRootMain.java",
        r#"
public class GcRootMain {
    static void sink(String value) {
    }

    static void churn() {
        sink("a");
        sink("b");
        sink("c");
        sink("d");
        sink("e");
        sink("f");
        sink("g");
        sink("h");
        sink("i");
    }

    public static void main(String[] args) {
        String saved = "survivor";
        churn();
        System.out.println(saved);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "GcRootMain"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "survivor\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_simple_object_construction() {
    let root = temp_dir("simple-object-construction");
    compile_java(
        &root,
        "Main.java",
        r#"
class Empty {
}

public class Main {
    public static void main(String[] args) {
        Empty value = new Empty();
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_instance_field_assignments() {
    let root = temp_dir("instance-field-assignments");
    compile_java(
        &root,
        "Main.java",
        r#"
class Car {
    String make;
    int year;

    Car(String make, int year) {
        this.make = make;
        this.year = year;
    }
}

class Garage {
    Car car;
}

public class Main {
    public static void main(String[] args) {
        Car car = new Car("Toyota", 2020);
        Garage garage = new Garage();
        garage.car = car;
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_constructor_expression_statement() {
    let root = temp_dir("constructor-expression-statement");
    compile_java(
        &root,
        "Main.java",
        r#"
class Empty {
}

public class Main {
    public static void main(String[] args) {
        new Empty();
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn reports_non_static_same_class_method_called_with_invokestatic() {
    let root = temp_dir("non-static-invokestatic");
    compile_java(
        &root,
        "NonStaticMain.java",
        r#"
public class NonStaticMain {
    static int helper(int value) {
        return value;
    }

    public static void main(String[] args) {
        System.out.println(helper(9));
    }
}
"#,
    );
    make_method_non_static(&root, "NonStaticMain.class", "helper");

    let output = jay(&["-cp", root.to_str().unwrap(), "NonStaticMain"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("invokestatic target NonStaticMain.helper(I)I must be static")
    );
}

#[test]
fn runs_cross_class_static_int_method() {
    let root = temp_dir("cross-class-invokestatic");
    compile_java_sources(
        &root,
        &[
            (
                "Other.java",
                r#"
public class Other {
    static int value() {
        return 11;
    }
}
"#,
            ),
            (
                "CrossClassMain.java",
                r#"
public class CrossClassMain {
    public static void main(String[] args) {
        System.out.println(Other.value());
    }
}
"#,
            ),
        ],
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "CrossClassMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "11\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_packaged_cross_class_static_int_method() {
    let root = temp_dir("packaged-cross-class-invokestatic");
    compile_java_sources(
        &root,
        &[
            (
                "com/example/Other.java",
                r#"
package com.example;

public class Other {
    static int value() {
        return 13;
    }
}
"#,
            ),
            (
                "com/example/Main.java",
                r#"
package com.example;

public class Main {
    public static void main(String[] args) {
        System.out.println(Other.value());
    }
}
"#,
            ),
        ],
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "com.example.Main"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "13\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn reports_non_static_cross_class_method_called_with_invokestatic() {
    let root = temp_dir("non-static-cross-class-invokestatic");
    compile_java_sources(
        &root,
        &[
            (
                "Other.java",
                r#"
public class Other {
    static int value() {
        return 17;
    }
}
"#,
            ),
            (
                "Main.java",
                r#"
public class Main {
    public static void main(String[] args) {
        System.out.println(Other.value());
    }
}
"#,
            ),
        ],
    );
    make_method_non_static(&root, "Other.class", "value");

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("invokestatic target Other.value()I must be static")
    );
}

#[test]
fn reports_malformed_class_file() {
    let root = temp_dir("malformed");
    std::fs::write(root.join("Broken.class"), b"broken").unwrap();

    let output = jay(&["-cp", root.to_str().unwrap(), "Broken"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid class file magic"));
}
