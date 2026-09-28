//! Small helpers shared by the commands.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Trims, and collapses runs of spaces and tabs into one space.
pub fn squeeze(s: &str) -> String {
    s.split([' ', '\t'])
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escapes `|` for a Markdown table cell.
pub fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Runs git, and returns its stdout when it succeeds.
pub fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Resolves symlinks in the deepest existing directory, since the file may not exist yet.
pub fn resolve(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => resolve(dir).join(name),
        _ => path.to_path_buf(),
    }
}

/// Writes a file through a temporary file, creating its directory.
pub fn write_file(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("can't create {}", dir.display()))?;
    }
    let tmp = path.with_extension("neoplanner-tmp");
    std::fs::write(&tmp, text).with_context(|| format!("can't write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("can't write {}", path.display()))
}

/// `$HOME`, or `/`.
pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// An XDG base directory: the variable when it's set, else the fallback under `$HOME`.
pub fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(fallback))
}
