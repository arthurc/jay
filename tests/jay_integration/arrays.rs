use crate::support::{compile_java, jay, temp_dir};

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
fn runs_int_array_store_load_and_length() {
    let stdout = run_main(
        "int-array",
        r#"
public class Main {
    public static void main(String[] args) {
        int[] xs = new int[3];
        xs[1] = 7;
        xs[2] = xs[1] * 2;
        int sum = 0;
        for (int i = 0; i < xs.length; i++) {
            sum += xs[i];
        }
        System.out.println(xs[0]);
        System.out.println(xs[1] + xs.length);
        System.out.println(sum);
    }
}
"#,
    );

    assert_eq!(stdout, "0\n10\n21\n");
}

#[test]
fn runs_char_byte_and_short_arrays_with_narrowing() {
    let stdout = run_main(
        "narrow-arrays",
        r#"
public class Main {
    static int wide = 300;

    public static void main(String[] args) {
        byte[] bs = new byte[2];
        bs[0] = (byte) wide;
        bs[1] = (byte) -wide;
        char[] cs = new char[2];
        cs[0] = 'A';
        cs[1] = (char) (wide * 300);
        short[] ss = new short[1];
        ss[0] = (short) (wide * 300);
        System.out.println(bs[0]);
        System.out.println(bs[1]);
        System.out.println(cs[0]);
        System.out.println((int) cs[1]);
        System.out.println(ss[0]);
    }
}
"#,
    );

    assert_eq!(stdout, "44\n-44\nA\n24464\n24464\n");
}

#[test]
fn runs_long_float_and_boolean_arrays() {
    let stdout = run_main(
        "long-float-boolean-arrays",
        r#"
public class Main {
    static int three = 3;

    public static void main(String[] args) {
        long[] ls = new long[2];
        ls[0] = 1L << 40;
        ls[1] = ls[0] * three;
        float[] fs = new float[1];
        fs[0] = 0.5f * three;
        boolean[] flags = new boolean[3];
        flags[1] = true;
        System.out.println(ls[1]);
        System.out.println(fs[0]);
        System.out.println(flags[0]);
        System.out.println(flags[1]);
        System.out.println(flags.length);
    }
}
"#,
    );

    assert_eq!(stdout, "3298534883328\n1.5\nfalse\ntrue\n3\n");
}

#[test]
fn passes_primitive_arrays_through_fields_and_parameters() {
    let stdout = run_main(
        "array-fields-parameters",
        r#"
public class Main {
    private int[] values;
    private static char[] letters = new char[2];

    Main(int size) {
        values = new int[size];
    }

    static int total(int[] xs) {
        int sum = 0;
        for (int x : xs) {
            sum += x;
        }
        return sum;
    }

    int[] values() {
        return values;
    }

    public static void main(String[] args) {
        Main main = new Main(4);
        main.values()[3] = 9;
        main.values[0] = 1;
        letters[1] = 'z';
        System.out.println(total(main.values));
        System.out.println(letters[1]);
        System.out.println(main.values().length);
    }
}
"#,
    );

    assert_eq!(stdout, "10\nz\n4\n");
}

#[test]
fn reports_array_index_out_of_bounds() {
    let root = temp_dir("array-oob");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static int index = 3;

    public static void main(String[] args) {
        int[] xs = new int[3];
        System.out.println(xs[index]);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("array index 3 out of bounds for length 3"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn rejects_double_arrays_with_explicit_error() {
    let root = temp_dir("double-array");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        double[] ds = new double[1];
        System.out.println(ds.length);
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported newarray element type double"),
        "stderr:\n{stderr}"
    );
}
