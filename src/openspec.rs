//! The OpenSpec CLI, as neoplanner runs it. `neoplanner os <args>` runs `openspec` from the
//! project root and, in stealth mode, adds `--store <id>` to the commands that take it, so
//! the agent's rules read the same in both modes. `status` and `commit` are built on it.
//!
//! `$NEOPLANNER_OPENSPEC` names another `openspec` to run, for the tests.

use crate::config::Config;
use crate::util::{cell, git};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// The oldest OpenSpec with stores.
pub const MIN_VERSION: (u32, u32, u32) = (1, 6, 0);

/// The commands that act on the OpenSpec root, and so take `--store`.
const TAKES_STORE: [&str; 9] = [
    "new",
    "status",
    "instructions",
    "list",
    "show",
    "validate",
    "archive",
    "doctor",
    "context",
];

pub fn program() -> String {
    std::env::var("NEOPLANNER_OPENSPEC")
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "openspec".into())
}

fn command() -> Command {
    let mut c = Command::new(program());
    c.env("NO_COLOR", "1");
    c
}

/// The arguments, with `--store <id>` added where the command takes it.
pub fn with_store(config: &Config, args: &[String]) -> Vec<String> {
    let mut args = args.to_vec();
    let Some(store) = config.store() else { return args };
    let takes = match args.first().map(String::as_str) {
        Some("new") => args.get(1).map(String::as_str) == Some("change"),
        Some(c) => TAKES_STORE.contains(&c),
        None => false,
    };
    if takes && !args.iter().any(|a| a == "--store" || a.starts_with("--store=")) {
        args.push("--store".into());
        args.push(store.into());
    }
    args
}

fn setup_needed(config: &Config) -> Result<()> {
    if config.mode.is_none() {
        bail!("This project isn't set up for neoplanner yet: run /neoplanner:init.");
    }
    if let Some(dir) = config.openspec_dir() {
        if !dir.is_dir() {
            bail!(
                "The {} OpenSpec root, {}, is missing. Run /neoplanner:init again.",
                config.mode_name(),
                dir.display()
            );
        }
    }
    Ok(())
}

/// `neoplanner os <args>`: runs openspec, passing its output and exit status through.
pub fn run(config: &Config, args: &[String]) -> Result<ExitCode> {
    setup_needed(config)?;
    let status = command()
        .args(with_store(config, args))
        .current_dir(&config.root)
        .status()
        .with_context(|| format!("can't run `{}`: is the OpenSpec CLI installed?", program()))?;
    Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
}

/// Runs openspec and returns its stdout, or the error it printed.
fn output(config: Option<&Config>, args: &[&str]) -> Result<String> {
    let (ok, stdout, stderr, args) = raw(config, args)?;
    if !ok {
        bail!("`openspec {}` failed: {}", args.join(" "), stderr.trim());
    }
    Ok(stdout)
}

/// Runs openspec: whether it succeeded, its stdout and stderr, and the arguments it got.
fn raw(config: Option<&Config>, args: &[&str]) -> Result<(bool, String, String, Vec<String>)> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let args = match config {
        Some(c) => with_store(c, &args),
        None => args,
    };
    let mut c = command();
    c.args(&args).stdin(Stdio::null());
    if let Some(config) = config {
        c.current_dir(&config.root);
    }
    let out = c
        .output()
        .with_context(|| format!("can't run `{}`: is the OpenSpec CLI installed?", program()))?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        args,
    ))
}

/// The errors `openspec validate` finds in a change, one line each.
pub fn validate_errors(config: &Config, change: &str) -> Result<Vec<String>> {
    let (_, stdout, _, _) = raw(
        Some(config),
        &["validate", change, "--type", "change", "--json", "--no-interactive"],
    )?;
    let v: Value = serde_json::from_str(&stdout).context("`openspec validate --json` didn't print JSON")?;
    Ok(v.get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|i| i.get("issues").and_then(Value::as_array).cloned().unwrap_or_default())
        .filter(|i| i.get("level").and_then(Value::as_str) == Some("ERROR"))
        .map(|i| {
            let s = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            format!("- {}: {}", s("path"), s("message"))
        })
        .collect())
}

/// The installed OpenSpec's version, or why it won't do.
pub fn version() -> Result<String> {
    let v = output(None, &["--version"])?.trim().to_string();
    let parts: Vec<u32> = v
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .take(3)
        .filter_map(|p| p.parse().ok())
        .collect();
    let have = (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    );
    if have < MIN_VERSION {
        let (a, b, c) = MIN_VERSION;
        bail!(
            "OpenSpec {v} is too old: neoplanner needs {a}.{b}.{c} or later, for stores. Update it with `npm install -g @fission-ai/openspec@latest`."
        );
    }
    Ok(v)
}

/// The stores registered on this machine, as (id, root).
pub fn stores() -> Result<Vec<(String, PathBuf)>> {
    let text = output(None, &["store", "list", "--json"])?;
    let v: Value = serde_json::from_str(&text).context("`openspec store list --json` didn't print JSON")?;
    Ok(v.get("stores")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| {
            Some((
                s.get("id")?.as_str()?.to_string(),
                PathBuf::from(s.get("root")?.as_str()?),
            ))
        })
        .collect())
}

