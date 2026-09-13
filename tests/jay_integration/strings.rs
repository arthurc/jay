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

#[test]
fn runs_string_builder_appends() {
    let stdout = run_main(
        "string-builder-appends",
        r#"
public class Main {
    static int number = 7;
    static long big = 1L << 40;

    public static void main(String[] args) {
        StringBuilder builder = new StringBuilder();
        builder.append("a").append(number).append('c').append(big).append(number > 1);
        builder.append((Object) null).append(Integer.valueOf(3));
        System.out.println(builder.toString());
        System.out.println(builder.length());
        System.out.println(builder.charAt(1));
        System.out.println(builder);
        System.out.println("value: " + builder);
        StringBuilder seeded = new StringBuilder("seed");
        seeded.append(new StringBuilder("ling"));
        System.out.println(seeded.reverse());
        seeded.setLength(2);
        System.out.println(seeded.toString());
        System.out.println(new StringBuilder(16).length());
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "a7c1099511627776truenull3\n25\n7\na7c1099511627776truenull3\nvalue: a7c1099511627776truenull3\ngnildees\ngn\n0\n"
    );
}

#[test]
fn string_builder_survives_garbage_collection_and_passes_between_methods() {
    let stdout = run_main(
        "string-builder-gc",
        r#"
public class Main {
    static void fill(StringBuilder target, int count) {
        for (int i = 0; i < count; i++) {
            target.append(i).append(",");
        }
    }

    static String describe(Object value) {
        return "[" + value + "]";
    }

    public static void main(String[] args) {
        StringBuilder builder = new StringBuilder();
        fill(builder, 12);
        System.out.println(builder.toString());
        System.out.println(describe(builder));
        System.out.println(builder.append(new Main()).length() > 25);
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "0,1,2,3,4,5,6,7,8,9,10,11,\n[0,1,2,3,4,5,6,7,8,9,10,11,]\ntrue\n"
    );
}

#[test]
fn string_literals_are_interned_and_new_strings_are_distinct() {
    let stdout = run_main(
        "string-interning",
        r#"
public class Main {
    static String first = "shared";
    static String second = "shared";

    public static void main(String[] args) {
        System.out.println(first == second);
        String copy = new String(first);
        System.out.println(copy == first);
        System.out.println(copy.equals(first));
        System.out.println(copy.intern() == first);
        System.out.println(first.concat("").intern() == first);
        System.out.println(new String().isEmpty());
    }
}
"#,
    );

    assert_eq!(stdout, "true\nfalse\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn runs_utf16_strings_through_the_jdk_string_bytecode() {
    let stdout = run_main(
        "string-utf16",
        r#"
public class Main {
    static String text = "héllo 😀 wörld";
    static String latin = "héllo";
    static int two = 2;

    public static void main(String[] args) {
        System.out.println(text.length());
        System.out.println(text.charAt(1));
        System.out.println((int) text.charAt(6));
        System.out.println(text.indexOf("w"));
        System.out.println(text.substring(6, 8).length());
        System.out.println(text.substring(9));
        System.out.println(text.hashCode());
        System.out.println(text.equals("héllo 😀 wörld"));
        System.out.println(text.startsWith(latin));
        System.out.println(latin.equals(text.substring(0, 5)));
        System.out.println(text.toCharArray().length);
        System.out.println(text.replace('ö', 'o'));
        System.out.println(text.codePointAt(6));
        System.out.println(String.valueOf(text.toCharArray()).equals(text));
        System.out.println(text.compareTo(latin) > 0);
        System.out.println(text.charAt(two) == latin.charAt(two));
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "14\né\n55357\n9\n2\nwörld\n-22170944\ntrue\ntrue\ntrue\n14\nhéllo \u{1F600} world\n128512\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn runs_unshimmed_jdk_string_methods() {
    let stdout = run_main(
        "string-more-methods",
        r#"
public class Main {
    static String text = "  Hello, World  ";
    static String csv = "a,b,,c";
    static int three = 3;

    public static void main(String[] args) {
        System.out.println(text.strip());
        System.out.println(text.isBlank());
        System.out.println("   ".isBlank());
        System.out.println("ab".repeat(three));
        System.out.println(text.indexOf("o", 8));
        System.out.println(text.lastIndexOf('l'));
        System.out.println("apple".compareToIgnoreCase("APPLE"));
        System.out.println(text.regionMatches(true, 2, "HELLO", 0, 5));
        System.out.println(String.join("-", "x", "y", "z"));
        String[] parts = csv.split(",");
        System.out.println(parts.length + ":" + parts[0] + parts[1] + parts[2] + parts[3]);
        System.out.println(text.trim().toCharArray()[0]);
        System.out.println(String.valueOf(new char[] {'o', 'k'}));
        System.out.println(Integer.toHexString(255));
        System.out.println(Integer.toString(255, 2));
        System.out.println(Integer.parseInt("ff", 16));
        System.out.println(Long.parseLong("-9000000000"));
        System.out.println(Character.isDigit('7'));
        System.out.println(String.format("%s has %d items%%", "cart", three));
        System.out.println("x".equals(null));
        System.out.println(text.contains("World"));
    }
}
"#,
    );

    assert_eq!(
        stdout,
        "Hello, World\nfalse\ntrue\nababab\n10\n12\n0\ntrue\nx-y-z\n4:abc\nH\nok\nff\n11111111\n255\n-9000000000\ntrue\ncart has 3 items%\nfalse\ntrue\n"
    );
}

#[test]
fn string_exceptions_carry_jdk_messages() {
    let stdout = run_main(
        "string-exceptions",
        r#"
public class Main {
    static String text = "abc";
    static String nothing = null;

    public static void main(String[] args) {
        try {
            text.substring(2, 1);
        } catch (StringIndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
        }
        try {
            text.charAt(-1);
        } catch (StringIndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
        }
        try {
            Integer.parseInt("12x");
        } catch (NumberFormatException e) {
            System.out.println(e.getMessage());
        }
        try {
            nothing.length();
        } catch (NullPointerException e) {
            System.out.println("npe");
        }
        try {
            "ab".repeat(-1);
        } catch (IllegalArgumentException e) {
            System.out.println(e.getMessage());
        }
    }
}
"#,
    );

    // JDK 21 spells the substring message "begin 2, end 1, length 3";
    // JDK 27 reports it through Preconditions as a range.
    let (substring_message, rest) = stdout.split_once('\n').unwrap();
    assert!(
        matches!(
            substring_message,
            "begin 2, end 1, length 3" | "Range [2, 1) out of bounds for length 3"
        ),
        "substring message: {substring_message}"
    );
    assert_eq!(
        rest,
        "Index -1 out of bounds for length 3\nFor input string: \"12x\"\nnpe\ncount is negative: -1\n"
    );
}

#[test]
fn strings_survive_garbage_collection_in_loops() {
    let stdout = run_main(
        "string-gc",
        r#"
public class Main {
    static String[] keep = new String[40];

    static String build(int i) {
        String prefix = "item-" + i;
        return prefix.substring(0, 4).concat(String.valueOf(i)).replace('i', 'I');
    }

    public static void main(String[] args) {
        for (int i = 0; i < keep.length; i++) {
            keep[i] = build(i);
        }
        int total = 0;
        for (String value : keep) {
            total += value.length() + value.hashCode() % 7;
        }
        System.out.println(keep[0] + " " + keep[39] + " " + total);
        System.out.println(keep[7].equals(build(7)));
    }
}
"#,
    );

    assert_eq!(stdout, "Item0 Item39 175\ntrue\n");
}
