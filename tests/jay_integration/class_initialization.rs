use crate::support::{compile_java, compile_java_sources, jay, temp_dir};

#[test]
fn putstatic_initializes_declaring_class_before_write() {
    let root = temp_dir("putstatic-initializes-class");
    compile_java(
        &root,
        "PutStaticMain.java",
        r#"
class A {
    static int x;
    static int y;

    static {
        y = x;
    }
}

public class PutStaticMain {
    public static void main(String[] args) {
        A.x = 42;
        System.out.println(A.y);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "PutStaticMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn putstatic_keeps_reference_value_alive_across_class_initialization() {
    let root = temp_dir("putstatic-keeps-reference-across-init");
    compile_java_sources(
        &root,
        &[
            (
                "Box.java",
                r#"
class Box {
}
"#,
            ),
            (
                "Holder.java",
                r#"
class Holder {
    static String saved;

    static {
        new Box();
        new Box();
        new Box();
        new Box();
        new Box();
        new Box();
        new Box();
        new Box();
        new Box();
    }
}
"#,
            ),
            (
                "PutStaticReferenceMain.java",
                r#"
public class PutStaticReferenceMain {
    public static void main(String[] args) {
        Holder.saved = "survivor";
        System.out.println(Holder.saved);
    }
}
"#,
            ),
        ],
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "PutStaticReferenceMain"]);

    assert!(
        output.status.success(),
        "jay failed
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "survivor\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn subclass_initialization_is_marked_in_progress_before_superclass_init() {
    let root = temp_dir("subclass-initialization-in-progress-before-super");
    compile_java_sources(
        &root,
        &[
            (
                "Super.java",
                r#"
class Super {
    static {
        System.out.println(Sub.x);
    }
}
"#,
            ),
            (
                "Sub.java",
                r#"
class Sub extends Super {
    static int x = 1;
}
"#,
            ),
            (
                "ReentrantInitMain.java",
                r#"
public class ReentrantInitMain {
    public static void main(String[] args) {
        System.out.println(Sub.x);
    }
}
"#,
            ),
        ],
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "ReentrantInitMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0\n1\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn class_literal_loads_class_mirror_without_initializing_represented_class() {
    let root = temp_dir("class-literal-desired-assertion-status");
    compile_java(
        &root,
        "ClassLiteralMain.java",
        r#"
class SomeClass {
    static {
        System.out.println("initialized");
    }
}

public class ClassLiteralMain {
    public static void main(String[] args) {
        System.out.println(SomeClass.class == SomeClass.class);
        System.out.println(SomeClass.class.desiredAssertionStatus());
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "ClassLiteralMain"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "true\nfalse\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}
