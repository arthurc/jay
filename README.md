# jay

`jay` is a small Java bytecode interpreter written in Rust. It loads compiled
`.class` files from a directory classpath, resolves JDK classes from the JDK
boot image at `JAVA_HOME/lib/modules`, and executes a focused subset of JVM
bytecode.

This is not a full JVM. It is currently useful for experimenting with class
file parsing, JImage lookup, and a minimal interpreter loop.

## Requirements

- Rust with Cargo
- A JDK with `lib/modules`
- `JAVA_HOME` set to that JDK

The integration tests compile Java sources with `javac --release 21`, so a JDK
that supports Java 21 is expected for the full test suite.

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
jay -cp <directory> <fully.qualified.MainClass>
```

For packaged classes, pass the fully qualified class name:

```sh
cargo run -- -cp /tmp/jay-demo/classes com.example.Main
```

## Current Capabilities

`jay` currently supports:

- Directory classpaths for application classes
- JDK boot class lookup through `JAVA_HOME/lib/modules`
- `public static void main(String[] args)` and `public static void main()`
- `System.out.println(String)`, `System.out.println(int)`, `System.out.println(long)`, `System.out.println(boolean)`, and focused `System.out.println(Object)` support for `null`, `String`, `Date`, and Jay-created `LocalDateTime`
- Heap-allocated `String` values managed by a simple internal mark-sweep garbage collector
- Limited heap-allocated `Object[]` arrays with allocation, length, load, and store bytecodes
- Integer constants, local variables, addition, subtraction, multiplication, division, and increment
- Class literals loaded through `ldc` as cached `java.lang.Class` mirrors, with limited `Class.desiredAssertionStatus()` support that reports assertions as disabled
- Limited `long` constants, local variables, fields, method parameters, and return values, including discarding unused `long` results from calls
- Integer comparisons, branches, and simple loops
- Null references in locals, fields, method calls, object arrays, casts, and reference comparison branches
- Static fields and class initialization through static class initializers, including `putstatic`-triggered initialization, re-entrant initialization guards, preserving `putstatic` reference values across initializer-triggered GC, and resolving interface fields inherited from superinterfaces
- Static method calls with `int` and object-reference parameters and `int`, object-reference, or `void` return values
- Same-class and cross-class static method calls
- Simple object allocation and constructor calls
- Constructor calls with `int` and object-reference parameters
- Instance field writes for `int` and object references
- Instance field reads for assigned `int` and object references
- Same-class and cross-class instance method calls with `int` and object-reference parameters and return values
- Interface method calls that dispatch to receiver-class overrides or interface default methods, including methods inherited from superinterfaces
- Private instance method calls invoked with `invokevirtual` resolve to the declaring class (no subclass override dispatch)
- Basic `ArrayList<String>` append and iterator traversal paths used by the integration tests
- Limited Java string concatenation through `StringConcatFactory.makeConcatWithConstants`
- Focused date/time shims for `System.currentTimeMillis()`, `Date.getTime()`, `Date.toString()`, `LocalDateTime.now()`, `TimeZone.getTimeZone(String)`, `SimpleDateFormat.setTimeZone(TimeZone)`, and `SimpleDateFormat` patterns `hh.mm aa` and `dd/MM/yyyy  HH:mm:ss z` with limited GMT/UTC/IST formatting
- Constructor expression statements (for example `new Empty();`)
- Class files up to the parser's supported class file version range

Primitive arrays, string interning, full collection semantics, general
invokedynamic bootstrap execution, long arithmetic, broad date formatting, and
general native/JDK method execution are still unsupported. Unsupported bytecode
or method shapes fail with an explicit error and an interpreted Java stacktrace
that names each active class, method descriptor, and bytecode program counter.

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
