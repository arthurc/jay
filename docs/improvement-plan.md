# Jay improvement plan

Five improvements to the `jay` Java bytecode interpreter, ordered so that each
one builds on the last. Written 2026-09-13 against commit `394bf23` on branch
`jdk-27-support`. Baseline: `cargo test` passes (89 unit, 75 integration) with
`JAVA_HOME` pointing at JDK 27.

This document is written for an implementing agent. Read `CLAUDE.md` and
`AGENTS.md` first; their rules apply to every step below and are not repeated
in full here.

---

## 0. How to work this plan

**Order.** Implement in the order 1 → 2 → 3 → 4 → 5. Items 1 and 2 are
independent of everything. Item 3 must precede 4 because JDK array code uses
the conversion and shift opcodes it adds. Item 5 is last and assumes
`instanceof` and switch support from item 3.

**Branching and commits.** One branch per improvement, branched from the
integrated result of the previous one (e.g. `feat/class-cache`,
`feat/program-args`, `feat/core-opcodes`, `feat/primitive-arrays`,
`feat/exceptions`). Use Conventional Commits. Commit in small red‑green‑refactor
increments; a step below that says "test first" means write the failing test,
run it, watch it fail for the expected reason, then implement.

**Definition of done for every item.**

1. `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
   and `cargo test --all-targets --all-features` all pass.
2. New code has `//!` module docs and `///` item docs in the existing style.
3. README.md "Current Capabilities" and the "still unsupported" paragraph are
   updated in the same commit as the behavior change.
4. The acceptance program listed for the item runs under
   `cargo run -- -cp <dir> Main` and prints the expected output.

**Integration test conventions** (see `tests/jay_integration/support.rs`):
`temp_dir(name)` → `compile_java(&root, "Main.java", src)` →
`jay(&["-cp", root.to_str().unwrap(), "Main"])` → assert on
`output.status`, `stdout`, `stderr`. Always include stdout and stderr in the
assertion failure message, as the existing tests do. Add tests to the topic
module that fits; add a new module to `tests/jay_integration.rs` with a
`#[path]` attribute when none fits.

**javac constant folding.** javac folds constant expressions at compile time,
so `System.out.println(10 % 3)` never emits `irem`. Every test that targets a
specific opcode must obtain its operands from something the compiler cannot
fold: a non‑final static field, a method return value, or (after item 2)
`args.length`. Verify with `javap -c` when in doubt.

**GC discipline.** `Heap::allocate` never collects; only explicit
`collect_if_needed(frame)` calls do. The safe pattern in a shim is: pop
arguments → allocate and mutate → push the result onto the frame → call
`collect_if_needed(frame)` last. Never hold an `ObjectRef` in a Rust local
across a `collect_if_needed` call unless it is also reachable from a frame,
`saved_roots`, `static_fields`, or `class_mirrors`.

---

## Baseline findings

Probe programs compiled with `javac --release 21` and run on the current binary:

| Program | Outcome |
|---|---|
| `main(String[] args)` printing `args.length` | `local variable #0 is uninitialized` |
| `new int[3]` | `unsupported bytecode 0xbc` (`newarray`) |
| `new StringBuilder()` | `unsupported bytecode 0xbc` inside `AbstractStringBuilder.<init>` |
| `"hello".length()` | `unsupported array field descriptor [B` inside `String.length` |
| `try { throw new IllegalStateException("boom") } catch (RuntimeException e)` | `Throwable.fillInStackTrace(I)` is native; no exception tables parsed; no `athrow` |
| `s.length()` on a null `String` | VM error `null reference on stack`, not an NPE |
| `o instanceof String` | `unsupported bytecode 0xc1` |
| `switch (k)` | `unsupported bytecode 0xab` (`lookupswitch`) |
| `long y = x * 3 + 5` | `unsupported bytecode 0x69` (`lmul`) |
| `double d = 1.5` | `unsupported ldc2_w constant` (double is out of scope for this plan) |
| Lambda `Runnable r = () -> ...` | `LambdaMetafactory` bootstrap unsupported (out of scope) |
| Loop: 200 000 iterations × (static call + putfield + virtual call) | 2.0 s release, 7.2 s debug |

Root cause of the last row: `Interpreter::load_class_file` and the direct
`load_class_bytes` + `ClassFile::parse` calls in `invocation.rs` re‑read and
re‑parse the class file on every cross‑class call, field resolution, and
hierarchy walk. Each loop iteration parses `Main$P` at least three times.

---

## Improvement 1 — Cache parsed class files

**Goal.** Parse each class at most once per VM run. Expect the 200k‑iteration
probe to drop well under one second in release mode.

**Design.**

