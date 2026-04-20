use crate::support::{compile_java, jay, temp_dir};

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
