# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`jay` is a small Java bytecode interpreter in Rust (edition 2024, single dependency: `flate2`). It loads `.class` files from a directory classpath, falls back to the JDK boot image at `$JAVA_HOME/lib/modules` (parsed by an in-house JImage reader), and interprets a focused subset of JVM bytecode. It is deliberately not a full JVM; unsupported opcodes/method shapes fail with an explicit `JayError` carrying an interpreted Java stacktrace. README.md has the authoritative list of supported capabilities — keep it updated when behavior changes.

## Environment

- `JAVA_HOME` must point to a JDK that has `lib/modules` and supports `--release 21` (integration tests shell out to `javac --release 21`). Without it, `ClassResolver::new` fails and all integration tests fail.

## Commands

```sh
cargo test                                    # full suite (unit + integration)
cargo test --test jay_integration collections # one integration module
cargo test runs_hash_map_iteration            # single test by name substring
cargo test --lib heap                         # unit tests in one module
cargo run -- -cp <dir> <fully.qualified.Main> # run a compiled class
```

CI (`.github/workflows/ci.yml`) runs exactly these; all must pass before committing:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## Workflow rules (from AGENTS.md)

- Test-driven: write/update a failing test first, make the smallest change to pass, then refactor green. Documentation-only changes need no tests.
- Update README.md when a change affects setup, usage, behavior, supported capabilities, or dev workflow.
- Document new code (module `//!` docs and `///` on items, matching existing style).
- Split into smaller modules rather than growing one large one.
- Run `cargo fmt` and `cargo clippy` before committing; use Conventional Commits.

## Architecture

Execution path: `main.rs` → `cli::parse_args` → `vm::Vm::new(classpath)` → `Vm::run_main_to_writer` → `Interpreter::execute`.

- `src/lib.rs` — `JayError`/`JayResult` and `JavaStackFrame`. Errors accumulate Java frames top-first as they propagate out of nested `execute` calls (`with_java_stack_frame`), which is how the CLI prints the interpreted stacktrace. A `JayError` is also the exception channel: `JayError::thrown` carries a heap slot for a Java exception in flight, and `JayError::fault(class, message)` marks a VM fault (NPE, bad index, `/ by zero`) that `execute` materializes as a Java object before looking for a handler.
- `src/classfile.rs` — class file parser (`ClassFile`, constant pool, `Method`, `Code`).
- `src/classpath.rs` + `src/jdk.rs` + `src/jimage.rs` — `ClassResolver` tries the directory classpath first, then the JDK boot image (`JImage`). JDK classes like `ArrayList`/`HashMap` are actually interpreted from OpenJDK bytecode, which is why float ops, `Class.desiredAssertionStatus`, typed `HashMap$Node[]` arrays etc. exist.
- `src/vm.rs` — public `Vm` facade; `run_main_to_writer` takes a generic `Write` so tests can capture stdout.
- `src/vm/` — the interpreter, split into `impl Interpreter` blocks across files (all `pub(super)`):
  - `interpreter.rs` — `Interpreter` struct (heap, static fields, class mirrors, init sets, per-run `class_cache` of `Rc<ClassFile>`) and the big `execute_instruction` opcode `match`. Add new opcodes here, delegating to helpers.
  - `invocation.rs` — `invoke_virtual/static/special/interface/dynamic`. **This is where JDK shims are intercepted**: hard-coded matches on `class_name`/`descriptor` (e.g. `PrintStream.println`, `Integer.valueOf`, `String.valueOf`, `Pattern.matches`, `StringConcatFactory` for `invokedynamic`) short-circuit before falling through to interpreting real bytecode. `native_runtime.rs` implements shims that touch heap objects; `native.rs` holds pure logic (date formatting, mini regex engine, `TimeZone`).
  - `natives.rs` — the native-method table. When resolution lands on an `ACC_NATIVE` method, `invoke_native` matches on `(class, name, descriptor)` (`System.arraycopy`, `Object.clone`, `CDS` stubs, ...). Prefer adding real natives here over intercepting Java methods in `invocation.rs`.
  - `strings.rs` — the only Rust-side API for `java.lang.String` objects (`java_string`, `new_java_string`, `is_java_string`, `pop_java_string`); never read string contents from the heap directly elsewhere.
  - `resolution.rs` — class loading via resolver, virtual/interface method lookup up the hierarchy, assignability checks.
  - `lifecycle.rs` — GC root collection (`collect_if_needed`) and `<clinit>` class initialization with re-entrancy guards.
  - `exceptions.rs` — Java exception dispatch: turns `JayError::fault` values into exception objects, finds handlers in the `Code` exception table, and formats `Throwable.toString()` text. `execute` in `interpreter.rs` catches Java-exception errors per instruction and jumps to the handler.
  - `string_shims.rs` / `string_builder_shims.rs` — `String` and `StringBuilder` methods implemented natively over UTF-16 units, since the JDK's byte[]-backed implementations cannot run on native strings.
  - `arithmetic.rs` — pure int/long opcode semantics (wrapping, shift masks, division faults).
  - `heap.rs` — `Heap`/`ObjectRef`, object kinds (`String`, `Instance`, `ObjectArray`, `PrimitiveArray`, `StringBuilder`), mark-sweep GC triggered every `DEFAULT_GC_THRESHOLD` (8) allocations. `Heap::allocate` never collects; only explicit `collect_if_needed(frame)` calls do, so the safe shim pattern is pop arguments, allocate, push the result, then collect. Because GC is that aggressive, any code holding an `ObjectRef` across an allocation must keep it rooted (frame stack/locals, `saved_roots`, `static_fields`, or `class_mirrors`) — several past bugs were of this kind.
  - `frame.rs` — locals + operand stack; long values take two local slots.
  - `value.rs` — `Value` enum (`Null`, `Int`, `Float`, `Long`, `Reference`, plus `PrintStream` sentinel for `System.out`).
  - `descriptors.rs` — field/method descriptor parsing; deliberately rejects unsupported types (e.g. primitive arrays) so failures are explicit.
  - `fields.rs`, `bytecode.rs`, `runtime.rs` — object/field/constant handlers, byte readers and branch predicates, checkcast/argument-popping helpers.

## Tests

- Unit tests live inline (`#[cfg(test)] mod tests`) in each module.
- Integration tests: `tests/jay_integration.rs` pulls in `tests/jay_integration/*.rs` via `#[path]`. javac constant-folds literal expressions, so tests that target a specific opcode must read operands from non-final static fields or method calls. Each test writes a Java source to a temp dir, compiles it with `javac`, runs the built `jay` binary (`support::jay(&["-cp", dir, "Main"])`), and asserts on stdout/stderr. `support.rs` also has a byte-level `ClassEditor` for patching class files (clearing access flags, renaming UTF-8 constants) to produce otherwise-uncompilable inputs.
- New behavior should get an integration test in the matching topic module (`arithmetic`, `arrays`, `control_flow`, `strings`, `objects`, `collections`, `static_methods`, `class_initialization`, `class_loading`, `program_args`, `errors`); add a new module to `jay_integration.rs` if none fits.
