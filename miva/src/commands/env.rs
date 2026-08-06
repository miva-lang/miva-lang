use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub fn compute_sha256(path: &str) -> String {
    let content = std::fs::read(path).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();
    hex::encode(result)
}

pub fn hash_file_path(cache_dir: &PathBuf, cache_key: &str) -> PathBuf {
    cache_dir.join(format!("{}.sha256", cache_key))
}

pub fn get_std_include_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MIVA_STD") {
        PathBuf::from(dir)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(format!("{}/.miver/lib/", home))
    } else {
        PathBuf::from("/.miver/lib/")
    }
}

pub fn get_build_dir() -> PathBuf {
    std::env::var("MIVA_BUILD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("build/debug"))
}

pub fn get_cache_dir() -> PathBuf {
    std::env::var("MIVA_BUILD_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("build/debug/cache"))
}

pub fn get_build_dir_rel(release: bool) -> PathBuf {
    if release {
        std::env::var("MIVA_RELEASE_BUILD")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("build/release"))
    } else {
        get_build_dir()
    }
}

pub fn get_cache_dir_rel(release: bool) -> PathBuf {
    if release {
        std::env::var("MIVA_RELEASE_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("build/release/cache"))
    } else {
        get_cache_dir()
    }
}

pub fn get_include_flags() -> String {
    std::env::var("MIVA_INC_FLAGS").unwrap_or_default()
}

pub fn get_link_flags() -> String {
    std::env::var("MIVA_LINK_FLAGS").unwrap_or_default()
}

pub fn get_keep_cpp() -> bool {
    std::env::var("MIVA_KEEP_CPP").is_ok()
}

/// Seconds before an external tool (g++, llc, cc, mvm, the compiled binary) is
/// killed. Defaults to 300; override with `MIVA_TIMEOUT_SECS`. Guards against
/// hangs like g++ parsing a stray build artifact as a header.
pub fn get_timeout_secs() -> u64 {
    std::env::var("MIVA_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(300)
}

/// Run an external command, killing it if it exceeds the timeout. `capture`
/// selects whether stdout/stderr are collected (false inherits the terminal).
/// Returns the process output with the exit status set.
pub fn run_with_timeout(
    cmd: &mut std::process::Command,
    what: &str,
    capture: bool,
) -> anyhow::Result<std::process::Output> {
    let timeout = std::time::Duration::from_secs(get_timeout_secs());
    if capture {
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to invoke {}: {}", what, e))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!(
                    "{} timed out after {}s (set MIVA_TIMEOUT_SECS to adjust)",
                    what,
                    get_timeout_secs()
                );
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| anyhow::anyhow!("failed to collect output from {}: {}", what, e))?;
    Ok(output)
}
