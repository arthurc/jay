use crate::support::{compile_java, compile_java_sources, jay, replace_utf8_constant, temp_dir};

#[test]
fn runs_simple_object_construction() {
    let root = temp_dir("simple-object-construction");
    compile_java(
        &root,
        "Main.java",
        r#"
class Empty {
}

public class Main {
    public static void main(String[] args) {
        Empty value = new Empty();
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_instance_field_assignments() {
    let root = temp_dir("instance-field-assignments");
    compile_java(
        &root,
        "Main.java",
        r#"
class Car {
    String make;
    int year;

    Car(String make, int year) {
        this.make = make;
        this.year = year;
    }
}

class Garage {
    Car car;
}

public class Main {
    public static void main(String[] args) {
        Car car = new Car("Toyota", 2020);
        Garage garage = new Garage();
        garage.car = car;
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_constructor_with_object_reference_parameter() {
    let root = temp_dir("constructor-object-reference-parameter");
    compile_java(
        &root,
        "Main.java",
        r#"
class Engine {
    String serial;

    Engine(String serial) {
        this.serial = serial;
    }
}

class Car {
    Engine engine;

    Car(Engine engine) {
        this.engine = engine;
    }
}

public class Main {
    public static void main(String[] args) {
        Engine engine = new Engine("E-123");
        Car car = new Car(engine);
        System.out.println(car.engine.serial);
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "E-123\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_instance_field_reads() {
    let root = temp_dir("instance-field-reads");
    compile_java(
        &root,
        "Main.java",
        r#"
class Car {
    String make;
    int year;
}

public class Main {
    public static void main(String[] args) {
        Car car = new Car();
        car.make = "Toyota";
        car.year = 2020;
        System.out.println(car.make);
        System.out.println(car.year);
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Toyota\n2020\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_long_fields_parameters_returns_and_println() {
    let root = temp_dir("long-fields-parameters-returns");
    compile_java(
        &root,
        "Main.java",
        r#"
class LongBox {
    long value;

    LongBox(long value, int marker) {
        this.value = value;
        System.out.println(marker);
    }

    long value() {
        return value;
    }
}

public class Main {
    static long pass(long value, int marker) {
        System.out.println(marker);
        return value;
    }

    public static void main(String[] args) {
        LongBox box = new LongBox(1234567890123L, 7);
        System.out.println(box.value());
        System.out.println(pass(1L, 8));
        System.out.println(0L);
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
        "7\n1234567890123\n8\n1\n0\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_date_constructor_with_current_time_millis_long() {
    let root = temp_dir("date-constructor-current-time-millis-long");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        new java.util.Date();
        System.out.println("ok");
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn reads_date_time_when_calendar_cache_is_null() {
    let root = temp_dir("date-get-time-null-calendar-cache");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        java.util.Date date = new java.util.Date(123L);
        System.out.println(date.getTime());
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "123\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn formats_date_with_test2_time_pattern() {
    let root = temp_dir("date-format-test2-time-pattern");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.text.SimpleDateFormat;
import java.util.Date;

public class Main {
    public static void main(String[] args) {
        Date date = new Date(0L);
        System.out.println("Current Time is : " + date);
        SimpleDateFormat formatTime = new SimpleDateFormat("hh.mm aa");
        String time = formatTime.format(date);
        System.out.println("Current Time in AM/PM Format is : " + time);
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
        "Current Time is : Thu Jan 01 00:00:00 GMT 1970\n\
Current Time in AM/PM Format is : 12.00 AM\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn formats_date_with_test2_date_time_zone_pattern() {
    let root = temp_dir("date-format-test2-date-time-zone-pattern");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.TimeZone;

public class Main {
    public static void main(String[] args) {
        Date date = new Date(0L);
        SimpleDateFormat formatDate = new SimpleDateFormat("dd/MM/yyyy  HH:mm:ss z");
        formatDate.setTimeZone(TimeZone.getTimeZone("IST"));
        System.out.println(formatDate.format(date));
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
        "01/01/1970  05:30:00 IST\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn prints_date_through_object_println() {
    let root = temp_dir("date-object-println");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.util.Date;

public class Main {
    public static void main(String[] args) {
        System.out.println((Object) new Date(0L));
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
        "Thu Jan 01 00:00:00 GMT 1970\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn prints_local_date_time_now_through_object_println() {
    let root = temp_dir("local-date-time-now-object-println");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.time.LocalDateTime;

public class Main {
    public static void main(String[] args) {
        System.out.println(LocalDateTime.now());
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
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.len(), "1970-01-01T00:00:00\n".len());
    assert!(
        stdout.chars().enumerate().all(|(index, character)| {
            matches!(
                (index, character),
                (0..=3, '0'..='9')
                    | (4, '-')
                    | (5..=6, '0'..='9')
                    | (7, '-')
                    | (8..=9, '0'..='9')
                    | (10, 'T')
                    | (11..=12, '0'..='9')
                    | (13, ':')
                    | (14..=15, '0'..='9')
                    | (16, ':')
                    | (17..=18, '0'..='9')
                    | (19, '\n')
            )
        }),
        "stdout should be ISO-like LocalDateTime, got {stdout:?}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn prints_null_through_object_println() {
    let root = temp_dir("null-object-println");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        System.out.println((Object) null);
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "null\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn prints_string_through_object_println() {
    let root = temp_dir("string-object-println");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        System.out.println((Object) "hello");
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_static_method_with_object_reference_parameter_and_return() {
    let root = temp_dir("static-object-reference-parameter-return");
    compile_java(
        &root,
        "Main.java",
        r#"
class Box {
    String value;

    Box(String value) {
        this.value = value;
    }
}

class Boxes {
    static Box identity(Box box) {
        return box;
    }
}

public class Main {
    public static void main(String[] args) {
        Box box = new Box("static");
        System.out.println(Boxes.identity(box).value);
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "static\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_instance_method_with_object_reference_parameter_and_return() {
    let root = temp_dir("instance-object-reference-parameter-return");
    compile_java(
        &root,
        "Main.java",
        r#"
class Box {
    String value;

    Box(String value) {
        this.value = value;
    }
}

class Echo {
    Box identity(Box box) {
        return box;
    }
}

public class Main {
    public static void main(String[] args) {
        Box box = new Box("instance");
        Echo echo = new Echo();
        System.out.println(echo.identity(box).value);
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "instance\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn rejects_reference_arguments_that_do_not_match_descriptor_type() {
    let root = temp_dir("rejects-reference-argument-type-mismatch");
    compile_java_sources(
        &root,
        &[
            (
                "Helper.java",
                r#"
class Helper {
    static Object identity(Object value) {
        return value;
    }
}
"#,
            ),
            (
                "Box.java",
                r#"
class Box {
}
"#,
            ),
            (
                "Main.java",
                r#"
public class Main {
    public static void main(String[] args) {
        Helper.identity(new Box());
    }
}
"#,
            ),
        ],
    );

    replace_utf8_constant(
        &root,
        "Helper.class",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        "(Ljava/lang/String;)Ljava/lang/Object;",
    );
    replace_utf8_constant(
        &root,
        "Main.class",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        "(Ljava/lang/String;)Ljava/lang/Object;",
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success(), "jay unexpectedly succeeded");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("invokestatic target Helper.identity(Ljava/lang/String;)Ljava/lang/Object; received Box, expected java.lang.String")
    );
}

#[test]
fn rejects_reference_returns_that_do_not_match_descriptor_type() {
    let root = temp_dir("rejects-reference-return-type-mismatch");
    compile_java_sources(
        &root,
        &[
            (
                "Helper.java",
                r#"
class Box {
}

class Helper {
    static Object make() {
        return new Box();
    }
}
"#,
            ),
            (
                "Main.java",
                r#"
public class Main {
    public static void main(String[] args) {
        Helper.make();
    }
}
"#,
            ),
        ],
    );

    replace_utf8_constant(
        &root,
        "Helper.class",
        "()Ljava/lang/Object;",
        "()Ljava/lang/String;",
    );
    replace_utf8_constant(
        &root,
        "Main.class",
        "()Ljava/lang/Object;",
        "()Ljava/lang/String;",
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(!output.status.success(), "jay unexpectedly succeeded");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("invokestatic target Helper.make()Ljava/lang/String; returned Box, expected java.lang.String")
    );
}

#[test]
fn accepts_reference_arguments_and_returns_assignable_to_interfaces() {
    let root = temp_dir("accepts-reference-interface-assignability");
    compile_java(
        &root,
        "Main.java",
        r#"
interface Named {
}

class Box implements Named {
}

class Helper {
    static Named identity(Named value) {
        return value;
    }
}

public class Main {
    public static void main(String[] args) {
        Helper.identity(new Box());
        System.out.println("ok");
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn accepts_string_arguments_assignable_to_jdk_interfaces() {
    let root = temp_dir("accepts-string-charsequence-assignability");
    compile_java(
        &root,
        "Main.java",
        r#"
import java.util.regex.Pattern;

public class Main {
    public static void main(String[] args) {
        System.out.println(Pattern.matches("geeks.*", "geeksforgeeks"));
        System.out.println(Pattern.matches("geeks[0-9]+", "geeks12s"));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "true\nfalse\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn invokes_interface_method_declared_on_superinterface() {
    let root = temp_dir("invokeinterface-superinterface-method");
    compile_java(
        &root,
        "Main.java",
        r#"
interface A {
    void ping();
}

interface B extends A {
}

class Impl implements B {
    public void ping() {
        System.out.println("ok");
    }
}

public class Main {
    static void call(B value) {
        value.ping();
    }

    public static void main(String[] args) {
        call(new Impl());
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn invokeinterface_falls_back_to_interface_default_method() {
    let root = temp_dir("invokeinterface-default-method-fallback");
    compile_java(
        &root,
        "Main.java",
        r#"
interface A {
    default void ping() {
        System.out.println("default");
    }
}

class Impl implements A {
}

public class Main {
    static void call(A value) {
        value.ping();
    }

    public static void main(String[] args) {
        call(new Impl());
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "default\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_same_class_instance_int_method() {
    let root = temp_dir("same-class-instance-int-method");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    int add(int left, int right) {
        return left + right;
    }

    public static void main(String[] args) {
        Main value = new Main();
        System.out.println(value.add(2, 3));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "5\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_cross_class_instance_string_method() {
    let root = temp_dir("cross-class-instance-string-method");
    compile_java_sources(
        &root,
        &[
            (
                "Greeter.java",
                r#"
public class Greeter {
    String message() {
        return "hello";
    }
}
"#,
            ),
            (
                "Main.java",
                r#"
public class Main {
    public static void main(String[] args) {
        Greeter greeter = new Greeter();
        System.out.println(greeter.message());
    }
}
"#,
            ),
        ],
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_string_concat_with_string_and_int_values() {
    let root = temp_dir("string-concat-string-int");
    compile_java(
        &root,
        "Main.java",
        r#"
public class Main {
    public static void main(String[] args) {
        String make = "Toyota";
        int year = 2020;
        System.out.println("Make: " + make);
        System.out.println("Year: " + year);
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
        "Make: Toyota\nYear: 2020\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_instance_method_with_field_reads_and_string_concat() {
    let root = temp_dir("instance-method-field-read-string-concat");
    compile_java(
        &root,
        "Main.java",
        r#"
class Car {
    String make;
    String model;
    int year;

    public void displayInfo() {
        System.out.println("Make: " + make);
        System.out.println("Model: " + model);
        System.out.println("Year: " + year);
    }
}

public class Main {
    public static void main(String[] args) {
        Car car = new Car();
        car.make = "Toyota";
        car.model = "Corolla";
        car.year = 2020;
        car.displayInfo();
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
        "Make: Toyota\nModel: Corolla\nYear: 2020\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn resolves_inherited_instance_fields_to_declaring_class() {
    let root = temp_dir("inherited-instance-field-resolution");
    compile_java(
        &root,
        "Main.java",
        r#"
class Parent {
    int value;

    int read() {
        return value;
    }
}

class Child extends Parent {
    void set(int next) {
        value = next;
    }
}

public class Main {
    public static void main(String[] args) {
        Child child = new Child();
        child.set(7);
        System.out.println(child.read());
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn invokes_private_virtual_targets_without_subclass_dispatch() {
    let root = temp_dir("private-invokevirtual-no-subclass-dispatch");
    compile_java_sources(
        &root,
        &[
            (
                "Parent.java",
                r#"
public class Parent {
    private String marker() {
        return "A";
    }

    String callMarker() {
        return marker();
    }
}
"#,
            ),
            (
                "Child.java",
                r#"
public class Child extends Parent {
    String marker() {
        return "B";
    }
}
"#,
            ),
            (
                "Main.java",
                r#"
public class Main {
    public static void main(String[] args) {
        Child child = new Child();
        System.out.println(child.callMarker());
        System.out.println(child.marker());
    }
}
"#,
            ),
        ],
    );

    let output = jay(&["-cp", root.to_str().unwrap(), "Main"]);

    assert!(
        output.status.success(),
        "jay failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "A\nB\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn runs_constructor_expression_statement() {
    let root = temp_dir("constructor-expression-statement");
    compile_java(
        &root,
        "Main.java",
        r#"
class Empty {
}

public class Main {
    public static void main(String[] args) {
        new Empty();
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}
