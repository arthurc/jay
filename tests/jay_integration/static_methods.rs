use crate::support::{compile_java, compile_java_sources, jay, make_method_non_static, temp_dir};

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
fn keeps_static_string_field_alive_when_gc_runs() {
    let root = temp_dir("gc-keeps-static-string-field");
    compile_java(
        &root,
        "StaticFieldGcMain.java",
        r#"
class StaticHolder {
    static String saved;
}

public class StaticFieldGcMain {
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
        StaticHolder.saved = "static survivor";
        churn();
        System.out.println(StaticHolder.saved);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "StaticFieldGcMain"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "static survivor\n");
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
