# neoplanner

A Claude Code plugin that runs [OpenSpec](https://github.com/Fission-AI/OpenSpec)'s
spec-driven workflow behind one agent, `neoplanner:agent`. The agent writes the proposals,
delta specs, designs and tasks, validates and archives them, and ticks the tasks as work
lands. The main Claude reads those artifacts and writes the code. The OpenSpec workflow
(its instructions and templates, the repository survey, validation, merging specs) runs in
the agent's context, so the main conversation's context stays free for the code.

You choose where the specs live when you set up a project:

- **committed**: OpenSpec's usual `openspec/` folder, in the repository. Everyone shares
  the specs and reviews them in pull requests.
- **stealth**: an [OpenSpec store](https://github.com/Fission-AI/OpenSpec), a standalone
  OpenSpec repository outside the project with its own git history. No tracked file in the
  project changes. This suits planning your own work in a repository whose team doesn't
  use OpenSpec.

## Install

With the `claude` CLI, git and jq on your PATH, and OpenSpec 1.6 or later
(`npm install -g @fission-ai/openspec@latest`):

```bash
curl -fsSL https://raw.githubusercontent.com/ewilazarus/neoplanner/main/bootstrap.sh | bash
```

It adds the marketplace, installs neoplanner for you in every project, and puts
`neoplanner` in `~/.local/bin`. Run it again to update. To declare the plugin in a
project's `.claude/settings.json` instead, so everyone who opens the project is offered
it, add `-s -- --project` and run it from the project's root. `bash bootstrap.sh --dry-run`
prints the commands without running them.

Then restart Claude Code and run `/neoplanner:init` in the project.

The plugin runs a small binary, the `neoplanner` CLI. The first time it's needed, the
plugin downloads the release matching its own version for your platform (macOS or Linux,
on arm64 or x86-64), checks it against its published sha256, and caches it in
`~/.cache/neoplanner/`.

## Commands

You drive the planning. OpenSpec's `/opsx:*` commands are wrapped so they run **inside
the agent**: their reading and writing stay there, and only the result comes back to the
conversation. When the agent needs your input, it ends with numbered questions, you answer
in the conversation, and Claude passes your answers on.

| OpenSpec | neoplanner | Runs in | What it does |
|---|---|---|---|
| `/opsx:propose` | `/neoplanner:propose <idea>` | the agent | Creates a change and every artifact it needs before implementation, then gives their paths |
| `/opsx:explore` | `/neoplanner:explore [topic]` | the agent | Thinks a problem through with you, grounded in the code and the specs. Writes nothing unless you ask |
| `/opsx:update` | `/neoplanner:update <change> <feedback>` | the agent | Revises a change's artifacts and keeps them consistent, showing each revision first |
| `/opsx:sync` | `/neoplanner:sync <change>` | the agent | Folds a change's delta specs into the main specs |
| `/opsx:archive` | `/neoplanner:archive <change>` | the agent | Validates, merges the specs, archives, and in stealth mode commits the store |
| `/opsx:apply` | `/neoplanner:apply [change]` | **the conversation** | Claude reads the tasks and design, writes the code, and reports progress to the agent |
| | `/neoplanner:status` | the agent | The active changes, their task progress, and what to pick up next |
| | `/neoplanner:ask <question>` | the agent | What the specs say, with the requirement or change the answer rests on |
| | `/neoplanner:init` | the agent | Sets up a project in either mode, and writes its OpenSpec context with you |

`apply` is the one exception, because applying means writing code, and that's the main
Claude's job. It stays lean: it reads only `tasks.md`, plus the design or a spec when a
task needs them, and never runs the OpenSpec workflow itself. Init installs OpenSpec with
`--tools none`, so the project doesn't get OpenSpec's own `/opsx:*` files, which would run
the workflow in the main conversation.

## The agent

Claude also talks to `neoplanner:agent` on its own, through a JSON contract. It learns
the contract from the `neoplanner:contract` skill, the only neoplanner skill it sees
unprompted, and from a short section in the project's `CLAUDE.md` (or `CLAUDE.local.md`,
in stealth mode).

- **`plan`**, in the background, before substantial work that no change covers yet. The
  reply gives the absolute path of each artifact, and Claude reads `tasks.md` from there.
- **`progress`**, in the background, as Claude finishes tasks (`done: ["1.1", "1.2"]`),
  departs from the design (`deviations`), or gets blocked (`blocked`). The agent ticks the
  boxes, and records deviations in the design or specs.
- **`revise`**, when implementation shows the plan wrong. **`ask`**, to look something up.
  **`archive`**, once the work is done.
- **Questions go through Claude.** A subagent can't talk to you, so when the agent needs
  an answer, its reply says `input-required`. Claude asks you, then resumes the same agent
  with your answers.

A hook refuses a request that doesn't fit the contract, saying exactly what's missing, and
sends back a reply that doesn't fit. The reply states follow the A2A task states:
`completed`, `input-required`, `failed`. The agent's rules are in `rules/`, compiled into
the binary, and handed to the agent as it starts, with the project's OpenSpec folder filled
in. Its side of the contract is in `agents/agent.md`.

## Hooks

- **Everyone reads the artifacts; only the agent writes them.** The main Claude's writes
  to the OpenSpec folder are refused and pointed at a `progress` or `revise` request. In
  stealth mode, reads of the store are approved without a prompt, even though it's outside
  the project.
- **The agent never writes code.** Its writes outside the OpenSpec folder are refused.
- **The agent starts with its rules,** and each request and reply is checked against the
  contract.
- **Commands without prompts.** The read-only neoplanner commands (`status`, `root`,
  `config`, `doctor`, and `os list|show|status|validate|instructions|context`) run for
  anyone. The agent also runs every neoplanner command, read-only git, and `date`, each on
  its own. Anything combined with `&&`, a pipe, a substitution or a redirection still asks
  you.
- **Each delta spec the agent writes is validated,** with `openspec validate`, and any
  errors go back to it while the spec is still in hand.
- **A reminder to report progress.** If a session planned or reported on a change, then
  changed project files, and hasn't reported progress since, Claude is asked once, as it
  stops, whether any of it finished tasks. `stop_reminder = false` in your own settings
  turns it off.

If the binary is missing or anything goes wrong, the hooks let the action through and
report a hook error, so they never lock you out of your own files.

## Settings

- **Committed:** `.neoplanner/config.toml`, at the project root and committed, holds
  `mode = "committed"`. The specs are in `openspec/`.
- **Stealth:** nothing in the project. `$XDG_CONFIG_HOME/neoplanner/projects.toml` (or
  `~/.config/neoplanner/`) maps each project to its store:

  ```toml
  [projects."/Users/me/work/app"]
  store = "app"
  path = "/Users/me/.local/share/neoplanner/stores/app"
  ```

  A project is keyed by its main working tree, so every worktree of a clone shares one
  store. New stores go in `$XDG_DATA_HOME/neoplanner/stores/<id>` unless you choose
  another path, and are registered with OpenSpec, so `openspec … --store <id>` works on
  them too. The section Claude reads goes in `CLAUDE.local.md`, and the plugin in
  `.claude/settings.local.json`. Both are kept out of git by the clone's
  `.git/info/exclude`. To plan from another clone or machine, clone the store there,
  `openspec store register` it, and run `/neoplanner:init` in that clone.
- **Yours:** `$XDG_CONFIG_HOME/neoplanner/config.toml` holds `stop_reminder = false` if
  you want it.

`neoplanner config` shows the settings in effect, `neoplanner root` where the specs are,
and `neoplanner doctor` checks OpenSpec's version and the store's registration.
`neoplanner os <args>` runs any OpenSpec command on the project's specs, adding `--store`
in stealth mode. Switching a project between modes isn't supported yet.

## Development

The toolchain comes from [devbox](https://www.jetify.com/devbox):

```bash
devbox run build   # cargo build --release
devbox run test    # the unit tests, then tests/run.sh
devbox run lint    # cargo fmt --check, and clippy
```

`tests/run.sh` builds the debug binary and runs the bash suite, which specifies every
command's output against a fake `openspec` (`tests/fake-openspec`), so it needs no Node.
On macOS, `/bin/bash tests/run.sh` also checks bash 3.2. To try the plugin from a checkout,
build it and run `claude --plugin-dir .`: the launcher uses the local build instead of
downloading a release.

A release is a version tag, `vX.Y.Z`, matching `Cargo.toml` and both manifests in
`.claude-plugin/`. `tests/versions.test.sh` checks that they agree. The release workflow
builds and publishes the binaries the launcher downloads.

## License

MIT. It wraps [Fission-AI/OpenSpec](https://github.com/Fission-AI/OpenSpec), whose
workflow, and whose `/opsx:*` skills, the agent's rules and commands are adapted from.
