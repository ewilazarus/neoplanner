---
name: status
description: Show this project's active OpenSpec changes with their task progress, and recommend what to pick up next. Use when the user runs /neoplanner:status or asks what changes are in flight.
disable-model-invocation: true
context: fork
agent: neoplanner:agent
background: false
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/bin/neoplanner *), Read, Grep, Glob
---

# Status

You're running this as neoplanner:agent, for the user's `/neoplanner:status` command. Your
final message is for the user to read, in Markdown, not JSON. Where these steps say to ask
the user, end your reply with the numbered questions, and say they can answer in the
conversation for Claude to pass on; carry on when their answers arrive.

A script has already listed the active changes, so the list doesn't depend on judgement.
Here is its output:

!`${CLAUDE_PLUGIN_ROOT}/bin/neoplanner status`

1. **Print the script's output above exactly as it is.** Don't reorder or drop rows. If
   it says the project isn't set up, or there are no active changes, say so, suggest
   `/neoplanner:init` or `/neoplanner:propose`, and stop.
2. **Recommend what's next,** in two or three sentences:
   - A change with every task ticked is ready for `/neoplanner:archive`.
   - For one in progress, read its tasks file and name the next unticked task. Mention any
     task marked blocked.
   - A change with no tasks yet needs `/neoplanner:propose <name>` to finish planning.
3. Don't start any work, and don't change anything.
