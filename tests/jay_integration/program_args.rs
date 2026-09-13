use crate::support::{compile_java, jay, temp_dir};

#[test]
fn main_receives_empty_args_array() {
    let root = temp_dir("empty-args");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        System.out.println(args.length);
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn main_receives_program_arguments() {
    let root = temp_dir("program-args");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        System.out.println(args.length);
        System.out.println(args[1]);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main", "alpha", "beta"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "2\nbeta\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn main_without_parameters_ignores_program_arguments() {
    let root = temp_dir("no-param-main-args");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main() {
        System.out.println("ran");
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main", "ignored"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ran\n");
}
