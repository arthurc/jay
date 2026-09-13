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

#[test]
fn runs_integer_remainder_negation_and_shifts() {
    let root = temp_dir("integer-remainder-negation-shifts");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static int ten = 10;
    static int three = 3;
    static int negative = -17;
    static int shift = 33;

    public static void main(String[] args) {
        System.out.println(ten % three);
        System.out.println(negative % three);
        System.out.println(-ten);
        System.out.println(ten << shift);
        System.out.println(negative >> 1);
        System.out.println(negative >>> 28);
        System.out.println(ten | three);
        System.out.println(ten & three);
        System.out.println(ten ^ three);
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1\n-2\n-10\n20\n-9\n15\n11\n2\n9\n"
    );
}

#[test]
fn runs_long_arithmetic_and_comparison() {
    let root = temp_dir("long-arithmetic-comparison");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static long big = 1L << 40;
    static long three = 3L;
    static int ten = 10;
    static int shift = 65;

    public static void main(String[] args) {
        System.out.println(big * three + 5);
        System.out.println(big - three);
        System.out.println(big / three);
        System.out.println(big % three);
        System.out.println(-big);
        System.out.println(three << shift);
        System.out.println(-big >> 38);
        System.out.println(-big >>> 60);
        System.out.println(big & (big + 1));
        System.out.println(big | three);
        System.out.println(big ^ big);
        System.out.println(big > ten);
        System.out.println(three == 3L);
        System.out.println(three < big);
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "3298534883333\n1099511627773\n366503875925\n1\n-1099511627776\n6\n-4\n15\n1099511627776\n1099511627779\n0\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn runs_int_long_conversions() {
    let root = temp_dir("int-long-conversions");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static int wide = 300;
    static int negative = -1;
    static long big = 1L << 40;

    public static void main(String[] args) {
        System.out.println((byte) wide);
        System.out.println((short) (wide * 300));
        System.out.println((int) (char) negative);
        System.out.println((long) wide * wide * wide);
        System.out.println((int) (big + 7));
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "44\n24464\n65535\n27000000\n7\n"
    );
}

#[test]
fn prints_chars_and_floats() {
    let root = temp_dir("print-chars-floats");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static int ten = 10;
    static float half = 0.5f;

    public static void main(String[] args) {
        System.out.println((char) (ten + 55));
        System.out.println(half * 3);
        System.out.println(half * 4);
        System.out.println(half > 0.25f);
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "A\n1.5\n2.0\ntrue\n"
    );
}
