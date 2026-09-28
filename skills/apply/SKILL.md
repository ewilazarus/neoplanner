---
name: apply
description: Implement an OpenSpec change's tasks in this conversation. Claude reads the tasks and design that neoplanner:agent wrote, writes the code, and reports progress to the agent in the background, which ticks the tasks.
argument-hint: "[change]"
disable-model-invocation: true
---

# Apply a change

This is OpenSpec's `/opsx:apply`, and the one neoplanner command that runs here, in the
main conversation: applying a change means writing code, and that's your job. The
planning stays with the `neoplanner:agent` agent. **You read the artifacts, but never
write them,** not even to tick a box. A hook refuses those writes. The agent ticks the
tasks when you report progress.

The change: $ARGUMENTS

1. **Pick the change.** Run `${CLAUDE_PLUGIN_ROOT}/bin/neoplanner status --json`. It lists
   the active changes with their task counts and the absolute path of each tasks file.
   Use the change named above. With none named, use the only active change, or the one
   this conversation has been about, or else ask the user which one. Say which change
   you're using.
2. **Load only what you need.** Read the change's `tasks.md`. Read `design.md` next to it
   when a task depends on a design choice, and a delta spec under `specs/` when a task
   implements a requirement whose exact behaviour matters. Leave the proposal unless the
   intent is unclear. Don't run OpenSpec commands to plan or validate. That's the agent's
   job, and keeping it there is what keeps your context free.
3. **Implement the unticked tasks in order.** Keep each change minimal and scoped to its
   task, and run the check the task names before calling it done.
4. **Report progress in the background** after each group of tasks, or sooner if you
   depart from the design or get blocked. Load the `neoplanner:contract` skill for the
   format, and send a `progress` request, with `done` holding the task numbers,
   `deviations` what you did differently and why, and `blocked` what stopped you. Don't
   wait for the reply before carrying on.
5. **Pause and ask the user** when a task is unclear, or when the implementation shows the
   design is wrong. If they agree the plan needs to change, send the agent a `revise` in
   the foreground, wait for it, then reread the tasks it names.
6. **When you stop,** say which tasks you finished this session, how many are left, and
   what's next. When all are done, suggest `/neoplanner:archive <change>`.
