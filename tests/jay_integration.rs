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

fn jay(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jay"))
        .args(args)
        .output()
        .unwrap()
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
fn reports_unsupported_bytecode() {
    let root = temp_dir("unsupported-bytecode");
    compile_java(
        &root,
        "UnsupportedMain.java",
        r#"
public class UnsupportedMain {
    public static void main(String[] args) {
        int x = 1;
        x++;
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "UnsupportedMain"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported bytecode"));
}

#[test]
fn reports_malformed_class_file() {
    let root = temp_dir("malformed");
    std::fs::write(root.join("Broken.class"), b"broken").unwrap();

    let output = jay(&["-cp", root.to_str().unwrap(), "Broken"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid class file magic"));
}
