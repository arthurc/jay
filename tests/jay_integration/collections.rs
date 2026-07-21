use crate::support::{compile_java, jay, temp_dir};

#[test]
fn runs_array_list_iterator_has_next() {
    let root = temp_dir("array-list-iterator-has-next");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.util.ArrayList;
import java.util.Iterator;

public class Main {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<String>();
        list.add("Geeks");
        list.add("for Geeks");

        Iterator<String> iterator = list.iterator();
        System.out.println(iterator.hasNext());
        iterator.next();
        System.out.println(iterator.hasNext());
        iterator.next();
        System.out.println(iterator.hasNext());
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
        "true\ntrue\nfalse\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_array_list_iterator_next_with_string_checkcast() {
    let root = temp_dir("array-list-iterator-next-checkcast");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.util.ArrayList;
import java.util.Iterator;

public class Main {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<String>();
        list.add("Element1");
        list.add("Element2");
        list.add("Element3");

        Iterator<String> iterator = list.iterator();
        System.out.println(iterator.next());
        System.out.println(iterator.next());
        System.out.println(iterator.next());
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
        "Element1\nElement2\nElement3\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_hash_map_iteration_with_string_concat() {
    let root = temp_dir("hash-map-iteration");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.util.HashMap;
import java.util.Map;

public class Main {
    public static void main(String[] args) {
        HashMap<String, Integer> hashMap = new HashMap<>();
        hashMap.put("John", 25);
        hashMap.put("Jane", 30);
        hashMap.put("Jim", 35);

        for (Map.Entry<String, Integer> entry : hashMap.entrySet()) {
            System.out.println(entry.getKey() + " -> " + entry.getValue());
        }
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

    let mut lines = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    lines.sort();

    assert_eq!(
        lines,
        vec![
            "Jane -> 30".to_string(),
            "Jim -> 35".to_string(),
            "John -> 25".to_string()
        ]
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}
