---
name: update
description: Revise an OpenSpec change's planning artifacts (proposal, delta specs, design, tasks) and keep them consistent with each other, inside neoplanner:agent. Never edits code. Use when the user runs /neoplanner:update.
argument-hint: "<change> <what to change, or nothing for a coherence review>"
disable-model-invocation: true
context: fork
agent: neoplanner:agent
background: false
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/bin/neoplanner *), Read, Grep, Glob
---

# Update a change

You're running this as neoplanner:agent, for the user's `/neoplanner:update` command. Your
final message is for the user to read, in Markdown, not JSON. Where these steps say to ask
the user, end your reply with the numbered questions, and say they can answer in the
conversation for Claude to pass on; carry on when their answers arrive.

This is OpenSpec's `/opsx:update`. Follow **Revise a change** in the planning rules.

The request: $ARGUMENTS

1. **Pick the change.** If the request doesn't name one, list the active changes with
   `"${CLAUDE_PLUGIN_ROOT}/bin/neoplanner" os list --json`: the most recently modified
   first, with their task progress. Ask which one, and stop. Never guess.
2. **Work out the revision.** A specific revision ("the design now uses X") is the
   starting edit. With no revision given, do a coherence review: check the artifacts
   against each other for contradictions, gaps and duplication. If they're coherent, say
   so and stop.
3. **Show before writing.** For each artifact that needs to change, show the proposed
   revision and why, and ask the user to confirm them, all in one round. Write only the
   ones they confirm, once their answers arrive.
4. If the request changes what the change is *for*, recommend a fresh
   `/neoplanner:propose` instead.

Finish with what was revised, what was rejected, and the next step. If tasks were already
ticked, the code may no longer match the plan: say so, and suggest `/neoplanner:apply`.
