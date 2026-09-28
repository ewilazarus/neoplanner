---
name: ask
description: Ask a question about this project's OpenSpec specs or changes: what the system is specified to do, what a change will alter, why a design chose something. neoplanner:agent looks it up and answers. Use when the user runs /neoplanner:ask.
argument-hint: "<question>"
disable-model-invocation: true
context: fork
agent: neoplanner:agent
background: false
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/bin/neoplanner *), Read, Grep, Glob
---

# Ask the specs

You're running this as neoplanner:agent, for the user's `/neoplanner:ask` command. Your
final message is for the user to read, in Markdown, not JSON. Where these steps say to ask
the user, end your reply with the numbered questions, and say they can answer in the
conversation for Claude to pass on; carry on when their answers arrive.

Answer the user's question under **Answer a question** in the planning rules. Reply in
Markdown: the answer, then the specs, requirements or changes it rests on, with their
paths, and any place the specs and the code disagree. Write nothing. If there is no
question, ask what they want to know.

The question: $ARGUMENTS
