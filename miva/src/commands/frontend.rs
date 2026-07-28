use std::env;
use std::path::PathBuf;

fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

/// Locate the `mvm` virtual machine binary.
///
/// Lookup order: `$MIVA_MVM` env var → `PATH` → alongside the miva
/// executable (workspace target dir or a miver-installed bundle).
pub fn find_mvm() -> Option<PathBuf> {
    if let Ok(p) = env::var("MIVA_MVM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }

    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let c = dir.join("mvm");
            if c.is_file() {
                return Some(c);
            }
        }
    }

    let base = exe_dir()?;
    let platform_names = ["mvm", "mvm-linux", "mvm-macos", "mvm-windows.exe"];
    for name in &platform_names {
        let c = base.join(name);
        if c.exists() {
            return Some(c);
        }
    }

    None
}

/// Parse a Miva source file in-process via the miva-frontend library.
pub fn run_frontend(input: &str) -> anyhow::Result<crate::ast::AstFile> {
    let source = std::fs::read_to_string(input)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {}", input, e))?;
    let defs = miva_frontend::parse(&source, input).map_err(|e| {
        let err = crate::error::Error {
            code: "E0000".to_string(),
            message: e.message,
            loc: crate::ast::Loc {
                line: e.line,
                col: e.col,
            },
        };
        anyhow::anyhow!(
            "{}",
            crate::error::format_error_with_source(&err, input, &source)
        )
    })?;
    Ok(crate::ast::AstFile {
        defs,
        files: vec![input.to_string()],
    })
}
