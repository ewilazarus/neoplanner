//! `neoplanner init plan|apply`: makes init's changes in a project, without overwriting
//! anything. Where it keeps the specs depends on the mode:
//!
//! - **committed**: writes `.neoplanner/config.toml`; creates `openspec/` with
//!   `openspec init --tools none`, so no tool's own commands or skills come with it; adds
//!   the marketplace and plugin to `.claude/settings.json`; and writes the neoplanner
//!   section into CLAUDE.md, replacing the text between its markers in CLAUDE.md,
//!   .claude/CLAUDE.md or AGENTS.md, or else appending it to CLAUDE.md. A project with only
//!   AGENTS.md needs `--agents-md`: `import` creates CLAUDE.md with `@AGENTS.md` and the
//!   section, and `append` adds the section to AGENTS.md.
//! - **stealth**: touches no tracked file. It creates and registers an OpenSpec store
//!   outside the project, with its own git repository, or adopts one already registered;
//!   maps the project to it in the user's `projects.toml`; writes the section into
//!   `CLAUDE.local.md` and the plugin into `.claude/settings.local.json`; and adds both to
//!   the clone's `.git/info/exclude` unless git already ignores them.
//!
//! A setting that already has a different value is reported as kept, never changed.
//! Running it again changes nothing. Switching a project between modes is refused.

use crate::config::{Config, Mode, PROJECT_FILE, read_table};
use crate::openspec;
use crate::util::{git, write_file, xdg};
use anyhow::{Result, bail};
use serde_json::{Map, Value, json};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const SECTION: &str = include_str!("../assets/CLAUDE-section.md");
const START: &str = "<!-- neoplanner:start -->";
const END: &str = "<!-- neoplanner:end -->";
const LOCAL: &str = "CLAUDE.local.md";
const LOCAL_SETTINGS: &str = ".claude/settings.local.json";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Want {
    Committed,
    Stealth,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AgentsMd {
    Import,
    Append,
}

pub struct Options {
    pub apply: bool,
    pub mode: Option<Want>,
    pub store_id: Option<String>,
    pub store_path: Option<PathBuf>,
    pub agents_md: Option<AgentsMd>,
}

struct Run<'a> {
    config: &'a Config,
    opts: &'a Options,
    out: String,
    changed: bool,
}

impl Run<'_> {
    fn path(&self, rel: &str) -> PathBuf {
        self.config.root.join(rel)
    }

    fn change(&mut self, line: impl AsRef<str>) {
        self.changed = true;
        self.say(line);
    }

    fn say(&mut self, line: impl AsRef<str>) {
        let _ = writeln!(self.out, "{}", line.as_ref());
    }

    fn new_file(&self, rel: &str) -> &'static str {
        if self.path(rel).exists() { "" } else { " new file" }
    }

    fn in_git(&self) -> bool {
        git(&self.config.root, &["rev-parse", "--is-inside-work-tree"]).is_some()
    }

    fn tracked(&self, rel: &str) -> bool {
        git(&self.config.root, &["ls-files", "--error-unmatch", rel]).is_some()
    }
}

/// The section, with where the specs are filled in.
fn section(where_: &str) -> String {
    SECTION.replace("{{openspec}}", where_)
}

