use super::*;

pub(crate) fn dep_cache_path(cache_dir: &Path, file: &str, std_path: &str) -> PathBuf {
    let cache_key = make_cache_key(file, std_path);
    cache_dir.join(format!("{}.d", cache_key))
}

pub(crate) fn read_dep_cache(path: &Path, source: &str) -> Option<Vec<String>> {
    let dep_meta = std::fs::metadata(path).ok()?;
    if let Ok(src_meta) = std::fs::metadata(source) {
        if let (Ok(src_mtime), Ok(dep_mtime)) = (src_meta.modified(), dep_meta.modified()) {
            if src_mtime > dep_mtime {
                return None;
            }
        }
    }
    let content = std::fs::read_to_string(path).ok()?;
    Some(content.lines().map(|l| l.to_string()).collect())
}

pub(crate) fn write_dep_cache(path: &Path, deps: &[String]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = deps.join("\n");
    let _ = std::fs::write(path, content);
}

/// In-process AST cache: each source file is parsed at most once per build,
/// shared by import scanning, macro collection, signature collection and
/// per-file compilation.
pub(crate) type AstCache = HashMap<String, crate::ast::AstFile>;

pub(crate) fn parse_cached<'a>(
    cache: &'a mut AstCache,
    file: &str,
) -> anyhow::Result<&'a crate::ast::AstFile> {
    if !cache.contains_key(file) {
        let ast = frontend::run_frontend(file)?;
        cache.insert(file.to_string(), ast);
    }
    Ok(&cache[file])
}

pub(crate) fn make_cache_key(file: &str, std_path: &str) -> String {
    if let Some(rest) = file.strip_prefix(std_path) {
        rest.trim_start_matches('/').to_string()
    } else if let Some(rest) = file.strip_prefix('/') {
        rest.to_string()
    } else {
        file.to_string()
    }
}

pub(crate) fn needs_rebuild_by_hash(file: &str, cache_dir: &Path, cache_key: &str, backend: Backend) -> bool {
    let hash_path = env::hash_file_path(&cache_dir.to_path_buf(), cache_key);
    let current_hash = env::compute_sha256(file);

    if let Ok(stored) = std::fs::read_to_string(&hash_path) {
        let lines: Vec<&str> = stored.trim().lines().collect();
        if lines.len() >= 2 {
            // Format: line 1 = hash, line 2 = backend name
            lines[0].trim() != current_hash || lines[1].trim() != backend.name()
        } else {
            // Legacy format: just the hash
            lines[0].trim() != current_hash
        }
    } else {
        true
    }
}

pub(crate) fn update_hash_cache(file: &str, cache_dir: &Path, cache_key: &str, backend: Backend) {
    let hash_path = env::hash_file_path(&cache_dir.to_path_buf(), cache_key);
    let current_hash = env::compute_sha256(file);
    if let Some(parent) = hash_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&hash_path, format!("{}\n{}", current_hash, backend.name()));
}
