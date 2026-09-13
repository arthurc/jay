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
fn runs_dense_switch() {
    // Four consecutive cases make javac emit tableswitch.
    let stdout = run_main(
        "dense-switch",
        r#"
public class Main {
    static int[] dummy;
    static int k = 3;

    public static void main(String[] args) {
        for (int i = 1; i <= 5; i++) {
            switch (i) {
                case 1: System.out.println("one"); break;
                case 2: System.out.println("two"); break;
                case 3: System.out.println("three"); break;
                case 4: System.out.println("four"); break;
                default: System.out.println("other");
            }
        }
    }
}
"#,
    );

    assert_eq!(stdout, "one\ntwo\nthree\nfour\nother\n");
}

#[test]
fn runs_sparse_switch() {
    // Widely spaced cases make javac emit lookupswitch.
    let stdout = run_main(
        "sparse-switch",
        r#"
public class Main {
    static int k = 100;

    public static void main(String[] args) {
        switch (k) {
            case 1: System.out.println("a"); break;
            case 100: System.out.println("b"); break;
            case 1000: System.out.println("c"); break;
            default: System.out.println("d");
        }
        switch (k + 1) {
            case 1: System.out.println("a"); break;
            case 100: System.out.println("b"); break;
            case 1000: System.out.println("c"); break;
            default: System.out.println("d");
        }
    }
}
"#,
    );

    assert_eq!(stdout, "b\nd\n");
}

#[test]
fn runs_switch_fallthrough_without_default() {
    let stdout = run_main(
        "switch-fallthrough",
        r#"
public class Main {
    static int k = 2;

    public static void main(String[] args) {
        switch (k) {
            case 1:
                System.out.println("one");
            case 2:
                System.out.println("two");
            case 3:
                System.out.println("three");
                break;
            case 4:
                System.out.println("four");
        }
        switch (k * 10) {
            case 1: System.out.println("never");
        }
        System.out.println("done");
    }
}
"#,
    );

    assert_eq!(stdout, "two\nthree\ndone\n");
}