- Add to `Interpreter` (`src/vm/interpreter.rs`):
  `class_cache: RefCell<HashMap<String, Rc<ClassFile>>>`, keyed by internal
  name (`java/lang/Object`). `RefCell` keeps the existing `&self` signatures in
  `resolution.rs` working. The interpreter is single‑threaded and `!Send`
  already, so `Rc` is fine.
- Change `load_class_file(&self, internal_name) -> JayResult<Rc<ClassFile>>`
  to consult the cache first. Do not cache failures.
- Replace every direct `self.classes.load_class_bytes(..)` +
  `ClassFile::parse(..)` pair in `src/vm/invocation.rs` (`invoke_special`,
  `invoke_static`) with `load_class_file`. The `if target == caller.this_class`
  shortcut can stay, but it is no longer needed for performance; prefer to
  remove it so there is one code path.
- `find_instance_method_class`, `resolve_instance_method`,
  `resolve_interface_method`, `resolve_field_class`, `reference_matches_type`,
  `initialize_class` all move to `Rc<ClassFile>`. `resolve_field_class`
  currently moves `class_file.this_class` out; clone it instead.
- Once the class file is an `Rc` held in a local, `target_method.code.as_ref()`
  can be borrowed for the duration of `self.execute(...)` without the current
  `.clone()` of the whole `Code` (the borrow is of the local `Rc`, not of
  `self`). Remove the `code.clone()` calls in all invoke paths and in
  `native_runtime.rs::invoke_reference_to_string`.
- Consider (optional, measure first) caching `MethodDescriptor::parse` results
  keyed by descriptor string. Only do this if profiling after the class cache
  still shows descriptor parsing as significant.

**Steps.**

1. Test first (unit, `src/vm/resolution.rs` tests): construct an `Interpreter`
   as the existing `string_is_assignable_to_char_sequence_but_not_unrelated_types`
   test does, call `load_class_file("java/lang/Object")` twice, assert
   `Rc::ptr_eq` on the results. Fails to compile until the return type changes.
2. Implement the cache and the `Rc` return type; fix compile errors outward
   through `resolution.rs`, `invocation.rs`, `lifecycle.rs`, `native_runtime.rs`.
3. Remove the `Code` clones.
4. Run the full suite; re‑time the loop probe (`docs/probes` below) in release.

**Files.** `src/vm/interpreter.rs`, `src/vm/resolution.rs`,
`src/vm/invocation.rs`, `src/vm/lifecycle.rs`, `src/vm/native_runtime.rs`.

**README.** No capability change. Add one sentence under Development or
Architecture notes that classes are parsed once per run.

**Pitfalls.** `initialize_class` loads the class before checking anything
else; make sure it goes through the cache. Do not put the cache in
`ClassResolver`: `Vm` is `Clone` and the resolver is shared by reference, and a
`RefCell` there would make `Vm` `!Sync` for no benefit.

---

## Improvement 2 — Populate `args` and accept program arguments

**Goal.** `jay -cp <dir> Main a b c` runs `main` with `args = {"a","b","c"}`,
and `main(String[] args)` with no extra CLI arguments sees an empty array.

**Design.**

- `src/cli.rs`: `Config` gains `pub program_args: Vec<String>`. `parse_args`
  accepts any number of arguments after the main class and stores them
  verbatim. Delete the "unexpected extra arguments" error and its test.
- `src/vm.rs`: `run_main(&self, main_class, program_args: &[String])` and
  `run_main_to_writer(&self, main_class, program_args: &[String], output)`.
  Update the single caller in `src/main.rs`.
- In `run_main_to_writer`, after creating the `Interpreter` and before
  `execute`: if the selected `main` has descriptor `([Ljava/lang/String;)V`,
  allocate `heap.allocate_reference_array("[Ljava/lang/String;", n)`, then for
  each argument `allocate_string` and `store_array_reference`. Build the frame
  with `Frame::with_arguments(code.max_locals, vec![Value::Reference(array)])`.
  For `main()V` keep `Frame::new`. No GC can run during this setup because no
  `collect_if_needed` is called, so no extra rooting is needed; once in local 0
  the array is a frame root.
- The store‑compatibility check in `Heap::store_array_reference` already
  accepts `java/lang/String` into `[Ljava/lang/String;`.

**Steps.**

1. Test first (unit, `src/cli.rs`): `collects_program_arguments_after_main_class`
   asserting `program_args == ["a", "b c"]`; and `program_args` is empty when
   none are given. Remove `rejects_extra_arguments`.
