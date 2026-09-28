The planning rules cover where the specs are, the wrapper, and what you may touch. These
rules don't repeat them.

## Record progress (`progress`)

The main agent implements the tasks, and tells you what happened. It can't write the
artifacts, so the tasks file only moves through you.

1. `"{{bin}}" os status --change "<name>" --json` for the tasks file's path, then read it.
2. **Tick each task in `done`:** `- [ ] 1.1 …` → `- [x] 1.1 …`. Match tasks by number. If
   an id isn't in the file, say so in `next` rather than guessing which task was meant.
   Change the box, and nothing else on that line.
3. **Record each deviation** where it belongs. A change to *how*, such as a different
   library or data shape, goes in `design.md`, stated as the design now stands, with its
   reason. A change to *what* the system does goes in the delta spec. If it adds work, add
   a task to `tasks.md`.
4. **Note each blocked task** under it, as an indented bullet that says why:
   `  - Blocked: no staging mail server to test against`. Leave its box unticked.
5. Validate the change if you touched a spec, and commit the store in stealth mode:
   `"{{bin}}" commit "<name>: progress"`.
6. Reply with what you wrote, and in `next`, the next unticked task and how many are
   left. When every task is ticked, say the change is ready to archive.

## Sync delta specs (`/neoplanner:sync`)

Fold a change's delta specs into the main specs, `{{openspec}}/specs/<capability>/spec.md`,
without archiving it. This is agent-driven, so merge with judgement:

- **ADDED:** add the requirement. If it already exists, update it to match.
- **MODIFIED:** update the requirement in place. Keep any scenarios the delta doesn't
  mention.
- **REMOVED:** remove the whole requirement block.
- **RENAMED:** rename `FROM:` to `TO:`.
- A capability with no main spec yet gets a new file with a short `## Purpose` and a
  `## Requirements` section.

Read both sides first. Running it twice must give the same result. Commit the store in
stealth mode, and summarise per capability what was added, modified, removed or renamed.

## Archive a change (`archive`, `/neoplanner:archive`)

1. `"{{bin}}" os status --change "<name>" --json`, and read the tasks file.
   - For a user's `/neoplanner:archive`: if any artifact isn't `done`, or any task is
     unticked, list them and ask whether to archive anyway. Don't archive until they say
     yes.
   - For an `archive` request from the main agent: if tasks are unticked, reply
     `input-required` with the question.
2. Validate: `"{{bin}}" os validate "<name>" --strict`. Fix what's wrong in the artifacts
   first. If the fix would need code changes, stop, and say so.
3. Archive. This folds the delta specs into the main specs and moves the change to
   `changes/archive/YYYY-MM-DD-<name>/`:

   ```bash
   "{{bin}}" os archive "<name>" --yes
   ```

   Add `--skip-specs` only for a change with no delta specs, such as tooling or docs work.
4. In stealth mode, commit the store: `"{{bin}}" commit "Archive <name>"`. In committed
   mode, the archive is part of the main agent's next commit in the project.
5. Report where it was archived and which capabilities' specs changed.

## If the project isn't set up

If `"{{bin}}" root` fails, reply `failed`, with the reason that `/neoplanner:init` sets
the project up. Don't set it up yourself.
