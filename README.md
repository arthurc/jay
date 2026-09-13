# jay

`jay` is a small Java bytecode interpreter written in Rust. It loads compiled
`.class` files from a directory classpath, resolves JDK classes from the JDK
boot image at `JAVA_HOME/lib/modules`, and executes a focused subset of JVM
bytecode.

This is not a full JVM. It is currently useful for experimenting with class
file parsing, JImage lookup, and a minimal interpreter loop.

## Requirements

- Rust with Cargo
- A JDK with `lib/modules` (JImage format 1.0 or 1.1, so JDK 9 through JDK 27
  boot images are readable; preview-mode class variants in 1.1 images are
  ignored)
- `JAVA_HOME` set to that JDK

The integration tests compile Java sources with `javac --release 21`, so a JDK
that supports Java 21 is expected for the full test suite. The suite is
exercised against both JDK 21 (CI) and JDK 27.

## Usage

Compile a Java class into a directory:

```sh
mkdir -p /tmp/jay-demo/classes
cat >/tmp/jay-demo/HelloWorld.java <<'JAVA'
public class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello from jay");
    }
}
JAVA
javac --release 21 -d /tmp/jay-demo/classes /tmp/jay-demo/HelloWorld.java
```

Run it with `jay`:

```sh
cargo run -- -cp /tmp/jay-demo/classes HelloWorld
```

The CLI shape is:

```text
jay -cp <directory> <fully.qualified.MainClass> [args...]
```

For packaged classes, pass the fully qualified class name:

```sh
cargo run -- -cp /tmp/jay-demo/classes com.example.Main
```

Anything after the main class is passed to `main(String[] args)` verbatim:

```sh
cargo run -- -cp /tmp/jay-demo/classes com.example.Main first "second arg"
```

## Current Capabilities

`jay` currently supports:

