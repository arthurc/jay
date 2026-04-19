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
- `System.out.println(String)` and `System.out.println(int)`
- Integer constants, local variables, addition, subtraction, multiplication, division, and increment
- Integer comparisons, branches, and simple loops
- Static method calls with `int` parameters and `int` or `void` return values
- Same-class and cross-class static method calls
- Class files up to the parser's supported class file version range

Unsupported bytecode or method shapes fail with an explicit error.

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
