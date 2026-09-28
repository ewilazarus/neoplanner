---
name: propose
description: Propose a new OpenSpec change and write everything it needs before implementation (proposal, delta specs, design and tasks) inside neoplanner:agent, so the main conversation's context stays free. Use when the user runs /neoplanner:propose.
argument-hint: "<what to build, or a change name>"
disable-model-invocation: true
context: fork
agent: neoplanner:agent
background: false
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/bin/neoplanner *), Read, Grep, Glob
---

# Propose a change

You're running this as neoplanner:agent, for the user's `/neoplanner:propose` command. Your
final message is for the user to read, in Markdown, not JSON. Where these steps say to ask
the user, end your reply with the numbered questions, and say they can answer in the
conversation for Claude to pass on; carry on when their answers arrive.

This is OpenSpec's `/opsx:propose`, run here instead of in the main conversation. Follow
**Plan a change** in the planning rules, from picking the name to validating and, in
stealth mode, committing the store.

What the user wants: $ARGUMENTS

If that's empty or too vague to name a change, ask what they want to build or fix, and
stop. Don't invent a change.

Prefer reasonable decisions to questions, so the plan keeps moving, and state the ones you
made in the design. Ask only about what would change what gets built: batch those
questions together, after writing what you can.

## Reply

- **The change,** by name, in a sentence or two: what it does and why.
- **Its artifacts,** as a table: each artifact, its absolute path, and a line on what it
  holds. The main agent reads them from these paths.
- **The tasks,** as their numbered groups with a count per group. Don't paste the whole
  list.
- **Decisions you made** on the user's behalf, if any.
- **Next:** `/neoplanner:apply <name>` to implement it, or `/neoplanner:update <name>
  <feedback>` to change the plan first.
