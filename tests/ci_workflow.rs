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
        "actions/checkout@v4",
        "actions/setup-java@v4",
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