pub fn run(config: &Config, opts: &Options) -> Result<String> {
    let mut r = Run {
        config,
        opts,
        out: String::new(),
        changed: false,
    };
    let current = match config.mode {
        Some(Mode::Committed) => Some(Want::Committed),
        Some(Mode::Stealth { .. }) => Some(Want::Stealth),
        None => None,
    };
    let want = match (current, opts.mode) {
        (Some(c), Some(w)) if c != w => bail!(
            "This project is already set up in {} mode. Switching modes isn't supported yet: move the specs by hand, and remove {} first.",
            config.mode_name(),
            if c == Want::Committed {
                PROJECT_FILE.to_string()
            } else {
                format!("its entry in {}", config.projects_file().display())
            }
        ),
        (Some(c), _) => c,
        (None, Some(w)) => w,
        (None, None) if opts.apply => bail!("Choose a mode: --mode committed or --mode stealth."),
        (None, None) => {
            r.say("mode: needs a choice");
            r.say("  --mode committed: the specs live in openspec/, in the repo, and everyone shares them");
            r.say("  --mode stealth: the specs live in an OpenSpec store outside the repo, with its own history, and no tracked file changes");
            overlap(&mut r);
            return Ok(r.out);
        }
    };
    let version = openspec::version()?;
    if let Some(Mode::Stealth { store, .. }) = &config.mode {
        r.say(format!("mode: stealth, store {store} (OpenSpec {version})"));
    } else {
        r.say(format!(
            "mode: {} (OpenSpec {version})",
            if want == Want::Committed {
                "committed"
            } else {
                "stealth"
            }
        ));
    }
    match want {
        Want::Committed => {
            if let Some(file) = marked_file(&r) {
                if replaced(&std::fs::read_to_string(r.path(&file))?, &section("x")).is_none() {
                    bail!(
                        "{file} has the neoplanner start marker but no end marker. Fix it by hand, then run init again."
                    );
                }
            }
            if opts.apply
                && opts.agents_md.is_none()
                && marked_file(&r).is_none()
                && claude_md(&r).is_none()
                && r.path("AGENTS.md").is_file()
            {
                bail!("The project has AGENTS.md but no CLAUDE.md. Pass --agents-md import or --agents-md append.");
            }
            config_file(&mut r)?;
            openspec_folder(&mut r)?;
            settings(&mut r, ".claude/settings.json")?;
            claude_section(&mut r)?;
        }
        Want::Stealth => {
            for rel in [LOCAL, LOCAL_SETTINGS] {
                if r.tracked(rel) {
                    bail!(
                        "{rel} is tracked by git, so stealth mode would change a committed file. Untrack it, or use --mode committed."
                    );
                }
            }
            if r.path(PROJECT_FILE).is_file() {
                bail!("{PROJECT_FILE} says this project is committed. Remove it to use stealth mode.");
            }
            let path = store(&mut r)?;
            local_section(&mut r, &path)?;
            settings(&mut r, LOCAL_SETTINGS)?;
            exclude(&mut r)?;
            if r.path("openspec").is_dir() {
                r.say("openspec/:\n  note: the project already has an openspec/ folder; stealth mode leaves it alone and plans in the store");
            }
        }
    }
    overlap(&mut r);
    if !r.changed {
        r.say("Nothing to change: the project already has everything init sets up.");
    }
    Ok(r.out)
}