2. Test first (integration, new module `tests/jay_integration/program_args.rs`
   registered in `tests/jay_integration.rs`):
   - `main_receives_empty_args_array`: prints `args.length` → `0`.
   - `main_receives_program_arguments`: `jay(&["-cp", dir, "Main", "alpha", "beta"])`,
     program prints `args.length` and `args[1]` → `2\nbeta\n`.
   - `main_without_parameters_ignores_program_arguments`: `main()` variant with
     extra CLI args still runs.
3. Implement CLI, then VM.

**Files.** `src/cli.rs`, `src/vm.rs`, `src/main.rs`, `tests/jay_integration.rs`,
new `tests/jay_integration/program_args.rs`.

**README.** Update the CLI shape to
`jay -cp <directory> <fully.qualified.MainClass> [args...]` and add a bullet
that `main` receives program arguments.

---

## Improvement 3 — Complete the core opcode set

**Goal.** Programs using ordinary int/long arithmetic, `switch`, `instanceof`,
and the remaining stack‑shuffle opcodes run without hitting
`unsupported bytecode`.

**Opcodes to add** (hex → mnemonic → semantics):

Integer arithmetic (`i32`, all wrapping):
`0x70 irem` (`wrapping_rem`; zero divisor → error, later `ArithmeticException`),
`0x74 ineg` (`wrapping_neg`), `0x78 ishl`, `0x7a ishr` (arithmetic),
`0x80 ior`. Shift counts are masked with `& 0x1f`.

Long arithmetic (`i64`, wrapping): `0x61 ladd`, `0x65 lsub`, `0x69 lmul`,
`0x6d ldiv`, `0x71 lrem`, `0x75 lneg`, `0x79 lshl`, `0x7b lshr`, `0x7d lushr`,
`0x7f land`, `0x81 lor`, `0x83 lxor`, `0x94 lcmp` (push -1/0/1). The shift
count for long shifts is an **int** popped first and masked with `& 0x3f`.

Conversions: `0x85 i2l`, `0x88 l2i` (truncate), `0x89 l2f`,
`0x91 i2b` (`as i8 as i32`), `0x92 i2c` (`as u16 as i32`),
`0x93 i2s` (`as i16 as i32`). `0x95 fcmpl` (NaN → -1; `0x96 fcmpg` already
exists with NaN → 1). `0xae freturn`.

Stack: `0x5b dup_x2`, `0x5c dup2`, `0x5d dup2_x1`, `0x5f swap`. `dup2` on a
`Long` duplicates the single `Value::Long` (this VM stores category‑2 values
as one stack entry; follow the existing `pop2` convention in
`runtime.rs::pop_two_words`). `dup_x2` must also handle the form where the
second value is a `Long`.

Control: `0xaa tableswitch`, `0xab lookupswitch`, `0xc8 goto_w`.
Both switches: after the opcode byte, skip padding so that the next read is at
`pc % 4 == 0` (relative to the start of the code array), then read `i32`
default offset. `tableswitch` then reads `low`, `high`, and `high - low + 1`
offsets; `lookupswitch` reads `npairs` then `npairs` `(match, offset)` pairs
(sorted ascending; a linear scan is fine). All offsets are relative to the
opcode's own pc. Generalize `branch_target` to take `i32`.

Type: `0xc1 instanceof` — pop reference; `Null` → push 0; otherwise push 1 if
the value's runtime type is assignable to the constant‑pool class, else 0.
Factor the array‑aware compatibility logic currently inlined in
`interpreter.rs::validate_reference_array_store` into one helper
`is_reference_compatible(actual, expected)` on `Interpreter` and use it from
`instanceof`, `checkcast`, and `aastore`. Arrays are assignable to
`java/lang/Object`, `java/lang/Cloneable`, `java/io/Serializable`, and to an
identical array descriptor; nothing else. Guard `reference_matches_type` so it
never tries to load a class named `[...`.

Output: `PrintStream.println(C)V` (print the UTF‑16 unit as a char),
`println(F)V` (match Java's `Float.toString` for the simple cases; document
that only shortest‑repr cases are supported).

**Design.**

- New module `src/vm/arithmetic.rs` with pure, unit‑tested functions:
  `int_binary(opcode, left, right) -> JayResult<i32>`,
  `long_binary(opcode, left, right) -> JayResult<i64>`,
  `long_shift(opcode, value, count: i32) -> i64`, `long_compare(left, right) -> i32`,
  `convert_int(opcode, value) -> i32`. The `execute_instruction` match then
  collapses the arithmetic arms into ranges that pop operands and delegate.
- `src/vm/bytecode.rs`: add `read_i4`, `align_pc_to_four(pc)`,
  `tableswitch_target(bytes, opcode_pc, pc, key) -> JayResult<usize>`,
  `lookupswitch_target(...)`, each unit‑tested on hand‑assembled byte slices.
