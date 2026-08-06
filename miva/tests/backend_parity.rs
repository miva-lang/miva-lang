//! End-to-end parity tests: build and run example projects with all three
//! backends (cxx, llvm, mvm) and assert their program output is identical.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("miva crate lives inside the workspace root")
        .to_path_buf()
}

fn miva_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_miva"))
}

/// Locate (or build) the mvm interpreter next to the miva binary.
fn mvm_bin() -> PathBuf {
    let candidate = miva_bin().with_file_name("mvm");
    if !candidate.exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "miva-vm", "--bin", "mvm"])
            .current_dir(repo_root())
            .status()
            .expect("failed to spawn cargo build for miva-vm");
        assert!(status.success(), "cargo build -p miva-vm failed");
    }
    assert!(
        candidate.exists(),
        "mvm binary not found at {}",
        candidate.display()
    );
    candidate
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&from, &to);
        } else {
            fs::copy(&from, &to).unwrap();
        }
    }
}

struct RunOutput {
    stdout: String,
    exit_code: i32,
}

fn run_backend(project: &Path, backend: &str, root: &Path, mvm: &Path) -> RunOutput {
    let output = Command::new(miva_bin())
        .args(["run", "-b", backend])
        .current_dir(project)
        .env("MIVA_STD", root.join("stdlib"))
        .env("MIVA_MVM", mvm)
        .output()
        .expect("failed to spawn miva run");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        panic!(
            "miva run -b {} failed in {} (exit {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
            backend,
            project.display(),
            output.status.code(),
            stdout,
            String::from_utf8_lossy(&output.stderr),
        );
    }
    RunOutput {
        stdout,
        exit_code: output.status.code().unwrap_or(-1),
    }
}

fn assert_backend_parity(example: &str) {
    let root = repo_root();
    let mvm = mvm_bin();
    let src = root.join("examples").join(example);
    assert!(src.exists(), "example not found: {}", src.display());

    let tmp = std::env::temp_dir().join(format!("miva-parity-{}-{}", example, std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::copy(src.join("miva.toml"), tmp.join("miva.toml")).unwrap();
    copy_dir(&src.join("src"), &tmp.join("src"));

    let backends = ["cxx", "llvm", "mvm"];
    let mut results = Vec::new();
    for backend in backends {
        // Fresh build per backend so stale artifacts never leak across runs.
        let _ = fs::remove_dir_all(tmp.join("build"));
        results.push(run_backend(&tmp, backend, &root, &mvm));
    }

    let baseline = &results[0];
    for (backend, result) in backends.iter().zip(&results).skip(1) {
        assert_eq!(
            baseline.stdout, result.stdout,
            "stdout mismatch between cxx and {} for example '{}'",
            backend, example
        );
        assert_eq!(
            baseline.exit_code, result.exit_code,
            "exit code mismatch between cxx and {} for example '{}'",
            backend, example
        );
    }

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn parity_enum() {
    assert_backend_parity("enum");
}

#[test]
fn parity_guard() {
    assert_backend_parity("guard");
}

#[test]
fn parity_generic_enum() {
    assert_backend_parity("generic-enum");
}

#[test]
fn parity_drop_system() {
    assert_backend_parity("drop-system");
}

#[test]
fn parity_mutex_guard() {
    assert_backend_parity("mutex-guard");
}

#[test]
fn parity_tuple() {
    assert_backend_parity("tuple");
}
