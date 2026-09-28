#!/usr/bin/env bash
# Tests for `neoplanner os`, `status`, `commit`, `root` and `doctor`, against the fake
# OpenSpec CLI, in both modes.

set -eu
. "$(dirname "$0")/lib.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tmp=$(cd "$tmp" && pwd -P)
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export XDG_CONFIG_HOME="$tmp/config" XDG_DATA_HOME="$tmp/data" FAKE_OPENSPEC_STATE="$tmp/openspec"
export FAKE_OPENSPEC_LOG="$tmp/log"
unset CLAUDE_PROJECT_DIR

committed="$tmp/committed" stealth="$tmp/stealth" bare="$tmp/bare"
for p in "$committed" "$stealth" "$bare"; do mkdir -p "$p" && git -C "$p" init -q; done
"$bin" --project "$committed" init apply --mode committed >/dev/null
"$bin" --project "$stealth" init apply --mode stealth >/dev/null
store="$tmp/data/neoplanner/stores/stealth"

# np <project> <args>: runs neoplanner in the project, and prints its output and any error.
np() { local p=$1; shift; (cd "$p" && "$bin" --project "$p" "$@" 2>&1 | sed 's/^neoplanner: //') || true; }
# called <project> <args>: what `os <args>` asked OpenSpec for.
called() { : >"$FAKE_OPENSPEC_LOG"; np "$@" >/dev/null; cat "$FAKE_OPENSPEC_LOG"; }

echo "os: the store flag"

check "committed mode passes the arguments as they are" "status --change add-x --json" \
  "$(called "$committed" os status --change add-x --json)"
check "stealth mode adds the store where it's taken" "status --change add-x --json --store stealth" \
  "$(called "$stealth" os status --change add-x --json)"
check "to new change too" "new change add-x --store stealth" "$(called "$stealth" os new change add-x)"
check "but not to commands without it" "schemas" "$(called "$stealth" os schemas)"
check "the exit status comes through" "exit 1" \
  "$(cd "$stealth" && FAKE_OPENSPEC_VALIDATE='{}' "$bin" --project "$stealth" os validate add-x >/dev/null 2>&1 && echo "exit 0" || echo "exit $?")"
check "a project that isn't set up is told how" \
  "This project isn't set up for neoplanner yet: run /neoplanner:init." "$(np "$bare" os list)"

echo "os: root and status"

check "root, committed" "$committed/openspec" "$(np "$committed" root)"
check "root, stealth" "$store/openspec" "$(np "$stealth" root)"
check "no changes" "OpenSpec (committed): $committed/openspec

No active changes." "$(np "$committed" status)"
export FAKE_OPENSPEC_LIST='{"changes": [{"name": "add-x", "completedTasks": 2, "totalTasks": 5, "lastModified": "2026-09-28T18:44:16.864Z", "status": "in-progress"}, {"name": "fix-y", "completedTasks": 0, "totalTasks": 0, "lastModified": "2026-09-27T10:00:00Z", "status": "no-tasks"}]}'
check "a table of changes" "OpenSpec (stealth): $store/openspec

| # | Change | Tasks | Status | Modified |
|---|---|---|---|---|
| 1 | add-x | 2/5 | in-progress | 2026-09-28 |
| 2 | fix-y | 0/0 | no-tasks | 2026-09-27 |

2 active, 3 open tasks." "$(np "$stealth" status)"
check "and JSON, with each change's paths" "stealth|$store/openspec/changes/add-x/tasks.md|2" \
  "$(np "$stealth" status --json | jq -r '[.mode, .changes[0].tasks, .changes[0].tasks_done] | join("|")')"
check "which asks the store" "list --json --store stealth" "$(called "$stealth" status)"
unset FAKE_OPENSPEC_LIST

echo "os: commit"

check "committed mode has nothing to commit" \
  "Committed mode: the project's own git holds the specs, so there's nothing to commit here." \
  "$(np "$committed" commit Plan add-x)"
check "stealth mode with a clean store neither" "Nothing to commit: the store is up to date." \
  "$(np "$stealth" commit Plan add-x)"
mkdir -p "$store/openspec/changes/add-x" && printf '# Proposal\n' >"$store/openspec/changes/add-x/proposal.md"
np "$stealth" commit Plan add-x >/dev/null
check "commits the store's changes" "Plan add-x" "$(git -C "$store" log -1 --format=%s)"
check "and leaves the project alone" "" "$(git -C "$stealth" status --porcelain)"

echo "os: doctor"

check "a healthy stealth project" "\
ok    OpenSpec 1.6.0
ok    stealth specs at $store/openspec
ok    store stealth is registered with OpenSpec" "$(np "$stealth" doctor)"
check "a project that isn't set up" "FAIL  not set up: run /neoplanner:init" "$(np "$bare" doctor | sed -n 2p)"

finish
