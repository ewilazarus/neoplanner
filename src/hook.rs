//! `neoplanner hook`: every Claude Code hook the plugin registers, reading the hook's JSON
//! on stdin. It keeps the OpenSpec workflow behind `neoplanner:agent`, and its artifacts
//! readable by everyone:
//!
//! - SubagentStart gives the agent its rules, with this project's OpenSpec root filled in,
//!   and tells it whether its task is a JSON request or the user's slash command.
//! - PreToolUse on Agent and SendMessage checks every request to the agent against the
//!   contract, and refuses one that doesn't fit, saying what to fix.
//! - SubagentStop checks the agent's reply to a request the same way, and sends it back
//!   once to fix it.
//! - PreToolUse on Bash approves the read-only neoplanner commands for anyone, and for the
//!   agent every neoplanner command, read-only git and `date`, each alone. A plugin agent
//!   can't carry permission rules of its own.
//! - PreToolUse on file tools: anyone may read the artifacts, and in stealth mode, where
//!   the store is outside the project, reading them is approved without a prompt. Only the
//!   agent writes them, and the agent writes nothing else.
//! - PostToolUse on the agent's writes to a change's delta specs validates the change, and
//!   hands back its errors. On the main agent's writes to the project, while a change is in
//!   flight, it notes the change for the stop reminder.
//! - Stop reminds Claude, once, to report progress when the session planned or reported
//!   on a change, then changed project files, and hasn't reported since. The user's
//!   `stop_reminder = false` turns it off.
//!
//! Anything unexpected fails open: the tool call goes through, and the error is reported
//! as a non-blocking hook error.

use crate::config::{Config, Mode};
use crate::contract::{self, Side};
use crate::openspec;
use crate::util::{resolve, squeeze};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const AGENT: &str = "neoplanner:agent";
const PLAN: &str = include_str!("../rules/plan.md");
const PROGRESS: &str = include_str!("../rules/progress.md");

/// `neoplanner os` commands that only read.
const READ_OS: [&str; 8] = [
    "list",
    "show",
    "status",
    "validate",
    "instructions",
    "context",
    "schemas",
    "templates",
];

struct Hook {
    input: Value,
    event: String,
    session: String,
    agent: Option<String>,
    agent_type: String,
    tool: String,
    cwd: PathBuf,
    config: Config,
    state: PathBuf,
}

/// What the hook tells Claude Code, if anything.
enum Out {
    Quiet,
    Deny(String),
    Allow(String),
    Context(String),
    Block(String),
}

pub fn run(input: &str) -> Result<Option<String>> {
    let input: Value = serde_json::from_str(input).context("the hook input isn't JSON")?;
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let cwd = PathBuf::from(s("cwd"));
    let root = std::env::var_os("CLAUDE_PROJECT_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cwd.as_os_str().is_empty() {
                std::env::current_dir().unwrap_or_default()
            } else {
                cwd.clone()
            }
        });
    let config = Config::load(Some(&root))?;
    let state = std::env::var_os("CLAUDE_PLUGIN_DATA")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("neoplanner"))
        .join("sessions");
    let hook = Hook {
        event: s("hook_event_name"),
        session: if s("session_id").is_empty() {
            "unknown".into()
        } else {
            s("session_id")
        },
        agent: input.get("agent_id").and_then(Value::as_str).map(String::from),
        agent_type: s("agent_type"),
        tool: s("tool_name"),
        cwd: if cwd.as_os_str().is_empty() { root.clone() } else { cwd },
        config,
        state,
        input,
    };
    Ok(match hook.dispatch()? {
        Out::Quiet => None,
        Out::Deny(reason) => Some(
            json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason}})
                .to_string(),
        ),
        Out::Allow(reason) => Some(
            json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow", "permissionDecisionReason": reason}})
                .to_string(),
        ),
        Out::Context(text) => {
            Some(json!({"hookSpecificOutput": {"hookEventName": hook.event, "additionalContext": text}}).to_string())
        }
        Out::Block(reason) => Some(json!({"decision": "block", "reason": reason}).to_string()),
    })
}

