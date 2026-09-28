//! `neoplanner contract check request|reply`: checks a message between the main agent and
//! `neoplanner:agent` against their contract. A message is one JSON object, alone or in a
//! ```json fence. Unknown fields are problems too, so a misspelt field fails instead of
//! being dropped.
//!
//! Requests:
//!   {"kind": "plan", "request": "…", "why": "…", "change": "…", "constraints": ["…"]}
//!                                                           only request is required
//!   {"kind": "ask", "question": "…", "why": "…", "change": "…"}     question is required
//!   {"kind": "revise", "change": "…", "feedback": "…"}
//!   {"kind": "progress", "change": "…",                          every list optional,
//!    "done": ["1.1"],                                            at least one not empty
//!    "deviations": [{"what": "…", "why": "…"}],
//!    "blocked": [{"task": "…", "why": "…"}]}
//!   {"kind": "archive", "change": "…"}
//!   {"kind": "answers", "answers": ["…"]}                           to an agent that asked
//!
//! A change is named in kebab-case, as OpenSpec names its folders.
//!
//! Replies, whose status names follow the A2A task states:
//!   {"status": "completed", "change": "…", "summary": "…", "answer": "…",
//!    "artifacts": {"proposal": "/abs/…", "specs": ["/abs/…"], …},
//!    "written": [{"path": "…", "change": "…"}], "next": "…", "reason": "…"}
//!   {"status": "input-required", "change": "…", "questions": [{"q": "…", "options": ["…"]}],
//!    "written": […]}
//!   {"status": "failed", "reason": "…"}
//! A completed reply has at least one of summary, answer, written or reason. Artifact
//! paths are absolute, so the main agent can read them wherever the specs live.

use serde_json::{Map, Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Request,
    Reply,
}

/// The message without surrounding blank lines or a ```json fence around the whole of it.
pub fn unwrap(message: &str) -> String {
    let lines: Vec<&str> = message.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(mut a), Some(mut b)) = (first, last) else {
        return String::new();
    };
    if b > a && matches!(lines[a].trim(), "```" | "```json") && lines[b].trim() == "```" {
        a += 1;
        b -= 1;
    }
    lines[a..=b].join("\n")
}

/// The message as one JSON object, or the problem.
pub fn parse(message: &str) -> Result<Map<String, Value>, String> {
    let body = unwrap(message);
    let mut values = serde_json::Deserializer::from_str(&body).into_iter::<Value>();
    let problem =
        || "The message isn't one JSON object: send only the object, alone or in a ```json fence.".to_string();
    let first = match values.next() {
        Some(Ok(Value::Object(map))) => map,
        _ => return Err(problem()),
    };
    if values.next().is_some() {
        return Err(problem());
    }
    Ok(first)
}

/// Every problem with the message, in order. None means it fits.
pub fn check(side: Side, message: &str) -> Vec<String> {
    let object = match parse(message) {
        Ok(o) => o,
        Err(p) => return vec![p],
    };
    let mut c = Checker { problems: Vec::new() };
    match side {
        Side::Request => c.request(&object),
        Side::Reply => c.reply(&object),
    }
    c.problems
}

fn text(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::String(s)) if !s.trim().is_empty())
}

fn texts(v: &Value) -> bool {
    matches!(v, Value::Array(a) if a.iter().all(|x| text(Some(x))))
}

fn non_empty_list(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Array(a)) if !a.is_empty())
}

/// Whether a change name is kebab-case.
pub fn change_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && !s.ends_with('-')
        && !s.contains("--")
}

struct Checker {
    problems: Vec<String>,
}

impl Checker {
    fn push(&mut self, p: impl Into<String>) {
        self.problems.push(p.into());
    }

    fn only(&mut self, o: &Map<String, Value>, allowed: &[&str], place: &str) {
        let mut keys: Vec<&String> = o.keys().filter(|k| !allowed.contains(&k.as_str())).collect();
        keys.sort();
        for k in keys {
            self.push(format!("{place}has an unknown field \"{k}\"."));
        }
    }

    fn need(&mut self, o: &Map<String, Value>, key: &str, place: &str) {
        if !text(o.get(key)) {
            self.push(format!("{place}needs \"{key}\", as text."));
        }
    }

    fn maybe(&mut self, o: &Map<String, Value>, key: &str, place: &str) {
        if o.contains_key(key) && !text(o.get(key)) {
            self.push(format!("{place}\"{key}\" must be text."));
        }
    }

    fn list(&mut self, o: &Map<String, Value>, key: &str) {
        if let Some(v) = o.get(key) {
            if !texts(v) {
                self.push(format!("\"{key}\" must be a list of text."));
            }
        }
    }

    fn items(&mut self, o: &Map<String, Value>, key: &str, required: &[&str], optional: &[&str]) {
        let Some(v) = o.get(key) else { return };
        let Value::Array(items) = v else {
            self.push(format!("\"{key}\" must be a list."));
            return;
        };
        let allowed: Vec<&str> = required.iter().chain(optional).copied().collect();
        for (i, item) in items.iter().enumerate() {
            let place = format!("\"{key}\"[{i}] ");
            let Value::Object(item) = item else {
                self.push(format!("\"{key}\"[{i}] must be an object."));
                continue;
            };
            self.only(item, &allowed, &place);
            for r in required {
                self.need(item, r, &place);
            }
            for opt in optional {
                self.maybe(item, opt, &place);
            }
        }
    }

    /// A change name, required or optional.
    fn change(&mut self, o: &Map<String, Value>, place: &str, required: bool) {
        match o.get("change") {
            None if !required => {}
            Some(Value::String(s)) if change_name(s) => {}
            Some(Value::String(s)) if !s.trim().is_empty() => self.push(format!(
                "\"change\" must be a change's kebab-case name, such as \"add-login\", not \"{s}\"."
            )),
            _ => self.push(format!("{place}needs \"change\", as text.")),
        }
    }