- `src/vm/frame.rs`: add `dup_x2`, `dup2`, `dup2_x1`, `swap` next to the
  existing `duplicate_top*` methods, with unit tests including the `Long`
  cases.

**Steps (each test first).**

1. Unit tests for `arithmetic.rs`: wrapping on overflow, `i32::MIN / -1 ==
   i32::MIN`, `i32::MIN % -1 == 0`, division by zero returns `Err`, shift
   masking (`1 << 33 == 2` for int, `1L << 65 == 2` for long), `lcmp`
   ordering, `i2b/i2c/i2s` truncation.
2. Integration `arithmetic.rs`: `runs_integer_remainder_negation_and_shifts`,
   `runs_long_arithmetic_and_comparison`, `runs_int_long_conversions`, each
   reading operands from non‑final static fields.
3. Unit tests for switch decoding; integration new module
   `tests/jay_integration/control_flow.rs`: `runs_dense_switch` (tableswitch; use at least four consecutive cases, since
   javac emits `lookupswitch` for a two-case switch),
   `runs_sparse_switch` (lookupswitch, e.g. cases 1, 100, 1000), `runs_switch_default`,
   `runs_string_switch` is **not** in scope (needs `String.hashCode` +
   `equals`, which item 4 adds; add it there).
4. `instanceof` integration in `objects.rs`: true for exact class, superclass,
   interface; false for unrelated; false for null; true for `String[]` vs
   `Object`.
5. Stack ops usually appear only inside JDK code; cover with unit tests on
   `Frame` and rely on later items for integration coverage.

**Files.** `src/vm/interpreter.rs`, new `src/vm/arithmetic.rs`,
`src/vm/bytecode.rs`, `src/vm/frame.rs`, `src/vm/runtime.rs`,
`src/vm/invocation.rs` (println overloads), `src/vm.rs` (register module),
tests as above.

**README.** Replace the "Integer constants, local variables, addition, ..."
bullet and the "Limited long ..." bullet with an accurate description; add
bullets for `switch`, `instanceof`, and the char/float println overloads.
Remove "long arithmetic" from the unsupported list.

---

## Improvement 4 — Primitive arrays, then `String` and `StringBuilder` shims

**Goal.** `int[]`, `byte[]`, `char[]`, `long[]`, `float[]`, `short[]`,
`boolean[]` work end to end; common `String` instance methods and an explicit
`StringBuilder` work through shims.

### 4a. Primitive arrays

**Design.**

- `src/vm/heap.rs`: add
  ```rust
  enum PrimitiveArray { Boolean(Vec<i8>), Byte(Vec<i8>), Char(Vec<u16>),
                        Short(Vec<i16>), Int(Vec<i32>), Long(Vec<i64>), Float(Vec<f32>) }
  ObjectKind::PrimitiveArray(PrimitiveArray)
  ```
  plus `allocate_primitive_array(atype: u8, length)`, `load_primitive(reference,
  index) -> Value` (widen to `Int`/`Long`/`Float`), `store_primitive(reference,
  index, Value)` (narrow: byte `as i8`, char `as u16`, short `as i16`, boolean
  `& 1`). `array_length` handles both array kinds. `value_type` returns
  `Reference("[I")` etc.; `type_name` returns `int[]` etc. `mark` treats
  primitive arrays as leaves.
- `newarray` atype codes: 4 boolean, 5 char, 6 float, 7 double (return an
  explicit "unsupported" error), 8 byte, 9 short, 10 int, 11 long.
- Opcodes in `interpreter.rs`: `0xbc newarray`; loads `0x2e iaload`,
  `0x2f laload`, `0x30 faload`, `0x33 baload`, `0x34 caload`, `0x35 saload`;
  stores `0x4f iastore`, `0x50 lastore`, `0x51 fastore`, `0x54 bastore`,
  `0x55 castore`, `0x56 sastore`. `baload`/`bastore` serve both `byte[]` and
  `boolean[]`. Negative length → error (later `NegativeArraySizeException`).
  Bounds failures reuse the existing "array index N out of bounds for length M"
  message so item 5 can map it.
- `src/vm/descriptors.rs`: accept `[` followed by one of `Z B C S I J F` in
  `parse_field_descriptor` (→ `FieldType::Reference`) and `parse_value_type`
  (→ `ValueType::Reference("[I")`). Keep `[D` and `[[` rejected with the
  existing explicit errors. Update `is_supported_object_array_descriptor` and
  its callers accordingly.
- `heap.rs::reference_array_name` and `fields.rs::reference_array_descriptor`
  must produce `int[]` / `[I` for primitive components.

**Steps.**

