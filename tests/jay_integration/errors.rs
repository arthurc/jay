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
        java.util.Map.Entry[] entries = new java.util.Map.Entry[1];
        System.out.println(entries.length);
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
        stderr.contains("jay: unsupported anewarray component java.util.Map$Entry"),
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
