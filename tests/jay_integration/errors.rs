use crate::support::{compile_java, jay, temp_dir};

#[test]
fn runtime_errors_include_interpreted_java_stack_trace() {
    let root = temp_dir("java-stack-trace");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        outer();
    }

    static void outer() {
        inner();
    }

    static void inner() {
        int[][] grid = new int[2][2];
        System.out.println(grid.length);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(
        !output.status.success(),
        "jay succeeded unexpectedly\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("jay: unsupported bytecode 0xc5 at pc 2"),
        "stderr missing base error:\n{stderr}"
    );
    assert!(
        stderr.contains("  at Main.inner()V (pc "),
        "stderr missing inner frame:\n{stderr}"
    );
    assert!(
        stderr.contains("  at Main.outer()V (pc "),
        "stderr missing outer frame:\n{stderr}"
    );
    assert!(
        stderr.contains("  at Main.main([Ljava/lang/String;)V (pc "),
        "stderr missing main frame:\n{stderr}"
    );
}

fn run_main(name: &str, source: &str) -> String {
    let root = temp_dir(name);
    compile_java(&root, "Main.java", source);

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn catches_thrown_exception_by_supertype() {
    let stdout = run_main(
        "catch-by-supertype",
        r#"
public class Main {
    public static void main(String[] args) {
        try {
            throw new IllegalStateException("boom");
        } catch (RuntimeException e) {
            System.out.println("caught " + e.getMessage());
        }
        System.out.println("after");
    }
}
"#,
    );

    assert_eq!(stdout, "caught boom\nafter\n");
}

#[test]
fn runs_finally_block_on_normal_and_exceptional_paths() {
    let stdout = run_main(
        "finally-paths",
        r#"
public class Main {
    static int attempt(boolean fail) {
        try {
            if (fail) {
                throw new RuntimeException("fail");
            }
            return 1;
        } catch (RuntimeException e) {
            return 2;
        } finally {
            System.out.println("finally " + fail);
        }
    }

    public static void main(String[] args) {
        System.out.println(attempt(false));
        System.out.println(attempt(true));
        try {
            try {
                throw new IllegalArgumentException("inner");
            } finally {
                System.out.println("cleanup");
            }
        } catch (IllegalArgumentException e) {
            System.out.println("outer " + e.getMessage());
        }
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "finally false\n1\nfinally true\n2\ncleanup\nouter inner\n"
    );
}

#[test]
fn propagates_exception_across_method_calls() {
    let stdout = run_main(
        "propagate-across-calls",
        r#"
public class Main {
    static void deepest() {
        throw new UnsupportedOperationException("deep");
    }

    static void middle() {
        deepest();
        System.out.println("unreachable");
    }

    static void outer() {
        middle();
    }

    public static void main(String[] args) {
        try {
            outer();
        } catch (UnsupportedOperationException e) {
            System.out.println("caught " + e.getMessage());
        }
    }
}
"#,
    );

    assert_eq!(stdout, "caught deep\n");
}

#[test]
fn catch_clause_type_is_respected() {
    let stdout = run_main(
        "catch-type-respected",
        r#"
public class Main {
    public static void main(String[] args) {
        try {
            try {
                throw new IllegalStateException("state");
            } catch (IllegalArgumentException e) {
                System.out.println("wrong handler");
            }
        } catch (IllegalStateException e) {
            System.out.println("right handler " + e.getMessage());
        }
        try {
            throw new IllegalStateException();
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.out.println("multi " + e.getMessage());
        }
    }
}
"#,
    );

    assert_eq!(stdout, "right handler state\nmulti null\n");
}

#[test]
fn catches_exceptions_raised_by_the_vm() {
    let stdout = run_main(
        "vm-raised-exceptions",
        r#"
public class Main {
    static String none;
    static int zero = 0;
    static int index = 3;
    static Object text = "text";

    public static void main(String[] args) {
        try {
            System.out.println(none.length());
        } catch (NullPointerException e) {
            System.out.println("npe");
        }
        try {
            System.out.println(10 / zero);
        } catch (ArithmeticException e) {
            System.out.println(e.getMessage());
        }
        try {
            System.out.println(10L % zero);
        } catch (ArithmeticException e) {
            System.out.println("long " + e.getMessage());
        }
        try {
            int[] xs = new int[3];
            xs[index] = 1;
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
        }
        try {
            String[] names = new String[2];
            System.out.println(names[index]);
        } catch (IndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
        }
        try {
            Integer boxed = (Integer) text;
            System.out.println(boxed);
        } catch (ClassCastException e) {
            System.out.println("cce");
        }
        try {
            int[] xs = new int[-index];
        } catch (NegativeArraySizeException e) {
            System.out.println(e.getMessage());
        }
        try {
            "abc".charAt(index);
        } catch (StringIndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
        }
        try {
            Integer.parseInt("12a");
        } catch (NumberFormatException e) {
            System.out.println(e.getMessage());
        }
        try {
            Object[] objects = new String[1];
            objects[0] = Integer.valueOf(1);
        } catch (ArrayStoreException e) {
            System.out.println(e.getMessage());
        }
        try {
            throw null;
        } catch (NullPointerException e) {
            System.out.println("throw null");
        }
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "npe\n/ by zero\nlong / by zero\nIndex 3 out of bounds for length 3\nIndex 3 out of bounds for length 2\ncce\n-3\nIndex 3 out of bounds for length 3\nFor input string: \"12a\"\njava.lang.Integer\nthrow null\n"
    );
}

#[test]
fn uncaught_exception_prints_java_style_header_and_frames() {
    let root = temp_dir("uncaught-exception");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static void fail() {
        throw new IllegalStateException("boom");
    }

    public static void main(String[] args) {
        System.out.println("before");
        fail();
        System.out.println("after");
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "before\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("Exception in thread \"main\" java.lang.IllegalStateException: boom\n"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("  at Main.fail()V (pc "),
        "stderr missing fail frame:\n{stderr}"
    );
    assert!(
        stderr.contains("  at Main.main([Ljava/lang/String;)V (pc "),
        "stderr missing main frame:\n{stderr}"
    );
}

#[test]
fn uncaught_exception_without_message_omits_colon() {
    let root = temp_dir("uncaught-no-message");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static int zero = 0;

    public static void main(String[] args) {
        try {
            throw new RuntimeException();
        } catch (RuntimeException e) {
            System.out.println("first " + e.getMessage());
        }
        System.out.println(1 / zero);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "first null\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .starts_with("Exception in thread \"main\" java.lang.ArithmeticException: / by zero\n"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn custom_exception_classes_carry_fields_and_messages() {
    let stdout = run_main(
        "custom-exception",
        r#"
class ValidationException extends Exception {
    final int code;

    ValidationException(String message, int code) {
        super(message);
        this.code = code;
    }
}

public class Main {
    static void validate(int value) throws ValidationException {
        if (value < 0) {
            throw new ValidationException("negative: " + value, 42);
        }
    }

    public static void main(String[] args) {
        try {
            validate(1);
            validate(-5);
        } catch (ValidationException e) {
            System.out.println(e.getMessage() + " code " + e.code);
            System.out.println(e instanceof Exception);
            System.out.println(e.toString());
        }
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "negative: -5 code 42\ntrue\nValidationException: negative: -5\n"
    );
}