1. **First**, fix the existing test
   `errors.rs::runtime_errors_include_interpreted_java_stack_trace`, which uses
   `new int[1]` to provoke `unsupported bytecode 0xbc`. Change the fixture to
   `int[][] grid = new int[2][2];` and the expected message to
   `unsupported bytecode 0xc5` (`multianewarray`). Do this as its own commit
   before primitive arrays land so the suite stays green throughout.
2. Unit tests in `heap.rs`: allocate each kind, round‑trip a value, narrowing
   on store (`store 300 into byte[] reads back 44`), bounds error, GC keeps a
   rooted primitive array and drops an unrooted one.
3. Unit tests in `descriptors.rs` for the accepted and rejected descriptors.
4. Integration new module `tests/jay_integration/arrays.rs`:
   `runs_int_array_store_load_and_length`, `runs_char_and_byte_arrays_narrow`,
   `runs_long_array`, `runs_boolean_array`, `array_fields_and_parameters`
   (an `int[]` instance field and an `int[]` method parameter),
   `rejects_double_array_with_explicit_error`.

### 4b. `String` instance shims

Strings are native Rust `String` heap objects, so interpreting
`java.lang.String` bytecode (which reads the `byte[] value` field) can never
work. Add shims instead.

**Design.**

- New module `src/vm/string_shims.rs` (register in `src/vm.rs`) with
  `pub(super) fn try_invoke_string_method(&mut self, frame, name, descriptor,
  receiver: ObjectRef, arguments: &[Value]) -> JayResult<bool>`. Call it from
  `invoke_virtual` immediately after `receiver_class_name` is known, in the
  same place the `Date.toString` and `String.hashCode` special cases live;
  move `hashCode` into the new module.
- Methods (all on UTF‑16 units via `encode_utf16`, matching Java semantics):
  `length()I`, `charAt(I)C`, `isEmpty()Z`, `equals(Ljava/lang/Object;)Z`
  (false for non‑String argument), `equalsIgnoreCase(Ljava/lang/String;)Z`,
  `compareTo(Ljava/lang/String;)I`, `hashCode()I`, `toString()Ljava/lang/String;`
  (returns receiver), `substring(I)`, `substring(II)`, `indexOf(Ljava/lang/String;)I`,
  `indexOf(I)I`, `contains(Ljava/lang/CharSequence;)Z`, `startsWith`, `endsWith`,
  `trim()`, `toUpperCase()`, `toLowerCase()`, `concat(Ljava/lang/String;)`,
  `toCharArray()[C`. Index errors return a `JayError` now; item 5 maps them to
  `StringIndexOutOfBoundsException`.
- Static shims in `invoke_static`: `String.valueOf(I)`, `(J)`, `(C)`, `(Z)`;
  `Integer.parseInt(Ljava/lang/String;)I` (return an error on malformed input
  now; `NumberFormatException` after item 5); `Integer.toString(I)`.
- `println(Object)` and `string_concat_argument` already handle strings.

**Steps.** Test first in `tests/jay_integration/strings.rs` (new module):
one test per small group of methods, each printing several results on separate
lines. Include `runs_string_switch` here (javac emits `hashCode` +
`lookupswitch` + `equals`).

### 4c. `StringBuilder` shim

Interpreting `AbstractStringBuilder` needs `System.arraycopy`, `Unsafe`, and
`StringLatin1`/`StringUTF16` natives; do not go down that path.

**Design.**

- `ObjectKind::StringBuilder(String)` in `heap.rs`, with
  `convert_instance_to_string_builder(reference, initial: String)` (the `new`
  opcode has already allocated an `Instance` of `java/lang/StringBuilder`; the
  `<init>` shim swaps its kind in place), `string_builder(&self, r) -> &str`,
  `string_builder_mut(&mut self, r) -> &mut String`. `value_type` reports
  `java/lang/StringBuilder`.
- `invoke_special` shim for `java/lang/StringBuilder.<init>` with descriptors
  `()V`, `(I)V` (ignore capacity), `(Ljava/lang/String;)V`.
- `invoke_virtual` shims when the receiver is a `StringBuilder`:
  `append` with `(Ljava/lang/String;)`, `(I)`, `(J)`, `(C)`, `(Z)`,
  `(Ljava/lang/Object;)` (use the existing `string_concat_argument` /
  `String.valueOf(Object)` logic, which may call interpreted `toString`), all
  returning the receiver; `toString()`, `length()I`, `charAt(I)C`,
  `reverse()`, `setLength(I)V`. Push the receiver back before any
  `collect_if_needed`.
- Extend `println_object_text` and `string_concat_argument` to accept a
  `StringBuilder` receiver.

**Steps.** Test first in `strings.rs`: chained append of mixed types,
`toString`, `length`, `reverse`, and `StringBuilder` passed to another method.

