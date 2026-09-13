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
fn runs_string_length_char_at_and_equality() {
    let stdout = run_main(
        "string-basics",
        r#"
public class Main {
    static String text = "héllo";
    static Object other = "héllo";

    public static void main(String[] args) {
        System.out.println(text.length());
        System.out.println(text.charAt(1));
        System.out.println(text.isEmpty());
        System.out.println("".isEmpty());
        System.out.println(text.equals(other));
        System.out.println(text.equals("HÉLLO"));
        System.out.println(text.equalsIgnoreCase("HÉLLO"));
        System.out.println(text.equals(null));
        System.out.println(text.equals(Integer.valueOf(1)));
        System.out.println(text.hashCode());
        System.out.println("apple".compareTo("banana"));
        System.out.println("b".compareTo("a"));
        System.out.println("same".compareTo("same"));
        System.out.println(text.toString() == text);
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "5\né\nfalse\ntrue\ntrue\nfalse\ntrue\nfalse\nfalse\n103094734\n-1\n1\n0\ntrue\n"
    );
}

#[test]
fn runs_string_searching_and_slicing() {
    let stdout = run_main(
        "string-search-slice",
        r#"
public class Main {
    static String text = "hello world";
    static int two = 2;

    public static void main(String[] args) {
        System.out.println(text.substring(6));
        System.out.println(text.substring(1, 3));
        System.out.println(text.indexOf("o"));
        System.out.println(text.indexOf("o", 5));
        System.out.println(text.indexOf('w'));
        System.out.println(text.indexOf("zzz"));
        System.out.println(text.lastIndexOf("o"));
        System.out.println(text.contains("lo w"));
        System.out.println(text.startsWith("hell"));
        System.out.println(text.endsWith("world"));
        System.out.println(text.endsWith("worlds"));
        System.out.println("  padded  ".trim());
        System.out.println(text.toUpperCase());
        System.out.println("MiXeD".toLowerCase());
        System.out.println(text.concat("!"));
        System.out.println(text.replace('l', 'L'));
        System.out.println(text.replace("world", "there"));
        System.out.println(text.charAt(two));
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "world\nel\n4\n7\n6\n-1\n7\ntrue\ntrue\ntrue\nfalse\npadded\nHELLO WORLD\nmixed\nhello world!\nheLLo worLd\nhello there\nl\n"
    );
}

#[test]
fn runs_string_conversions() {
    let stdout = run_main(
        "string-conversions",
        r#"
public class Main {
    static int number = 42;
    static long big = 1L << 40;
    static String digits = "-123";

    public static void main(String[] args) {
        System.out.println(String.valueOf(number));
        System.out.println(String.valueOf(big));
        System.out.println(String.valueOf('c'));
        System.out.println(String.valueOf(number > 1));
        System.out.println(Integer.toString(number));
        System.out.println(Integer.parseInt(digits) + 1);
        char[] chars = "abc".toCharArray();
        System.out.println(chars.length);
        System.out.println(chars[2]);
        System.out.println(new String(chars));
        System.out.println(String.valueOf(chars));
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "42\n1099511627776\nc\ntrue\n42\n-122\n3\nc\nabc\nabc\n"
    );
}

#[test]
fn runs_string_switch() {
    let stdout = run_main(
        "string-switch",
        r#"
public class Main {
    static String[] words = {"apple", "banana", "cherry", "durian"};

    public static void main(String[] args) {
        for (String word : words) {
            switch (word) {
                case "apple":
                    System.out.println("red");
                    break;
                case "banana":
                    System.out.println("yellow");
                    break;
                case "cherry":
                    System.out.println("dark red");
                    break;
                default:
                    System.out.println("unknown");
            }
        }
    }
}
"#,
    );

    assert_eq!(stdout, "red\nyellow\ndark red\nunknown\n");
}

#[test]
fn reports_string_index_out_of_bounds() {
    let root = temp_dir("string-oob");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    static int index = 5;

    public static void main(String[] args) {
        System.out.println("abc".charAt(index));
    }
}
"#,
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Index 5 out of bounds for length 3"),
        "stderr:\n{stderr}"
    );
}