- Directory classpaths for application classes
- JDK boot class lookup through `JAVA_HOME/lib/modules`
- `public static void main(String[] args)` and `public static void main()`, with `args` populated from the command line
- `System.out.println(String)`, `System.out.println(int)`, `System.out.println(long)`, `System.out.println(boolean)`, `System.out.println(char)`, `System.out.println(float)` (shortest round-trip formatting), and focused `System.out.println(Object)` support for `null`, `String`, `Date`, and Jay-created `LocalDateTime`
- Heap-allocated `String` values managed by a simple internal mark-sweep garbage collector
- Limited heap-allocated reference arrays with allocation, length, load, and store bytecodes, including typed JDK arrays such as `HashMap$Node[]`
- Primitive arrays of `boolean`, `byte`, `char`, `short`, `int`, `long`, and `float` with `newarray`, length, and the typed load/store bytecodes; stores narrow to the element width as the JVM specifies. `double[]` and multi-dimensional arrays are rejected with an explicit error
- Runtime reference-array store validation that accepts assignable subtypes (for example, allowing `Integer` values in `Number[]`) and rejects incompatible values (for example, rejecting `Integer` values stored into `String[]`)
- `int` constants, locals, fields, and the full arithmetic set: `+ - * / %`, negation, `<< >> >>>`, `& | ^`, and increment
- `long` constants, locals, fields, parameters, return values, and the full arithmetic set including shifts, bitwise operators, and `lcmp`
- Conversions `i2l`, `l2i`, `l2f`, `i2f`, `f2i`, and the narrowing casts `(byte)`, `(char)`, `(short)`
- Focused `float` support for constants, fields, locals, multiplication, comparison (`fcmpl`/`fcmpg`), and `float` return values
- Class literals loaded through `ldc` as cached `java.lang.Class` mirrors, with limited `Class.desiredAssertionStatus()` support that reports assertions as disabled
- Integer comparisons, branches, loops, `goto_w`, and `switch` on `int` through both `tableswitch` and `lookupswitch`
- `instanceof` against classes, interfaces, and reference array types
- Operand stack shuffles `dup`, `dup_x1`, `dup_x2`, `dup2`, `dup2_x1`, `swap`, `pop`, and `pop2`
- Null references in locals, fields, method calls, object arrays, casts, and reference comparison branches
- Static fields and class initialization through static class initializers, including `putstatic`-triggered initialization, re-entrant initialization guards, preserving `putstatic` reference values across initializer-triggered GC, and resolving interface fields inherited from superinterfaces
- Static method calls with `int` and object-reference parameters and `int`, object-reference, or `void` return values
- Same-class and cross-class static method calls
- Reference assignability checks across class and interface hierarchies, including passing `String` values to JDK APIs that declare compatible supertypes such as `CharSequence`
- Simple object allocation and constructor calls
- Constructor calls with `int` and object-reference parameters
- Instance field writes for `int` and object references
- Instance field reads for assigned `int` and object references
- Same-class and cross-class instance method calls with `int` and object-reference parameters and return values
- Interface method calls that dispatch to receiver-class overrides or interface default methods, including methods inherited from superinterfaces
- Private instance method calls invoked with `invokevirtual` resolve to the declaring class (no subclass override dispatch)
- Basic `ArrayList<String>` append and iterator traversal paths used by the integration tests
- Basic `HashMap<String, Integer>` insertion and entry-set iteration paths used by the integration tests
- Java string concatenation through `StringConcatFactory.makeConcatWithConstants`
- `String` instance methods implemented natively over UTF-16 code units: `length`, `isEmpty`, `charAt`, `equals`, `equalsIgnoreCase`, `compareTo`, `hashCode`, `toString`, `contains`, `startsWith`, `endsWith`, `indexOf` (char and `String`, with and without a start index), `lastIndexOf`, `substring`, `trim`, `toUpperCase`, `toLowerCase`, `concat`, `replace` (char and `CharSequence`), and `toCharArray`
- `String` constructors `String()`, `String(String)`, and `String(char[])`, plus `String.valueOf` for `int`, `long`, `char`, `boolean`, and `char[]`, `Integer.toString(int)`, `Long.toString(long)`, and `Integer.parseInt(String)`
- `switch` on `String` values (javac's `hashCode` + `equals` lowering)
- `StringBuilder` backed by a native buffer: constructors `()`, `(int)`, `(String)`, `(CharSequence)`, `append` for `String`, `Object`, `CharSequence`, `int`, `long`, `float`, `char`, and `boolean`, plus `toString`, `length`, `isEmpty`, `charAt`, `reverse`, and `setLength`
- String concatenation and `println(Object)` format `char`, `boolean`, `long`, `float`, `Integer`, `StringBuilder`, and arbitrary objects through their interpreted `toString()`
- `byte`, `char`, and `short` fields, parameters, and return values, carried as `int` values
- Focused `String.valueOf(Object)` behavior with `null` handling, `Integer`/`String` fast paths, and virtual `toString()` fallback for general objects, VM-side default `Object.toString()` identity formatting, and array receivers via `Object`-style formatting
- Focused `Pattern.matches(String, CharSequence)` support for the regex constructs exercised by the integration tests, including `.`, `*`, `+`, exact repetition like `{4}`, digit escapes like `\d`, and simple character classes such as `[0-9]`
- Focused date/time shims for `System.currentTimeMillis()`, `Date.getTime()`, `Date.toString()`, `LocalDateTime.now()`, `TimeZone.getTimeZone(String)`, `SimpleDateFormat.setTimeZone(TimeZone)`, and `SimpleDateFormat` patterns `hh.mm aa` and `dd/MM/yyyy  HH:mm:ss z` with limited GMT/UTC/IST formatting
- Constructor expression statements (for example `new Empty();`)
- Class files with major versions 45 through 71 (Java 1.1 through Java 27)
- Each class file is read and parsed once per run and shared by every call, field lookup, and hierarchy walk

`double` values, multi-dimensional arrays, string interning, full collection semantics,
general invokedynamic bootstrap execution, broad date formatting, general regex
execution, and general native/JDK method execution are still unsupported. Unsupported bytecode or method shapes fail with an explicit error
and an interpreted Java stacktrace that names each active class, method
descriptor, and bytecode program counter.

## Development

Run the test suite:

```sh
cargo test
```

Run the same checks expected by CI:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

The project uses a test-driven workflow. Add or update a failing test for the
behavior first, make the smallest implementation change, then refactor with the
suite green.