**Files.** `src/vm/heap.rs`, `src/vm/descriptors.rs`, `src/vm/interpreter.rs`,
`src/vm/fields.rs`, `src/vm/invocation.rs`, new `src/vm/string_shims.rs`,
`src/vm/runtime.rs`, `src/vm.rs`, `tests/jay_integration/{errors,arrays,strings}.rs`,
`tests/jay_integration.rs`.

**README.** Add bullets for primitive arrays (listing the seven element
types and that `double[]` and multi‑dimensional arrays are still rejected),
the `String` shim list, and the `StringBuilder` shim list. Remove "Primitive
arrays" from the unsupported paragraph.

---

## Improvement 5 — Java exceptions

**Goal.** `throw`, `try`/`catch`/`finally`, propagation across interpreted
frames, VM‑raised `NullPointerException` / `ArrayIndexOutOfBoundsException` /
`ArithmeticException` / `ClassCastException` / `NegativeArraySizeException`,
and Java‑style output for uncaught exceptions.

### 5a. Parse exception tables

`src/classfile.rs::parse_code` currently skips the table
(`self.skip(exception_table_length * 8)`). Replace with:

```rust
pub struct ExceptionHandler { pub start_pc: u16, pub end_pc: u16,
                              pub handler_pc: u16, pub catch_type: Option<String> }
// Code gains: pub exception_table: Vec<ExceptionHandler>
```