/// Creates and registers a store, with its own git repository.
pub fn store_setup(id: &str, path: &Path) -> Result<()> {
    output(
        None,
        &["store", "setup", id, "--path", &path.display().to_string(), "--json"],
    )
    .map(|_| ())
}

/// Creates `openspec/` in a project, without any tool's commands or skills.
pub fn init(root: &Path) -> Result<()> {
    output(None, &["init", "--tools", "none", &root.display().to_string()]).map(|_| ())
}

struct Change {
    name: String,
    done: u64,
    total: u64,
    status: String,
    modified: String,
}

fn changes(config: &Config) -> Result<Vec<Change>> {
    let text = output(Some(config), &["list", "--json"])?;
    let v: Value = serde_json::from_str(&text).context("`openspec list --json` didn't print JSON")?;
    Ok(v.get("changes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| {
            let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let n = |k: &str| c.get(k).and_then(Value::as_u64).unwrap_or(0);
            Change {
                name: s("name"),
                done: n("completedTasks"),
                total: n("totalTasks"),
                status: s("status"),
                modified: s("lastModified").get(..10).unwrap_or("").to_string(),
            }
        })
        .collect())
}

/// `neoplanner status`: the active changes and their tasks, as a table or JSON with paths.
pub fn status(config: &Config, as_json: bool) -> Result<String> {
    setup_needed(config)?;
    let dir = config.openspec_dir().unwrap_or_default();
    let changes = changes(config)?;
    if as_json {
        let list: Vec<Value> = changes
            .iter()
            .map(|c| {
                let root = dir.join("changes").join(&c.name);
                json!({
                    "name": c.name, "tasks_done": c.done, "tasks_total": c.total, "status": c.status,
                    "path": root.display().to_string(),
                    "tasks": root.join("tasks.md").display().to_string(),
                })
            })
            .collect();
        let out = json!({
            "mode": config.mode_name(), "store": config.store(),
            "openspec": dir.display().to_string(), "changes": list,
        });
        return Ok(serde_json::to_string_pretty(&out)? + "\n");
    }
    let mut out = String::new();
    let _ = writeln!(out, "OpenSpec ({}): {}\n", config.mode_name(), dir.display());
    if changes.is_empty() {
        out.push_str("No active changes.\n");
        return Ok(out);
    }
    out.push_str("| # | Change | Tasks | Status | Modified |\n|---|---|---|---|---|\n");
    for (i, c) in changes.iter().enumerate() {
        let _ = writeln!(
            out,
            "| {} | {} | {}/{} | {} | {} |",
            i + 1,
            cell(&c.name),
            c.done,
            c.total,
            cell(&c.status),
            c.modified
        );
    }
    let open: u64 = changes.iter().map(|c| c.total - c.done.min(c.total)).sum();
    let _ = writeln!(out, "\n{} active, {} open tasks.", changes.len(), open);
    Ok(out)
}

/// `neoplanner commit <message>`: in stealth mode, commits everything in the store.
pub fn commit(config: &Config, message: &str) -> Result<String> {
    setup_needed(config)?;
    let Some(crate::config::Mode::Stealth { path, .. }) = &config.mode else {
        return Ok(
            "Committed mode: the project's own git holds the specs, so there's nothing to commit here.\n".into(),
        );
    };
    if git(path, &["rev-parse", "--is-inside-work-tree"]).is_none() {
        return Ok(format!(
            "The store at {} isn't a git repository, so its history isn't kept.\n",
            path.display()
        ));
    }
    git(path, &["add", "-A"]).context("`git add` failed in the store")?;
    if git(path, &["diff", "--cached", "--quiet"]).is_some() {
        return Ok("Nothing to commit: the store is up to date.\n".into());
    }
    let message = if message.trim().is_empty() {
        "neoplanner"
    } else {
        message.trim()
    };
    git(
        path,
        &[
            "-c",
            "user.name=neoplanner",
            "-c",
            "user.email=neoplanner@localhost",
            "commit",
            "-q",
            "-m",
            message,
        ],
    )
    .context("`git commit` failed in the store")?;
    let head = git(path, &["log", "-1", "--format=%h %s"]).unwrap_or_default();
    Ok(format!("Committed in the store: {}", head))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Mode, User};

    fn config(mode: Option<Mode>) -> Config {
        Config {
            root: PathBuf::from("/p"),
            mode,
            user: User::default(),
            user_dir: PathBuf::from("/u"),
        }
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn stealth_adds_the_store_where_it_is_taken() {
        let c = config(Some(Mode::Stealth {
            store: "demo".into(),
            path: "/s".into(),
        }));
        assert_eq!(
            with_store(&c, &args(&["status", "--change", "x", "--json"])),
            args(&["status", "--change", "x", "--json", "--store", "demo"])
        );
        assert_eq!(
            with_store(&c, &args(&["new", "change", "x"])),
            args(&["new", "change", "x", "--store", "demo"])
        );
        assert_eq!(with_store(&c, &args(&["schemas"])), args(&["schemas"]));
        assert_eq!(
            with_store(&c, &args(&["list", "--store", "other"])),
            args(&["list", "--store", "other"])
        );
    }

    #[test]
    fn committed_leaves_the_arguments_alone() {
        let c = config(Some(Mode::Committed));
        assert_eq!(with_store(&c, &args(&["list", "--json"])), args(&["list", "--json"]));
    }
}
