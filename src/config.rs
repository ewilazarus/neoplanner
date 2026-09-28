//! Settings, and where a project's OpenSpec root is. A project is in one of two modes:
//!
//! - **committed**: `.neoplanner/config.toml` at the project root says `mode = "committed"`,
//!   and the specs live in the project's own `openspec/` folder, in its git history.
//! - **stealth**: nothing in the project says so. `$XDG_CONFIG_HOME/neoplanner/projects.toml`
//!   maps the project to an OpenSpec store, a standalone OpenSpec repo outside it, with its
//!   own history. A project is keyed by its main working tree, so every worktree of a clone
//!   shares one store.
//!
//! `$XDG_CONFIG_HOME/neoplanner/config.toml` holds personal preferences. Every file is
//! optional; a project in neither mode isn't set up yet.

use crate::util::{git, xdg};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const PROJECT_FILE: &str = ".neoplanner/config.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Committed,
    Stealth { store: String, path: PathBuf },
}

/// `.neoplanner/config.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    mode: String,
}

/// Personal preferences, from the user's config file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct User {
    /// Whether the Stop hook reminds Claude to report progress before it finishes.
    pub stop_reminder: bool,
}

impl Default for User {
    fn default() -> Self {
        User { stop_reminder: true }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// The project root.
    pub root: PathBuf,
    /// None when the project isn't set up.
    pub mode: Option<Mode>,
    pub user: User,
    /// `$XDG_CONFIG_HOME/neoplanner`.
    pub user_dir: PathBuf,
}

impl Config {
    /// Loads the settings for the project at `root`, or the one found from the current
    /// directory: `$CLAUDE_PROJECT_DIR` when Claude Code runs us, else the enclosing git
    /// repository, else the current directory.
    pub fn load(root: Option<&Path>) -> Result<Config> {
        let root = match root {
            Some(root) => root.to_path_buf(),
            None => find_root()?,
        };
        let user_dir = xdg("XDG_CONFIG_HOME", ".config").join("neoplanner");
        let user = read_user(&user_dir)?;
        let committed = read_project(&root)?;
        let stealth = read_stealth(&user_dir, &project_key(&root))?;
        let mode = match (committed, stealth) {
            (true, Some(_)) => bail!(
                "This project is set up twice: committed, in {PROJECT_FILE}, and stealth, in {}. Remove one.",
                user_dir.join("projects.toml").display()
            ),
            (true, None) => Some(Mode::Committed),
            (false, stealth) => stealth,
        };
        Ok(Config {
            root,
            mode,
            user,
            user_dir,
        })
    }

    /// The OpenSpec root: the folder holding `openspec/`. The project itself in committed
    /// mode, or the store.
    pub fn home(&self) -> Option<PathBuf> {
        match &self.mode {
            Some(Mode::Committed) => Some(self.root.clone()),
            Some(Mode::Stealth { path, .. }) => Some(path.clone()),
            None => None,
        }
    }

    /// The `openspec/` folder, where the specs and changes are.
    pub fn openspec_dir(&self) -> Option<PathBuf> {
        self.home().map(|h| h.join("openspec"))
    }

    /// The store id, in stealth mode.
    pub fn store(&self) -> Option<&str> {
        match &self.mode {
            Some(Mode::Stealth { store, .. }) => Some(store),
            _ => None,
        }
    }

    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            Some(Mode::Committed) => "committed",
            Some(Mode::Stealth { .. }) => "stealth",
            None => "not set up",
        }
    }

    pub fn projects_file(&self) -> PathBuf {
        self.user_dir.join("projects.toml")
    }

    pub fn key(&self) -> String {
        project_key(&self.root)
    }
}

/// The project's key in `projects.toml`: its main working tree, symlinks resolved, so a
/// linked worktree finds the same store as the clone it belongs to.
pub fn project_key(root: &Path) -> String {
    let main = git(root, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .map(|d| PathBuf::from(d.trim()))
        .and_then(|d| d.parent().map(Path::to_path_buf));
    let dir = main.unwrap_or_else(|| root.to_path_buf());
    dir.canonicalize().unwrap_or(dir).display().to_string()
}

fn find_root() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_PROJECT_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let cwd = std::env::current_dir().context("can't read the current directory")?;
    let toplevel = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
    Ok(toplevel.unwrap_or(cwd))
}

/// Whether the project is committed, from its settings file.
fn read_project(root: &Path) -> Result<bool> {
    let file = root.join(PROJECT_FILE);
    if !file.is_file() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(&file).with_context(|| format!("can't read {}", file.display()))?;
    let project: ProjectFile = toml::from_str(&text).with_context(|| format!("{} isn't valid", file.display()))?;
    if project.mode != "committed" {
        bail!(
            "{}: `mode` can only be \"committed\". A stealth project keeps nothing in the repo.",
            file.display()
        );
    }
    Ok(true)
}

/// The store the user keeps this project in, from their projects file.
fn read_stealth(user_dir: &Path, key: &str) -> Result<Option<Mode>> {
    let file = user_dir.join("projects.toml");
    let table = read_table(&file)?;
    let Some(entry) = table
        .get("projects")
        .and_then(|p| p.as_table())
        .and_then(|p| p.get(key))
    else {
        return Ok(None);
    };
    let field = |k: &str| entry.get(k).and_then(|v| v.as_str()).map(String::from);
    match (field("store"), field("path")) {
        (Some(store), Some(path)) if Path::new(&path).is_absolute() => Ok(Some(Mode::Stealth {
            store,
            path: PathBuf::from(path),
        })),
        _ => bail!(
            "{}: the entry for {key} needs `store`, the store's id, and `path`, its absolute path.",
            file.display()
        ),
    }
}

pub fn read_table(file: &Path) -> Result<toml::Table> {
    if !file.is_file() {
        return Ok(toml::Table::new());
    }
    let text = std::fs::read_to_string(file).with_context(|| format!("can't read {}", file.display()))?;
    toml::from_str(&text).with_context(|| format!("{} isn't valid", file.display()))
}

fn read_user(dir: &Path) -> Result<User> {
    let file = dir.join("config.toml");
    if !file.is_file() {
        return Ok(User::default());
    }
    let text = std::fs::read_to_string(&file).with_context(|| format!("can't read {}", file.display()))?;
    toml::from_str(&text).with_context(|| format!("{} isn't valid", file.display()))
}
