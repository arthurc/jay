use std::fs;
use std::path::Path;

#[test]
fn github_ci_runs_project_checks() {
    let workflow_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml");
    let workflow = fs::read_to_string(&workflow_path)
        .expect("expected a GitHub Actions CI workflow at .github/workflows/ci.yml");

    for required in [
        "name: CI",
        "runs-on: ubuntu-latest",
        "actions/checkout@v6",
        "actions/setup-java@v",
        "java-version: '21'",
        "rustup toolchain install stable --profile minimal --component rustfmt,clippy",
        "cargo fmt --all --check",
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo test --all-targets --all-features",
    ] {
        assert!(
            workflow.contains(required),
            "CI workflow should contain `{required}`"
        );
    }
}

#[test]
fn dependabot_updates_cargo_dependencies_and_github_actions() {
    let dependabot_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/dependabot.yml");
    let dependabot = fs::read_to_string(&dependabot_path)
        .expect("expected a Dependabot config at .github/dependabot.yml");

    for required in [
        "version: 2",
        "package-ecosystem: \"cargo\"",
        "package-ecosystem: \"github-actions\"",
        "directory: \"/\"",
        "interval: \"weekly\"",
    ] {
        assert!(
            dependabot.contains(required),
            "Dependabot config should contain `{required}`"
        );
    }
}

#[test]
fn vm_facade_stays_small_and_interpreter_implementation_is_split() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let facade_path = manifest_dir.join("src/vm.rs");
    let facade = fs::read_to_string(&facade_path).expect("expected src/vm.rs to exist");
    let facade_line_count = facade.lines().count();

    assert!(
        facade_line_count <= 120,
        "src/vm.rs should stay a small facade, found {facade_line_count} lines"
    );

    for module in [
        "src/vm/interpreter.rs",
        "src/vm/fields.rs",
        "src/vm/invocation.rs",
        "src/vm/lifecycle.rs",
        "src/vm/native_runtime.rs",
        "src/vm/resolution.rs",
        "src/vm/runtime.rs",
    ] {
        assert!(
            manifest_dir.join(module).exists(),
            "VM implementation should include {module}"
        );
    }
}
