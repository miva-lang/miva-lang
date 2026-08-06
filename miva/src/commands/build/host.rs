use super::*;

/// Locate a C compiler for compiling the project's libhost.so. Prefers `cc`,
/// falls back to `gcc`, then `clang`.
pub(crate) fn which_cc() -> anyhow::Result<std::path::PathBuf> {
    for candidate in ["cc", "gcc", "clang"] {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(std::path::PathBuf::from(candidate));
        }
    }
    Err(anyhow::anyhow!(
        "MVM user unsafe functions require a C toolchain (cc/gcc/clang) but none was found in PATH"
    ))
}

/// How the generated libhost C shims are packaged: a shared library loaded by
/// the MVM interpreter, or a static object linked into the LLVM executable.
#[derive(PartialEq)]
pub(crate) enum HostKind {
    SharedLib,
    Object,
}

/// Write the libhost C source (raw C shims for user `unsafe fn` definitions)
/// into `build_dir` and compile it. Returns the compiled artifact path.
pub(crate) fn compile_libhost(
    build_dir: &Path,
    host_defs: &[crate::codegen::mvm::HostDef],
    kind: HostKind,
) -> Result<PathBuf> {
    let libhost_c = build_dir.join("libhost.c");
    let libhost_h = build_dir.join("mvp_host.h");
    let header = match kind {
        HostKind::SharedLib => miva_vm::host::host_header(),
        HostKind::Object => miva_vm::host::host_header_llvm(),
    };
    std::fs::write(&libhost_h, &header)?;
    let mut c_src = String::new();
    c_src.push_str("#include <mvp_host.h>\n\n");
    for hd in host_defs {
        c_src.push_str(&format!(
            "MivaValue miva_host_{}(const MivaValue* args, int argc) {{\n",
            hd.name
        ));
        c_src.push_str(&hd.code);
        c_src.push_str("\n}\n\n");
    }
    std::fs::write(&libhost_c, &c_src)?;

    let cc = which_cc()?;
    let (out_path, mode_args, artifact): (PathBuf, &[&str], &str) = match kind {
        HostKind::SharedLib => (
            build_dir.join("libhost.so"),
            &["-shared", "-fPIC"],
            "libhost.so",
        ),
        HostKind::Object => (build_dir.join("libhost.o"), &["-c"], "libhost.o"),
    };
    let mut cmd = std::process::Command::new(&cc);
    cmd.args(mode_args)
        .arg("-O2")
        .arg("-I")
        .arg(build_dir)
        .arg(&libhost_c)
        .arg("-o")
        .arg(&out_path);
    let output =
        super::env::run_with_timeout(&mut cmd, &format!("C compiler ({})", artifact), true)
            .map_err(|e| anyhow::anyhow!("failed to invoke C compiler for {}: {}", artifact, e))?;
    let status = output.status;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "failed to compile {} from user unsafe functions",
            artifact
        ));
    }
    eprintln!(
        "{}",
        color::success(&format!("{} -> {}", artifact, out_path.display()))
    );
    Ok(out_path)
}
