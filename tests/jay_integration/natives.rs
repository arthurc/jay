//! Native methods reached from JDK bytecode: `System.arraycopy`,
//! `Object.clone`, `System.nanoTime`, and the CDS stubs.

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
fn runs_system_arraycopy_and_array_clone() {
    let stdout = run_main(
        "arraycopy-clone",
        r#"
public class Main {
    static int[] numbers = {1, 2, 3, 4, 5};
    static String[] words = {"a", "b", "c"};

    public static void main(String[] args) {
        int[] target = new int[5];
        System.arraycopy(numbers, 1, target, 2, 3);
        System.out.println(target[0] + "," + target[2] + "," + target[4]);
        System.arraycopy(numbers, 0, numbers, 1, 4);
        System.out.println(numbers[0] + "," + numbers[1] + "," + numbers[4]);
        Object[] objects = new Object[3];
        System.arraycopy(words, 0, objects, 0, 3);
        System.out.println(objects[2]);
        int[] copy = numbers.clone();
        copy[0] = 99;
        System.out.println(numbers[0] + "," + copy[0] + "," + copy.length);
        String[] wordsCopy = words.clone();
        System.out.println(wordsCopy[1] + (wordsCopy == words));
        System.out.println(java.util.Arrays.copyOf(numbers, 2).length);
    }
}
"#,
    );

    assert_eq!(stdout, "0,2,4\n1,1,4\nc\n1,99,5\nbfalse\n2\n");
}

#[test]
fn system_arraycopy_reports_java_exceptions() {
    let stdout = run_main(
        "arraycopy-errors",
        r#"
public class Main {
    static int[] numbers = {1, 2, 3};
    static Object[] objects = {"a", Integer.valueOf(1)};
    static String[] words = new String[2];

    public static void main(String[] args) {
        try {
            System.arraycopy(numbers, 1, new int[3], 0, 3);
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
        }
        try {
            System.arraycopy(numbers, 0, new int[3], 0, -1);
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
        }
        try {
            System.arraycopy(numbers, 0, new byte[3], 0, 1);
        } catch (ArrayStoreException e) {
            System.out.println(e.getMessage());
        }
        try {
            System.arraycopy(objects, 0, words, 0, 2);
        } catch (ArrayStoreException e) {
            System.out.println(words[0] + " " + words[1]);
        }
        try {
            System.arraycopy(null, 0, numbers, 0, 1);
        } catch (NullPointerException e) {
            System.out.println("npe");
        }
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "arraycopy: last source index 4 out of bounds for int[3]\narraycopy: length -1 is negative\narraycopy: type mismatch: can not copy int[] into byte[]\na null\nnpe\n"
    );
}

#[test]
fn runs_system_nano_time_and_cds_backed_immutable_collections() {
    let stdout = run_main(
        "nano-time-list-of",
        r#"
public class Main {
    public static void main(String[] args) {
        long before = System.nanoTime();
        long after = System.nanoTime();
        System.out.println(after >= before);
        System.out.println(java.util.List.of(1, 2, 3).size());
        System.out.println(java.util.List.of("x", "y").get(1));
    }
}
"#,
    );

    assert_eq!(stdout, "true\n3\ny\n");
}

#[test]
fn class_mirrors_describe_primitives_arrays_and_classes() {
    let stdout = run_main(
        "class-mirrors",
        r#"
public class Main {
    static int[] numbers = {1, 2};
    static String[] words = {"a", "b", "c"};

    public static void main(String[] args) {
        System.out.println(int.class.getName());
        System.out.println(int.class.isPrimitive());
        System.out.println(int.class == Integer.TYPE);
        System.out.println(byte.class == Byte.TYPE);
        System.out.println(int[].class.getName());
        System.out.println(int[].class.isArray());
        System.out.println(int[].class.getComponentType() == int.class);
        System.out.println(String[].class.getComponentType().getName());
        System.out.println(numbers.getClass() == int[].class);
        System.out.println(words.getClass().isArray());
        System.out.println(String.class.isArray());
        System.out.println(String.class.isPrimitive());
        System.out.println(String.class.isInterface());
        System.out.println(Runnable.class.isInterface());
        System.out.println(java.util.Arrays.copyOfRange(words, 1, 3)[0]);
        System.out.println(java.util.Arrays.copyOf(words, 5).length);
        int[] made = (int[]) java.lang.reflect.Array.newInstance(int.class, 4);
        System.out.println(made.length);
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "int\ntrue\ntrue\ntrue\n[I\ntrue\ntrue\njava.lang.String\ntrue\ntrue\nfalse\nfalse\nfalse\ntrue\nb\n5\n4\n"
    );
}