/// Notes OpenSpec's own commands and skills for Claude, if the project has them.
fn overlap(r: &mut Run) {
    let skills = std::fs::read_dir(r.path(".claude/skills"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with("openspec-"));
    if skills || r.path(".claude/commands/opsx").is_dir() {
        r.say(".claude/:\n  note: the project has OpenSpec's own /opsx commands or openspec-* skills. They run the workflow in the main conversation, and the hooks refuse their writes to the artifacts; neoplanner's /neoplanner:* commands replace them");
    }
}

fn config_file(r: &mut Run) -> Result<()> {
    if r.path(PROJECT_FILE).is_file() {
        return Ok(());
    }
    r.change(format!("{PROJECT_FILE}: new file, with mode = \"committed\""));
    if r.opts.apply {
        write_file(
            &r.path(PROJECT_FILE),
            "# neoplanner's settings for this project, shared by everyone who works on it.\n\n\
             # The specs live in openspec/, in the repo.\nmode = \"committed\"\n",
        )?;
    }
    Ok(())
}

fn openspec_folder(r: &mut Run) -> Result<()> {
    if r.path("openspec").is_dir() {
        return Ok(());
    }
    r.change("openspec/: new, from `openspec init --tools none`");
    if r.opts.apply {
        openspec::init(&r.config.root)?;
    }
    Ok(())
}

/// A store id from the project's folder name: lowercase letters, digits and dashes.
fn default_id(key: &str) -> String {
    let name = Path::new(key)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let id: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() { "project".into() } else { id }
}

/// Sets up the store and maps the project to it. Returns the store's path.
fn store(r: &mut Run) -> Result<PathBuf> {
    if let Some(Mode::Stealth { store, path }) = &r.config.mode {
        if !path.join("openspec").is_dir() {
            r.say(format!(
                "store {store}:\n  note: {} has no openspec/ folder; check the store with `openspec store doctor {store}`",
                path.display()
            ));
        }
        return Ok(path.clone());
    }
    let key = r.config.key();
    let id = r.opts.store_id.clone().unwrap_or_else(|| default_id(&key));
    if !crate::contract::change_name(&id) {
        bail!("A store id is lowercase letters, digits and single dashes, such as \"my-project\", not \"{id}\".");
    }
    let registered = openspec::stores()?.into_iter().find(|(s, _)| *s == id).map(|(_, p)| p);
    let path = match (registered, &r.opts.store_path) {
        (Some(have), Some(want)) if *want != have => bail!(
            "The store {id} is already registered at {}, not {}. Pick another --store-id, or leave out --store-path.",
            have.display(),
            want.display()
        ),
        (Some(have), _) => {
            r.say(format!("store {id}:\n  kept, already registered at {}", have.display()));
            have
        }
        (None, want) => {
            let path = want
                .clone()
                .unwrap_or_else(|| xdg("XDG_DATA_HOME", ".local/share").join("neoplanner/stores").join(&id));
            if !path.is_absolute() {
                bail!("--store-path must be absolute, not {}.", path.display());
            }
            r.change(format!(
                "store {id}: new, at {}, with its own git repository (openspec store setup)",
                path.display()
            ));
            if r.opts.apply {
                openspec::store_setup(&id, &path)?;
            }
            path
        }
    };
    let file = r.config.projects_file();
    r.change(format!("{}: map {key} to the store {id}", file.display()));
    if r.opts.apply {
        let mut table = read_table(&file)?;
        let projects = table
            .entry("projects")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let toml::Value::Table(projects) = projects {
            let mut entry = toml::Table::new();
            entry.insert("store".into(), toml::Value::String(id.clone()));
            entry.insert("path".into(), toml::Value::String(path.display().to_string()));
            projects.insert(key, toml::Value::Table(entry));
        }
        write_file(
            &file,
            &format!(
                "# neoplanner: the projects kept in stealth mode on this machine, each with its OpenSpec store.\n\n{}",
                toml::to_string(&table)?
            ),
        )?;
    }
    Ok(path)
}

fn wanted_settings() -> Value {
    json!({
        "extraKnownMarketplaces": {
            "neoplanner": {"source": {"source": "github", "repo": "ewilazarus/neoplanner"}}
        },
        "enabledPlugins": {"neoplanner@neoplanner": true}
    })
}

fn settings(r: &mut Run, rel: &str) -> Result<()> {
    let path = r.path(rel);
    let mut current: Map<String, Value> = if path.is_file() {
        match serde_json::from_str(&std::fs::read_to_string(&path)?) {
            Ok(Value::Object(m)) => m,
            _ => bail!("{rel} isn't valid JSON. Fix it, then run init again."),
        }
    } else {
        Map::new()
    };
    let wanted = wanted_settings();
    let mut lines = Vec::new();
    let mut adds = false;
    for (section, entries) in wanted.as_object().unwrap() {
        for (key, value) in entries.as_object().unwrap() {
            match current.get(section).and_then(|s| s.get(key)) {
                None | Some(Value::Null) => {
                    lines.push(format!("  add {section}.{key}"));
                    adds = true;
                }
                Some(v) if v == value => {}
                Some(v) => lines.push(format!("  kept {section}.{key}, already {v}")),
            }
        }
    }
    if lines.is_empty() {
        return Ok(());
    }
    if adds {
        let header = format!("{rel}:{}", r.new_file(rel));
        r.change(header);
    } else {
        r.say(format!("{rel}:"));
    }
    for l in &lines {
        r.say(l);
    }
    if r.opts.apply && adds {
        for (section, entries) in wanted.as_object().unwrap() {
            let s = current.entry(section.clone()).or_insert_with(|| json!({}));
            if let Value::Object(s) = s {
                for (key, value) in entries.as_object().unwrap() {
                    if s.get(key).is_none_or(Value::is_null) {
                        s.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        write_file(&path, &(serde_json::to_string_pretty(&Value::Object(current))? + "\n"))?;
    }
    Ok(())
}

/// The file with the neoplanner markers, if any.
fn marked_file(r: &Run) -> Option<String> {
    ["CLAUDE.md", ".claude/CLAUDE.md", "AGENTS.md"]
        .into_iter()
        .find(|f| has_marker(&r.path(f)))
        .map(String::from)
}

fn has_marker(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| t.lines().any(|l| l == START))
}

/// The CLAUDE.md to append to, if there is one.
fn claude_md(r: &Run) -> Option<String> {
    ["CLAUDE.md", ".claude/CLAUDE.md"]
        .into_iter()
        .find(|f| r.path(f).is_file())
        .map(String::from)
}

/// The text with everything between its markers replaced by the section, or None when the
/// start marker has no end marker after it.
fn replaced(text: &str, section: &str) -> Option<String> {
    let mut out = String::new();
    let mut lines = text.split_inclusive('\n');
    let mut done = false;
    while let Some(line) = lines.next() {
        if !done && line.trim_end_matches(['\n', '\r']) == START {
            out.push_str(section);
            loop {
                let l = lines.next()?;
                if l.trim_end_matches(['\n', '\r']) == END {
                    break;
                }
            }
            done = true;
        } else {
            out.push_str(line);
        }
    }
    Some(out)
}

fn append_section(path: &Path, section: &str) -> Result<()> {
    let mut text = std::fs::read_to_string(path).unwrap_or_default();
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str(section);
    write_file(path, &text)
}

/// Replaces the section in a marked file, if it changed.
fn update_marked(r: &mut Run, file: &str, section: &str) -> Result<()> {
    let text = std::fs::read_to_string(r.path(file))?;
    let Some(new) = replaced(&text, section) else {
        bail!("{file} has the neoplanner start marker but no end marker. Fix it by hand, then run init again.");
    };
    if new != text {
        r.change(format!("{file}: update the neoplanner section"));
        if r.opts.apply {
            write_file(&r.path(file), &new)?;
        }
    }
    Ok(())
}

fn claude_section(r: &mut Run) -> Result<()> {
    let section = section("`openspec/`, in this repository");
    if let Some(file) = marked_file(r) {
        return update_marked(r, &file, &section);
    }
    if let Some(file) = claude_md(r) {
        r.change(format!("{file}: append the neoplanner section"));
        if r.opts.apply {
            append_section(&r.path(&file), &section)?;
        }
    } else if r.path("AGENTS.md").is_file() {
        match r.opts.agents_md {
            Some(AgentsMd::Import) => {
                r.change("CLAUDE.md: new file, importing AGENTS.md, with the neoplanner section");
                if r.opts.apply {
                    write_file(&r.path("CLAUDE.md"), &format!("@AGENTS.md\n\n{section}"))?;
                }
            }
            Some(AgentsMd::Append) => {
                r.change("AGENTS.md: append the neoplanner section");
                if r.opts.apply {
                    append_section(&r.path("AGENTS.md"), &section)?;
                }
            }
            None => r.change(
                "CLAUDE.md: needs a choice. The project has AGENTS.md but no CLAUDE.md: --agents-md import creates CLAUDE.md importing AGENTS.md, and --agents-md append adds the section to AGENTS.md.",
            ),
        }
    } else {
        r.change("CLAUDE.md: new file, with the neoplanner section");
        if r.opts.apply {
            write_file(&r.path("CLAUDE.md"), &section)?;
        }
    }
    Ok(())
}

/// The section in CLAUDE.local.md, which Claude Code reads alongside CLAUDE.md.
fn local_section(r: &mut Run, store: &Path) -> Result<()> {
    let section = section(&format!(
        "`{}`, an OpenSpec store outside this repository",
        store.join("openspec").display()
    ));
    if has_marker(&r.path(LOCAL)) {
        return update_marked(r, LOCAL, &section);
    }
    let header = if r.path(LOCAL).is_file() {
        format!("{LOCAL}: append the neoplanner section")
    } else {
        format!("{LOCAL}: new file, with the neoplanner section")
    };
    r.change(header);
    if r.opts.apply {
        append_section(&r.path(LOCAL), &section)?;
    }
    Ok(())
}

/// Keeps the stealth files out of git, in this clone only.
fn exclude(r: &mut Run) -> Result<()> {
    if !r.in_git() {
        r.say(".git/info/exclude:\n  note: not a git repository, so there is nothing to keep the local files out of");
        return Ok(());
    }
    let missing: Vec<&str> = [LOCAL, LOCAL_SETTINGS]
        .into_iter()
        .filter(|p| git(&r.config.root, &["check-ignore", "-q", "--no-index", p]).is_none())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let rel = git(&r.config.root, &["rev-parse", "--git-path", "info/exclude"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let header = format!("{rel}:{}", r.new_file(&rel));
    r.change(header);
    for p in &missing {
        r.say(format!("  add {p}"));
    }
    if r.opts.apply {
        let path = r.path(&rel);
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("# neoplanner, stealth mode: this clone's own Claude settings\n");
        for p in &missing {
            text.push('/');
            text.push_str(p);
            text.push('\n');
        }
        write_file(&path, &text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_ids_come_from_the_folder_name() {
        assert_eq!(default_id("/Users/me/My_Project.v2"), "my-project-v2");
        assert_eq!(default_id("/"), "project");
    }
}
