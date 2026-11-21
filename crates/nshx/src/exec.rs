//! Running the selected CLI, and the installed distribution for a project
//! without a pin.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use anyhow::Context as _;
use anyhow::bail;

/// The executable's file name on this platform (`nsh.exe` on Windows).
pub(crate) fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// Replaces this process with `bin` on Unix, or runs it and exits with its
/// status on Windows.
pub(crate) fn exec(bin: &Path, args: &[OsString]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        let error = std::process::Command::new(bin).args(args).exec();
        Err(anyhow::Error::new(error).context(format!("exec {}", bin.display())))
    }
    #[cfg(windows)]
    {
        let status = std::process::Command::new(bin)
            .args(args)
            .status()
            .with_context(|| format!("run {}", bin.display()))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// Runs the installed distribution when the project does not pin nsh.
pub(crate) fn fallback(args: &[OsString]) -> anyhow::Result<()> {
    let Some(bin) = fallback_bin() else {
        bail!(
            "no nsh.json with a toolchain.nsh pin here, and no installed nsh distribution; \
             try nshx help"
        );
    };
    exec(&bin, args)
}

/// The distribution `NSH_FALLBACK` names, else the installer's `~/.nsh`
/// (`%LOCALAPPDATA%\nsh`), unless that is nshx itself.
fn fallback_bin() -> Option<PathBuf> {
    let current = std::env::current_exe()
        .ok()
        .and_then(|path| fs::canonicalize(path).ok());
    let candidate = std::env::var_os("NSH_FALLBACK")
        .map(PathBuf::from)
        .or_else(|| {
            if cfg!(windows) {
                std::env::var_os("LOCALAPPDATA")
                    .map(|dir| PathBuf::from(dir).join("nsh/bin/nsh.exe"))
            } else {
                std::env::var_os("HOME").map(|dir| PathBuf::from(dir).join(".nsh/bin/nsh"))
            }
        })?;
    let candidate = fs::canonicalize(candidate).ok()?;
    (candidate.is_file() && Some(&candidate) != current.as_ref()).then_some(candidate)
}
