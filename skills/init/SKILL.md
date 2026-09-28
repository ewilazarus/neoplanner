---
name: init
description: Set up a project's OpenSpec planning, kept by the neoplanner:agent agent. Committed mode keeps the specs in the repo's openspec/ folder; stealth mode keeps them in an OpenSpec store outside the repo and changes no tracked file. Also writes the project's OpenSpec context with the user. Run once per project, and again after updating neoplanner.
argument-hint: "[committed|stealth]"
disable-model-invocation: true
context: fork
agent: neoplanner:agent
background: false
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/bin/neoplanner *), Read, Grep, Glob
---

# Set up OpenSpec planning

You're running this as neoplanner:agent, for the user's `/neoplanner:init` command. Your
final message is for the user to read, in Markdown, not JSON. Where these steps say to ask
the user, end your reply with the numbered questions, and say they can answer in the
conversation for Claude to pass on; carry on when their answers arrive. **Ask everything
in one round:** the mode and the choices from step 2, and the context questions from step
4, together, so the user answers once.

The user asked for: $ARGUMENTS

neoplanner makes every mechanical change: the settings, the OpenSpec folder or store, the
`CLAUDE.md` section, and the git excludes. It only adds what is missing, keeps any value
already set differently and says so, and changes nothing on a second run. Don't make those
edits by hand.

```bash
${CLAUDE_PLUGIN_ROOT}/bin/neoplanner init plan  [--mode committed|stealth] [--store-id ID] [--store-path PATH] [--agents-md import|append]
${CLAUDE_PLUGIN_ROOT}/bin/neoplanner init apply [the same]
```

Run it from the project root.

## 1. Survey what exists

```bash
${CLAUDE_PLUGIN_ROOT}/bin/neoplanner config
${CLAUDE_PLUGIN_ROOT}/bin/neoplanner init plan
git rev-parse --is-inside-work-tree
```

If the project is already set up, `config` says in which mode, and `init plan` shows only
what's missing: skip the mode question. Otherwise, run `init plan` once for each mode, so
the user can compare. Glob for an existing `openspec/` folder, and for `.claude/skills/openspec-*` or
`.claude/commands/opsx/`. Skim the repository's README and build files, to draft the
context in step 4.

## 2. Show the plan and settle its choices

Explain the two modes in plain words, and recommend one:

- **committed**: the specs live in `openspec/`, in the repository. Everyone on the
  project shares them, reviews them in pull requests, and gets the plugin offered through
  `.claude/settings.json`. `CLAUDE.md` gets a short section. This is OpenSpec's usual
  setup, and the one to recommend for a team that has agreed to use it.
- **stealth**: the specs live in an OpenSpec store, a standalone OpenSpec repository
  outside the project (by default `~/.local/share/neoplanner/stores/<id>`), with its own
  git history, which neoplanner commits to as it plans. Nothing tracked in the project
  changes: the section goes in `CLAUDE.local.md` and the plugin in
  `.claude/settings.local.json`, both kept out of git by this clone's `.git/info/exclude`.
  It suits planning your own work in a repository whose team doesn't use OpenSpec. Every
  worktree of the clone shares the store, but another clone or machine needs the store
  copied or cloned there and `/neoplanner:init` run again.

Show each plan's output, and explain its lines:

- **`openspec/: new`** comes from `openspec init --tools none`. No tool's own commands or
  skills come with it, because the `/neoplanner:*` commands replace OpenSpec's `/opsx:*`
  ones. They run the workflow here, in the agent, so the main conversation keeps its
  context for code.
- **`store <id>: new`** comes from `openspec store setup`. `kept, already registered`
  means the store already exists, and this project will share it. Offer `--store-id` if
  the user wants a different id, or `--store-path` for somewhere else.
- **The `CLAUDE.md` (or `CLAUDE.local.md`) section** tells the main Claude that the specs
  are yours to write and its to read, and when to send you requests. **Don't add rules to
  it.** The project's context goes in `config.yaml`, in step 4.
- **`CLAUDE.md: needs a choice`** means the project has `AGENTS.md` and no `CLAUDE.md`.
  Recommend `--agents-md import`.
- **`note: the project has OpenSpec's own /opsx commands`**: they run the workflow in the
  main conversation, and the hooks refuse their writes to the artifacts. Suggest removing
  them. Don't remove them yourself.

## 3. Apply

Run `init apply` with the mode and flags chosen in step 2, and show its output. Then run
`${CLAUDE_PLUGIN_ROOT}/bin/neoplanner doctor`, and pass on anything it reports as FAIL.

## 4. Write the project's context with the user

`openspec/config.yaml`, which `neoplanner root` locates, carries a `context:` that
OpenSpec hands you with every artifact's instructions, and optional per-artifact `rules:`.
Draft the context from the survey: the stack, how the code is laid out, how it's tested,
and the conventions that bind changes. Ask the user to correct or add to it, in the same
round as step 2. Keep it short, because it reaches every artifact. Leave out whatever they
have no answer for yet.

Read `config.yaml` first, and edit it, keeping its `schema:` line. If it already has a
`context:`, offer only additions. In stealth mode, commit the store afterwards with
`${CLAUDE_PLUGIN_ROOT}/bin/neoplanner commit "Set up the project's context"`.

## 5. Hand over

Tell the user how it works from now on:

- **They drive planning** with the `/neoplanner:*` commands, which run in you:
  `propose <idea>`, `explore [topic]`, `update <change> <feedback>`, `sync <change>`,
  `archive <change>`, `status` and `ask <question>`.
- **`/neoplanner:apply [change]`** runs in the main conversation, because it writes code.
  Claude reads the tasks and design, implements them, and reports progress to you in the
  background. You tick the tasks.
- **Claude can also plan on its own,** through the contract, when substantial work comes
  up without a change.

In committed mode, remind them to commit `openspec/`, `.neoplanner/`, `.claude/settings.json`
and `CLAUDE.md`, so teammates are offered the plugin in their next session. Summarise what
was created, what was already there, and what was skipped.