fn key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "_.-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl Hook {
    fn str_at(&self, pointer: &str) -> String {
        self.input
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    fn by_agent(&self) -> bool {
        self.agent.is_some() && self.agent_type == AGENT
    }

    fn marker(&self, name: &str) -> PathBuf {
        self.state.join(format!("{}.{name}", key(&self.session)))
    }

    fn contract_marker(&self, agent: &str) -> PathBuf {
        self.state.join(format!("{}.contract", key(agent)))
    }

    fn touch(&self, path: &Path) {
        let _ = std::fs::create_dir_all(&self.state);
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(path);
        self.prune();
    }

    /// Drops markers from sessions older than a week.
    fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.state) else {
            return;
        };
        let week = std::time::Duration::from_secs(7 * 24 * 3600);
        for e in entries.flatten() {
            if e.metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > week))
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    fn dispatch(&self) -> Result<Out> {
        match self.event.as_str() {
            "SubagentStart" if self.agent_type == AGENT => Ok(self.give_rules()),
            "SubagentStop" => Ok(self.check_reply()),
            "Stop" => Ok(self.stop_reminder()),
            "PostToolUse" => match self.tool.as_str() {
                "Edit" | "Write" | "MultiEdit" => Ok(self.after_write()),
                _ => Ok(Out::Quiet),
            },
            "PreToolUse" => match self.tool.as_str() {
                "Agent" | "SendMessage" => Ok(self.check_request()),
                "Bash" => Ok(self.approve_command()),
                "Read" | "Grep" | "Glob" | "Edit" | "Write" | "MultiEdit" => Ok(self.guard_file()),
                _ => Ok(Out::Quiet),
            },
            _ => Ok(Out::Quiet),
        }
    }

    // --- the contract --------------------------------------------------------------------

    fn bin(&self) -> String {
        match std::env::var_os("CLAUDE_PLUGIN_ROOT").filter(|d| !d.is_empty()) {
            Some(root) => PathBuf::from(root).join("bin/neoplanner").display().to_string(),
            None => std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "neoplanner".into()),
        }
    }

    /// The agent's rules, with this project's OpenSpec root and the binary's path filled in.
    pub fn rules(&self) -> String {
        let openspec = self
            .config
            .openspec_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "(none yet: this project isn't set up, and /neoplanner:init sets it up)".into());
        let fill = |s: &str| {
            s.replace("{{bin}}", &self.bin())
                .replace("{{openspec}}", &openspec)
                .replace("{{mode}}", self.config.mode_name())
                .replace("{{project}}", &self.config.root.display().to_string())
        };
        format!(
            "# The planning rules\n\n{}\n\n# The progress rules\n\n{}",
            fill(PLAN),
            fill(PROGRESS)
        )
    }

    fn give_rules(&self) -> Out {
        let pending = self.marker("pending");
        let lines = std::fs::read_to_string(&pending).unwrap_or_default();
        let mode = if !lines.trim().is_empty() {
            // One line per request sent: this agent takes one of them.
            let rest: String = lines.lines().skip(1).map(|l| format!("{l}\n")).collect();
            let _ = std::fs::write(&pending, rest);
            if let Some(agent) = &self.agent {
                self.touch(&self.contract_marker(agent));
            }
            "This task came from the main agent, so it is a JSON request, and your final message is a JSON reply."
        } else {
            "This task is the user's slash command, so your final message is for the user to read: plain Markdown, not JSON. If you need the user's answers, end with the numbered questions, and say they can answer in the conversation for Claude to pass on."
        };
        Out::Context(format!("{mode}\n\n{}", self.rules()))
    }

    fn check_request(&self) -> Out {
        let message = if self.tool == "Agent" {
            if self.str_at("/tool_input/subagent_type") != AGENT {
                return Out::Quiet;
            }
            self.str_at("/tool_input/prompt")
        } else {
            let m = self
                .input
                .pointer("/tool_input/message")
                .or_else(|| self.input.pointer("/tool_input/content"));
            let m = match m {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => return Out::Quiet,
            };
            let t = m.trim_start();
            if !(t.starts_with('{') || t.starts_with("```")) || !m.contains("\"kind\"") {
                return Out::Quiet;
            }
            m
        };
        let problems = contract::check(Side::Request, &message);
        if !problems.is_empty() {
            return Out::Deny(format!(
                "This request to neoplanner:agent doesn't fit its contract: {} Load the neoplanner:contract skill for the format, and send it again.",
                problems.join(" ")
            ));
        }
        if self.tool == "Agent" {
            let pending = self.marker("pending");
            let mut lines = std::fs::read_to_string(&pending).unwrap_or_default();
            lines.push_str("pending\n");
            let _ = std::fs::create_dir_all(&self.state);
            let _ = std::fs::write(&pending, lines);
        }
        // A change is in flight from its plan until its archive, and a progress report
        // covers the code changed so far, so the stop reminder waits.
        match contract::parse(&message)
            .ok()
            .and_then(|o| o.get("kind").and_then(Value::as_str).map(String::from))
            .as_deref()
        {
            Some("plan" | "revise") => self.touch(&self.marker("inflight")),
            Some("progress") => {
                self.touch(&self.marker("inflight"));
                let _ = std::fs::remove_file(self.marker("changed"));
            }
            Some("archive") => {
                let _ = std::fs::remove_file(self.marker("inflight"));
                let _ = std::fs::remove_file(self.marker("changed"));
            }
            _ => {}
        }
        Out::Quiet
    }

    fn check_reply(&self) -> Out {
        let Some(agent) = &self.agent else { return Out::Quiet };
        if self.agent_type != AGENT || !self.contract_marker(agent).exists() {
            return Out::Quiet;
        }
        if self.input.get("stop_hook_active") == Some(&Value::Bool(true)) {
            return Out::Quiet;
        }
        let problems = contract::check(Side::Reply, &self.str_at("/last_assistant_message"));
        if problems.is_empty() {
            return Out::Quiet;
        }
        Out::Block(format!(
            "Your reply doesn't fit your contract: {} Reply again with one JSON object, as your instructions describe, and nothing else.",
            problems.join(" ")
        ))
    }

    /// Approves neoplanner's read-only commands for anyone, and the agent's own commands.
    /// Anything with a shell operator, a substitution or a redirection goes through the
    /// usual permission check.
    fn approve_command(&self) -> Out {
        let command = self.str_at("/tool_input/command");
        let command = command.trim();
        if command.contains(|c| ";&|`$<>(){}\n".contains(c)) {
            return Out::Quiet;
        }
        let command = command.replace(['"', '\''], "");
        let bin = self.bin();
        let rest = command
            .strip_prefix(&format!("{bin} "))
            .or_else(|| command.strip_prefix("neoplanner "));
        if let Some(rest) = rest {
            let words: Vec<&str> = rest.split_whitespace().collect();
            let sub = words.first().copied().unwrap_or("");
            let read_only = match sub {
                "root" | "status" | "config" | "doctor" | "--version" => true,
                "contract" => words.get(1) == Some(&"check"),
                "os" => words.get(1).is_some_and(|w| READ_OS.contains(w)),
                _ => false,
            };
            if read_only {
                return Out::Allow("A read-only neoplanner command.".into());
            }
            if self.by_agent() && !sub.is_empty() && sub != "hook" && sub.chars().all(|c| c.is_ascii_lowercase()) {
                return Out::Allow("A command neoplanner:agent's rules call for.".into());
            }
            return Out::Quiet;
        }
        if !self.by_agent() {
            return Out::Quiet;
        }
        let approved = if command.starts_with("git ") {
            let re = regex::Regex::new(r"^git( -C [^ ]+)? (status|diff|log|show|ls-files|ls-tree|rev-parse|grep)( |$)")
                .unwrap();
            re.is_match(&command) && !format!(" {command}").contains(" --output")
        } else {
            command == "date" || command == "date +%F"
        };
        if approved {
            Out::Allow("A command neoplanner:agent's rules call for: a read-only git command, or date.".into())
        } else {
            Out::Quiet
        }
    }

    // --- files -------------------------------------------------------------------------

    /// What the hooks keep: the whole store in stealth mode, or the project's `openspec/`.
    fn guarded(&self) -> Option<PathBuf> {
        match &self.config.mode {
            Some(Mode::Stealth { path, .. }) => Some(path.clone()),
            Some(Mode::Committed) => self.config.openspec_dir(),
            None => None,
        }
    }

    fn stealth(&self) -> bool {
        matches!(self.config.mode, Some(Mode::Stealth { .. }))
    }

    /// The tool's target, absolute with symlinks resolved, if it names one.
    fn target(&self) -> Option<PathBuf> {
        let target = self.str_at("/tool_input/file_path");
        let target = if target.is_empty() {
            self.str_at("/tool_input/path")
        } else {
            target
        };
        if target.is_empty() {
            return None;
        }
        let path = Path::new(&target);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        };
        Some(resolve(&path))
    }

    /// The target relative to the guarded folder, or None when it's outside it.
    fn guarded_relative(&self) -> Option<String> {
        let guarded = resolve(&self.guarded()?);
        let path = self.target()?;
        if path == guarded {
            return Some(".".into());
        }
        path.strip_prefix(&guarded)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    }

    fn guard_file(&self) -> Out {
        let reading = matches!(self.tool.as_str(), "Read" | "Grep" | "Glob");
        let Some(_) = self.guarded_relative() else {
            if !reading && self.by_agent() && self.target().is_some() && self.guarded().is_some() {
                return Out::Deny(format!(
                    "neoplanner:agent writes only planning artifacts, in {}. The code is the main agent's to change: say what it needs in your reply's summary or tasks.",
                    self.guarded().unwrap_or_default().display()
                ));
            }
            return Out::Quiet;
        };
        if reading {
            return if self.stealth() {
                Out::Allow(
                    "Reading the planning artifacts neoplanner:agent keeps in this project's OpenSpec store.".into(),
                )
            } else {
                Out::Quiet
            };
        }
        if !self.by_agent() {
            return Out::Deny(
                "The OpenSpec artifacts are written by the neoplanner:agent agent, and nobody else writes them; anyone may read them. To tick tasks or record a deviation, send it a `progress` request; to change the plan, a `revise`. Load the neoplanner:contract skill for the format.".into(),
            );
        }
        if self.stealth() {
            Out::Allow("neoplanner:agent writing a planning artifact in this project's OpenSpec store.".into())
        } else {
            Out::Quiet
        }
    }

    fn after_write(&self) -> Out {
        match self.guarded_relative() {
            Some(rel) if self.by_agent() => self.validate_delta(&rel),
            Some(_) => Out::Quiet,
            None => {
                self.note_change();
                Out::Quiet
            }
        }
    }

    /// After the agent writes a change's delta spec: the change's validation errors.
    fn validate_delta(&self, rel: &str) -> Out {
        let rel = if self.stealth() {
            rel.strip_prefix("openspec/").unwrap_or("")
        } else {
            rel
        };
        let re = regex::Regex::new(r"^changes/([a-z0-9][a-z0-9-]*)/specs/.+\.md$").unwrap();
        let Some(change) = re.captures(rel).map(|c| c[1].to_string()) else {
            return Out::Quiet;
        };
        if change == "archive" {
            return Out::Quiet;
        }
        match openspec::validate_errors(&self.config, &change) {
            Ok(errors) if !errors.is_empty() => Out::Context(format!(
                "`openspec validate {change}` found errors after this write. Fix them now, while the spec is in hand:\n\n{}",
                errors.join("\n")
            )),
            _ => Out::Quiet,
        }
    }

    /// Notes that Claude changed a project file while a change is in flight.
    fn note_change(&self) {
        if self.by_agent() || !self.marker("inflight").exists() {
            return;
        }
        let Some(path) = self.target() else { return };
        let root = resolve(&self.config.root);
        if path.starts_with(&root) && !path.starts_with(root.join(".git")) {
            self.touch(&self.marker("changed"));
        }
    }

    // --- the stop reminder -----------------------------------------------------------------

    fn stop_reminder(&self) -> Out {
        if !self.config.user.stop_reminder || self.input.get("stop_hook_active") == Some(&Value::Bool(true)) {
            return Out::Quiet;
        }
        let changed = self.marker("changed");
        if !changed.exists() || !self.marker("inflight").exists() || self.config.mode.is_none() {
            return Out::Quiet;
        }
        let _ = std::fs::remove_file(changed);
        Out::Block(squeeze(
            "Before finishing: this session is working on an OpenSpec change, changed project files, and hasn't reported progress to neoplanner:agent since. If the work finished tasks, departed from the design, or hit a blocker, send it a `progress` request in the background; the neoplanner:contract skill has the format. If the edits weren't part of the change, say so in one line, and stop.",
        ))
    }
}
