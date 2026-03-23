use std::path::{Path, PathBuf};
use std::process::Command;

fn ui_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ui")
}

fn run_cargo_check(case_dir: &Path) -> std::process::Output {
    Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(case_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", case_dir.join("target"))
        .output()
        .expect("failed to execute cargo check for UI fixture")
}

#[test]
fn auto_node_pass_cases_compile() {
    let case_dir = ui_root().join("pass/auto_node_success");
    let output = run_cargo_check(&case_dir);
    if !output.status.success() {
        panic!(
            "expected pass fixture to compile\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn auto_node_fail_cases_emit_expected_diagnostics() {
    let cases = [
        (
            ui_root().join("fail/tuple_struct"),
            "`auto_node` macro can only be annotated on named struct or unit struct.",
        ),
        (
            ui_root().join("fail/reserved_field"),
            "field `id` is reserved by `auto_node`",
        ),
    ];

    for (case_dir, expected) in cases {
        let output = run_cargo_check(&case_dir);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "expected failure fixture to fail: {}\nstdout:\n{}\nstderr:\n{}",
            case_dir.display(),
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
        assert!(
            stderr.contains(expected),
            "expected stderr to contain `{expected}` for {}\nstderr:\n{}",
            case_dir.display(),
            stderr
        );
    }
}
