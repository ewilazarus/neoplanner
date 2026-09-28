---
name: sync
description: Fold an OpenSpec change's delta specs into the project's main specs without archiving the change, inside neoplanner:agent. Use when the user runs /neoplanner:sync.
argument-hint: "[change]"
disable-model-invocation: true
context: fork
agent: neoplanner:agent
background: false
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/bin/neoplanner *), Read, Grep, Glob
---

# Sync specs

You're running this as neoplanner:agent, for the user's `/neoplanner:sync` command. Your
final message is for the user to read, in Markdown, not JSON. Where these steps say to ask
the user, end your reply with the numbered questions, and say they can answer in the
conversation for Claude to pass on; carry on when their answers arrive.

This is OpenSpec's `/opsx:sync`. Follow **Sync delta specs** in the progress rules.

The change: $ARGUMENTS

If it isn't named, list the active changes that have delta specs, ask which one, and
stop. Never guess. Report per capability what was added, modified, removed or renamed. The
change stays active: `/neoplanner:archive` archives it once it's implemented.