`catch_type` index 0 means "any" (used for `finally`); otherwise resolve the
`Class` constant to its internal name. Unit test in `classfile.rs` by parsing a
class compiled from a `try/catch` (the integration harness compiles Java; for
a unit test, hand‑assemble or use the smallest `ClassEditor`‑style fixture
already used in the module's tests).

### 5b. Error channel for thrown objects

Keep `JayResult` everywhere; extend `JayError` in `src/lib.rs` with two
private fields and crate‑visible accessors:

```rust
/// Set when the error is a Java exception object in flight (heap slot index).
thrown_object: Option<usize>,
/// Set when a VM fault should be materialized as a Java exception of this class.
fault_class: Option<&'static str>,
```

- `JayError::thrown(slot: usize, display: String)` — `display` is
  `"java.lang.IllegalStateException: boom"` computed at throw time from the
  class name and the `detailMessage` field (omit `: msg` when null).
- `JayError::fault(class: &'static str, message)` — used at existing fault
  sites: `Frame::pop_object_ref` null → `"java/lang/NullPointerException"`;
  `runtime.rs::checked_array_index` and heap bounds errors →
  `"java/lang/ArrayIndexOutOfBoundsException"` (message
  `Index 5 out of bounds for length 3`, matching HotSpot); integer `idiv`/`irem`/`ldiv`/`lrem`
  by zero → `"java/lang/ArithmeticException"` with message `/ by zero`;
  `check_cast` failure → `"java/lang/ClassCastException"`; negative array
  size → `"java/lang/NegativeArraySizeException"`; string index errors →
  `"java/lang/StringIndexOutOfBoundsException"`; `Integer.parseInt` →
  `"java/lang/NumberFormatException"`.
- `pub fn is_java_exception(&self) -> bool`, `pub(crate) fn thrown_object()`,
  `pub(crate) fn fault_class()`. `Display` is unchanged (prints the message).
- `heap.rs`: add `ObjectRef::index(self) -> usize` and
  `ObjectRef::from_index(usize)` (both `pub(super)`) so the slot can cross the
  `lib.rs` boundary without exposing `ObjectRef`.

### 5c. Throwing and catching in `execute`

- `0xbf athrow`: pop reference. `Null` → fault NPE. Otherwise return
  `Err(JayError::thrown(ref.index(), display))`.
- New module `src/vm/exceptions.rs` with:
  - `fn materialize_fault(&mut self, error: &JayError) -> JayResult<ObjectRef>`:
    allocate an `Instance` of `fault_class`, allocate a `String` for the
    message, `put_instance_field(.., FieldKey::new("java/lang/Throwable",
    "detailMessage", "Ljava/lang/String;"), ..)`. Do **not** run the JDK
    constructor. Mark the class initialized so a later `getstatic` does not
    re‑run `<clinit>` inconsistently (or simply leave it; verify with a test
    that `e.getMessage()` works on a VM‑raised NPE).
  - `fn find_handler(&self, code: &Code, pc: usize, exception_class: &str)
    -> JayResult<Option<usize>>`: first entry in table order with
    `start_pc <= pc < end_pc` and (`catch_type == None` or
    `is_assignable_reference(exception_class, catch_type)`). Pure enough to
    unit test with a stub table.
- In `Interpreter::execute`, replace the `map_err(.. with_java_stack_frame ..)`
  on the instruction result with:

  ```text
  match self.execute_instruction(...) {
      Ok(Continue) => {}
      Ok(Return(v)) => return Ok(v),
      Err(error) if error.is_java_exception() => {
          let thrown = match error.thrown_object() {
              Some(slot) => ObjectRef::from_index(slot),
              None => self.materialize_fault(&error)?,   // fault → object
          };
          let class = self.heap.instance_class_name(thrown)?.to_string();
          match self.find_handler(code, opcode_pc, &class)? {
              Some(handler_pc) => { frame.stack.clear();
                                    frame.stack.push(Value::Reference(thrown));
                                    pc = handler_pc; }
              None => return Err(JayError::thrown(thrown.index(), display)
                                     .with_java_stack_frame(frame_at(opcode_pc))),
          }
      }
      Err(error) => return Err(error.with_java_stack_frame(frame_at(opcode_pc))),
  }
  ```

  Errors from nested calls arrive here through the invoke instruction, whose
  `opcode_pc` lies inside the caller's `try` range, so propagation across
  frames needs no changes in `invocation.rs`.
- GC safety while unwinding: no allocation happens between the throw and the
  handler push, and `materialize_fault` performs its two allocations without
  calling `collect_if_needed`, so the in‑flight object is never exposed to a
  collection. Add a comment saying so in `exceptions.rs`, and a unit test in
  `heap.rs`/`lifecycle.rs` is not required.
- `finally`: javac emits an any‑handler that stores, runs the block, reloads,
  and `athrow`s. Nothing special is needed.

### 5d. JDK natives on the throw path

The probe reached `Throwable.fillInStackTrace()` → native
`fillInStackTrace(I)Ljava/lang/Throwable;`. Add a shim in `invoke_virtual`
(next to `Class.desiredAssertionStatus`): pop the int and receiver, push the
receiver. Verified with `javap` on JDK 27: everything before it in `Throwable.<init>`
already interprets, including `Throwable.<clinit>` (the `UNASSIGNED_STACK` and
`SUPPRESSED_SENTINEL` reads at pc 10 and 17 succeed). After it, the
constructor stores `detailMessage` and reads the static `jfrTracing` flag,
which defaults to 0 and skips the JFR tracer call. Discover any further natives test‑first;
each gets a one‑line shim and a README mention.

`e.getMessage()` is a plain field read of `detailMessage` and needs no shim.
`e.printStackTrace()` is out of scope; document it as unsupported.

### 5e. Uncaught exceptions at the top level

`src/main.rs`: when `error.is_java_exception()`, print
`Exception in thread "main" {error}` instead of `jay: {error}`, then the
existing `  at Class.method(desc) (pc N)` lines, and exit with failure. For a
fault that was never materialized (thrown from `main` itself with no handler,
the loop above always materializes before propagating, so this case does not
occur; assert it in a test anyway).

### Tests

Integration, in `tests/jay_integration/errors.rs` (rename nothing; add):

- `catches_thrown_exception_by_supertype`: `throw new IllegalStateException("boom")`
  caught as `RuntimeException`, prints `caught boom` via `getMessage()`.
- `runs_finally_block_on_normal_and_exceptional_paths`.
- `propagates_exception_across_method_calls`: thrown three frames deep,
  caught in `main`.
- `catch_clause_type_is_respected`: `catch (IllegalArgumentException)` does
  not catch `IllegalStateException`; outer handler does.
- `catches_null_pointer_exception_from_vm_fault`,
  `catches_arithmetic_exception_from_division_by_zero` (operands from a
  static field), `catches_array_index_out_of_bounds`,
  `catches_class_cast_exception`.
- `uncaught_exception_prints_java_style_header_and_frames`: stderr starts with
  `Exception in thread "main" java.lang.IllegalStateException: boom` and
  contains `  at Main.main([Ljava/lang/String;)V (pc `; exit status non‑zero;
  stdout is empty.
- `uncaught_exception_without_message_omits_colon`.

Unit: `find_handler` selection order and range semantics in `exceptions.rs`;
`ExceptionHandler` parsing in `classfile.rs`; `JayError` accessors in
`lib.rs`.

**Files.** `src/classfile.rs`, `src/lib.rs`, `src/main.rs`,
`src/vm/interpreter.rs`, new `src/vm/exceptions.rs`, `src/vm/frame.rs`,
`src/vm/runtime.rs`, `src/vm/heap.rs`, `src/vm/invocation.rs`, `src/vm.rs`,
`tests/jay_integration/errors.rs`.

**README.** New bullet group for exceptions: explicit `throw`, `try/catch/finally`,
the list of VM‑raised exception classes, `getMessage()`, and the uncaught
output format. State that `printStackTrace`, stack‑trace elements, suppressed
exceptions, and line numbers are unsupported.

---

## Out of scope (deliberately)

Lambdas / `LambdaMetafactory`, `double` values, string interning semantics for
`==`, `String.format`, `printStackTrace`, multi‑dimensional and `double[]`
arrays, `synchronized` (`monitorenter`/`monitorexit`), `wide`. Each should get
its own plan after these five land.

---

## Appendix A — Acceptance programs

Each must compile with `javac --release 21` and run under `jay` after the
named item. Keep them in a scratch directory; they are not part of the test
suite (the integration tests cover the same ground with assertions).

**After item 1** — timing only:

```java
public class Main {
    static int f(int x) { return x + 1; }
    static class P { int v; int g() { return v + 1; } }
    public static void main(String[] a) {
        int s = 0; P p = new P();
        for (int i = 0; i < 200000; i++) { s = f(s); p.v = s; s = p.g(); }
        System.out.println(s);   // 400000
    }
}
```

**After item 2** — `jay -cp . Main x y`:

```java
public class Main { public static void main(String[] a) {
    System.out.println(a.length); System.out.println(a[1]); } }   // 2, y
```

**After item 3** — operands via non‑final statics to defeat folding:

```java
public class Main {
    static int ten = 10, three = 3, k = 100; static long big = 1L << 40;
    public static void main(String[] a) {
        System.out.println(ten % three);            // 1
        System.out.println(-ten >> 1);              // -5
        System.out.println(big * three + 5);        // 3298534883333
        System.out.println(big > ten);              // true
        switch (k) { case 1: System.out.println("one"); break;
                     case 100: System.out.println("hundred"); break;
                     default: System.out.println("other"); }
        Object o = "x"; System.out.println(o instanceof String);   // true
        System.out.println((char) (ten + 55));      // A
    }
}
```

**After item 4**:

```java
public class Main { public static void main(String[] a) {
    int[] xs = new int[3]; xs[1] = 7; System.out.println(xs[1] + xs.length);   // 10
    byte[] bs = new byte[1]; bs[0] = (byte) 300; System.out.println(bs[0]);   // 44
    String s = "hello";
    System.out.println(s.length() + " " + s.charAt(1) + " " + s.substring(1, 3) + " " + s.equals("hello"));  // 5 e el true
    StringBuilder b = new StringBuilder(); b.append("a").append(1).append('c');
    System.out.println(b.toString());   // a1c
} }
```

**After item 5**:

```java
public class Main {
    static int zero = 0; static String none;
    public static void main(String[] a) {
        try { throw new IllegalStateException("boom"); }
        catch (RuntimeException e) { System.out.println("caught " + e.getMessage()); }
        try { System.out.println(none.length()); }
        catch (NullPointerException e) { System.out.println("npe"); }
        try { System.out.println(1 / zero); }
        catch (ArithmeticException e) { System.out.println(e.getMessage()); }   // / by zero
        finally { System.out.println("finally"); }
        throw new IllegalArgumentException("bye");
        // stderr: Exception in thread "main" java.lang.IllegalArgumentException: bye
        //         at Main.main([Ljava/lang/String;)V (pc N)
    }
}
```

## Appendix B — Opcode quick reference for this plan

| Hex | Mnemonic | Item | Hex | Mnemonic | Item |
|---|---|---|---|---|---|
| 0x2e–0x30 | iaload laload faload | 4 | 0x85 | i2l | 3 |
| 0x33–0x35 | baload caload saload | 4 | 0x88 0x89 | l2i l2f | 3 |
| 0x4f–0x51 | iastore lastore fastore | 4 | 0x91–0x93 | i2b i2c i2s | 3 |
| 0x54–0x56 | bastore castore sastore | 4 | 0x94 | lcmp | 3 |
| 0x5b 0x5c 0x5d 0x5f | dup_x2 dup2 dup2_x1 swap | 3 | 0x95 | fcmpl | 3 |
| 0x61 0x65 0x69 0x6d | ladd lsub lmul ldiv | 3 | 0xaa 0xab | tableswitch lookupswitch | 3 |
| 0x70 0x71 | irem lrem | 3 | 0xae | freturn | 3 |
| 0x74 0x75 | ineg lneg | 3 | 0xbc | newarray | 4 |
| 0x78–0x7d | ishl lshl ishr lshr iushr* lushr | 3 | 0xbf | athrow | 5 |
| 0x7f–0x83 | land ior lor ixor* lxor | 3 | 0xc1 | instanceof | 3 |
| | | | 0xc8 | goto_w | 3 |

`*` already implemented.
