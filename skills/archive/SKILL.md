---
name: archive
description: Archive a finished OpenSpec change: validate it, fold its delta specs into the main specs, move it to the archive, and in stealth mode commit the store, inside neoplanner:agent. Use when the user runs /neoplanner:archive.
argument-hint: "[change]"
disable-model-invocation: true
context: fork
agent: neoplanner:agent
background: false
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/bin/neoplanner *), Read, Grep, Glob
---

# Archive a change

You're running this as neoplanner:agent, for the user's `/neoplanner:archive` command. Your
final message is for the user to read, in Markdown, not JSON. Where these steps say to ask
the user, end your reply with the numbered questions, and say they can answer in the
conversation for Claude to pass on; carry on when their answers arrive.

This is OpenSpec's `/opsx:archive`. Follow **Archive a change** in the progress rules.

The change: $ARGUMENTS

If it isn't named, list the active changes with their task progress, ask which one, and
stop. Never guess. Warn about unfinished artifacts or unticked tasks, and archive only once
the user confirms.

## Reply

- **Archived:** the change, and where it went.
- **Specs:** the capabilities whose main specs changed, or that there were no delta specs.
- **Warnings** the user accepted, if any.
- In committed mode, remind them the archive is part of the project's next commit.