    fn request(&mut self, o: &Map<String, Value>) {
        match o.get("kind").and_then(Value::as_str) {
            Some("plan") => {
                let p = "A plan ";
                self.only(o, &["kind", "request", "why", "change", "constraints"], p);
                self.need(o, "request", p);
                self.maybe(o, "why", p);
                self.change(o, p, false);
                self.list(o, "constraints");
            }
            Some("ask") => {
                let p = "An ask ";
                self.only(o, &["kind", "question", "why", "change"], p);
                self.need(o, "question", p);
                self.maybe(o, "why", p);
                self.change(o, p, false);
            }
            Some("revise") => {
                let p = "A revise ";
                self.only(o, &["kind", "change", "feedback"], p);
                self.change(o, p, true);
                self.need(o, "feedback", p);
            }
            Some("progress") => {
                let p = "A progress report ";
                self.only(o, &["kind", "change", "done", "deviations", "blocked"], p);
                self.change(o, p, true);
                self.list(o, "done");
                self.items(o, "deviations", &["what", "why"], &[]);
                self.items(o, "blocked", &["task", "why"], &[]);
                if !["done", "deviations", "blocked"]
                    .iter()
                    .any(|k| non_empty_list(o.get(*k)))
                {
                    self.push("A progress report needs at least one non-empty list: done, deviations or blocked.");
                }
            }
            Some("archive") => {
                let p = "An archive ";
                self.only(o, &["kind", "change"], p);
                self.change(o, p, true);
            }
            Some("answers") => {
                self.only(o, &["kind", "answers"], "An answers request ");
                if !(o.get("answers").is_some_and(texts) && non_empty_list(o.get("answers"))) {
                    self.push("\"answers\" must be a non-empty list of text.");
                }
            }
            _ => self.push("\"kind\" must be \"plan\", \"ask\", \"revise\", \"progress\", \"archive\" or \"answers\"."),
        }
    }

    fn reply(&mut self, o: &Map<String, Value>) {
        let p = "The reply ";
        self.only(
            o,
            &[
                "status",
                "change",
                "summary",
                "answer",
                "artifacts",
                "written",
                "questions",
                "next",
                "reason",
            ],
            p,
        );
        self.change(o, p, false);
        for k in ["summary", "answer", "next", "reason"] {
            self.maybe(o, k, p);
        }
        match o.get("artifacts") {
            None => {}
            Some(Value::Object(a)) => {
                for (k, v) in a {
                    let ok = match v {
                        Value::String(s) => s.starts_with('/'),
                        Value::Array(list) => list.iter().all(|x| x.as_str().is_some_and(|s| s.starts_with('/'))),
                        _ => false,
                    };
                    if !ok {
                        self.push(format!(
                            "\"artifacts\".\"{k}\" must be an absolute path, or a list of them."
                        ));
                    }
                }
            }
            Some(_) => self.push("\"artifacts\" must be an object, from each artifact to its absolute path."),
        }
        self.items(o, "written", &["path", "change"], &[]);
        match o.get("questions") {
            Some(Value::Array(qs)) => {
                for (i, q) in qs.iter().enumerate() {
                    let place = format!("\"questions\"[{i}] ");
                    let Value::Object(q) = q else {
                        self.push(format!("\"questions\"[{i}] must be an object."));
                        continue;
                    };
                    self.only(q, &["q", "options"], &place);
                    self.need(q, "q", &place);
                    if q.get("options").is_some_and(|v| !texts(v)) {
                        self.push(format!("\"questions\"[{i}] \"options\" must be a list of text."));
                    }
                }
            }
            Some(_) => self.push("\"questions\" must be a list."),
            None => {}
        }
        match o.get("status").and_then(Value::as_str) {
            Some("completed") => {
                if !(text(o.get("summary"))
                    || text(o.get("answer"))
                    || non_empty_list(o.get("written"))
                    || text(o.get("reason")))
                {
                    self.push(
                        "A completed reply needs a summary, an answer, what was written, or a reason nothing was.",
                    );
                }
            }
            Some("input-required") => {
                if !non_empty_list(o.get("questions")) {
                    self.push("An input-required reply needs its questions.");
                }
            }
            Some("failed") => self.need(o, "reason", "A failed reply "),
            _ => self.push("\"status\" must be \"completed\", \"input-required\" or \"failed\"."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_and_bare_messages_parse() {
        assert!(
            check(
                Side::Request,
                "```json\n{\"kind\": \"archive\", \"change\": \"x\"}\n```\n"
            )
            .is_empty()
        );
        assert!(check(Side::Request, "  \n{\"kind\": \"ask\", \"question\": \"x\"}\n\n").is_empty());
    }

    #[test]
    fn two_objects_are_refused() {
        assert_eq!(
            check(
                Side::Request,
                r#"{"kind":"ask","question":"a"} {"kind":"ask","question":"b"}"#
            )
            .len(),
            1
        );
    }

    #[test]
    fn problems_come_in_field_order() {
        let p = check(
            Side::Request,
            r#"{"kind":"progress","change":"add-login","dun":[],"deviations":[{"what":"x"}]}"#,
        );
        assert_eq!(
            p,
            vec![
                "A progress report has an unknown field \"dun\".",
                "\"deviations\"[0] needs \"why\", as text.",
            ]
        );
    }

    #[test]
    fn change_names_are_kebab_case() {
        assert!(change_name("add-login"));
        assert!(change_name("v2-api"));
        assert!(!change_name("Add login"));
        assert!(!change_name("add--login"));
        assert!(!change_name("add-"));
        assert!(!change_name("../x"));
    }
}
