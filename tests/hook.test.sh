#!/usr/bin/env bash
# Tests for `neoplanner hook`: who reads and writes the artifacts, which commands run
# without a prompt, the contract, and the stop reminder, in both modes.

set -eu
. "$(dirname "$0")/lib.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tmp=$(cd "$tmp" && pwd -P)
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export XDG_CONFIG_HOME="$tmp/config" XDG_DATA_HOME="$tmp/data" FAKE_OPENSPEC_STATE="$tmp/openspec"
export CLAUDE_PLUGIN_DATA="$tmp/plugin-data" CLAUDE_PLUGIN_ROOT=$repo
launcher="$repo/bin/neoplanner"

# Two projects: one committed, one stealth.
committed="$tmp/committed" stealth="$tmp/stealth"
for p in "$committed" "$stealth"; do mkdir -p "$p/src" && git -C "$p" init -q; done
"$bin" --project "$committed" init apply --mode committed >/dev/null
"$bin" --project "$stealth" init apply --mode stealth >/dev/null
store="$tmp/data/neoplanner/stores/stealth"

session=s1 project=$committed
agent="" agent_type=""
as_main() { agent="" agent_type=""; }
as_planner() { agent=planner1 agent_type=neoplanner:agent; }
as_agent() { agent=$1 agent_type=$2; }

# decide <json>: feeds one hook event to the hook and prints its decision: "allow" or
# "deny" when it says so, "context" or "block" when it hands something back, and "quiet"
# when it prints nothing.
decide() {
  local out
  out=$(printf '%s' "$1" | CLAUDE_PROJECT_DIR=$project "$bin" hook)
  if [ -z "$out" ]; then
    echo quiet
  elif [ -n "$(jq -r '.hookSpecificOutput.permissionDecision // empty' <<<"$out")" ]; then
    jq -r .hookSpecificOutput.permissionDecision <<<"$out"
  elif [ "$(jq -r '.decision // empty' <<<"$out")" = block ]; then
    echo block
  elif [ -n "$(jq -r '.hookSpecificOutput.additionalContext // empty' <<<"$out")" ]; then
    echo context
  else
    echo "unexpected: $out"
  fi
}

# reason <json>: what the hook said, in full.
reason() {
  printf '%s' "$1" | CLAUDE_PROJECT_DIR=$project "$bin" hook |
    jq -r '.hookSpecificOutput.permissionDecisionReason // .hookSpecificOutput.additionalContext // .reason // empty'
}

# event <json fields>: common fields for this session and agent, merged with the given ones.
event() {
  jq -nc --arg s "$session" --arg a "$agent" --arg cwd "$project" --argjson extra "$1" \
    --arg t "$agent_type" \
    '{session_id: $s, cwd: $cwd} + (if $a == "" then {} else {agent_id: $a, agent_type: $t} end) + $extra'
}

tool() { event "$(jq -nc --arg t "$1" --arg p "$2" '{hook_event_name: "PreToolUse", tool_name: $t, tool_input: {file_path: $p, content: "x", old_string: "a", new_string: "b"}}')"; }
after() { event "$(jq -nc --arg p "$1" '{hook_event_name: "PostToolUse", tool_name: "Write", tool_input: {file_path: $p, content: "x"}}')"; }
bash_() { event "$(jq -nc --arg c "$1" '{hook_event_name: "PreToolUse", tool_name: "Bash", tool_input: {command: $c}}')"; }
send() { event "$(jq -nc --arg p "$1" '{hook_event_name: "PreToolUse", tool_name: "Agent", tool_input: {subagent_type: "neoplanner:agent", prompt: $p}}')"; }
message() { event "$(jq -nc --arg m "$1" '{hook_event_name: "PreToolUse", tool_name: "SendMessage", tool_input: {to: "planner1", message: $m}}')"; }
start() { event '{"hook_event_name": "SubagentStart"}'; }
stop_agent() { event "$(jq -nc --arg m "$1" '{hook_event_name: "SubagentStop", last_assistant_message: $m}')"; }
stop() { event '{"hook_event_name": "Stop"}'; }

echo "hook: committed, the artifacts"

tasks="$committed/openspec/changes/add-x/tasks.md"
check "the main agent reads them, with the usual permissions" quiet "$(decide "$(tool Read "$tasks")")"
check "but can't write them" deny "$(decide "$(tool Write "$tasks")")"
check "or edit them" deny "$(decide "$(tool Edit "$tasks")")"
check "and is sent to the agent" yes \
  "$(reason "$(tool Edit "$tasks")" | grep -q 'send it a `progress` request' && echo yes)"
check "its own code is its own business" quiet "$(decide "$(tool Write "$committed/src/main.rs")")"
as_agent sub1 general-purpose
check "another subagent can't write them either" deny "$(decide "$(tool Write "$tasks")")"
as_planner
check "the planner writes them" quiet "$(decide "$(tool Write "$tasks")")"
check "but never the code" deny "$(decide "$(tool Write "$committed/src/main.rs")")"
check "though it reads it" quiet "$(decide "$(tool Read "$committed/src/main.rs")")"

echo "hook: stealth, the store"

project=$stealth
as_main
tasks="$store/openspec/changes/add-x/tasks.md"
check "the main agent reads the store without a prompt" allow "$(decide "$(tool Read "$tasks")")"
check "and greps it" allow "$(decide "$(event "$(jq -nc --arg p "$store/openspec" '{hook_event_name: "PreToolUse", tool_name: "Grep", tool_input: {pattern: "x", path: $p}}')")")"
check "but can't write it" deny "$(decide "$(tool Write "$tasks")")"
check "files elsewhere are left alone" quiet "$(decide "$(tool Read "$tmp/elsewhere.md")")"
as_planner
check "the planner writes the store without a prompt" allow "$(decide "$(tool Write "$tasks")")"
check "but never the project" deny "$(decide "$(tool Edit "$stealth/src/main.rs")")"

echo "hook: commands"

project=$committed
as_main
check "the main agent's read-only neoplanner commands run" allow "$(decide "$(bash_ "$launcher status --json")")"
check "including os reads, by the bare name" allow "$(decide "$(bash_ "neoplanner os instructions apply --change add-x --json")")"
check "but not os writes" quiet "$(decide "$(bash_ "$launcher os archive add-x --yes")")"
check "or commits" quiet "$(decide "$(bash_ "$launcher commit Plan")")"
check "or git" quiet "$(decide "$(bash_ "git status")")"
as_planner
check "the planner's os writes run" allow "$(decide "$(bash_ "\"$launcher\" os new change \"add-x\"")")"
check "and its commits" allow "$(decide "$(bash_ "$launcher commit \"Plan add-x\"")")"
check "and read-only git" allow "$(decide "$(bash_ "git log --oneline -5")")"
check "and date" allow "$(decide "$(bash_ "date +%F")")"
check "but not the hook itself" quiet "$(decide "$(bash_ "$launcher hook")")"
check "nor anything combined" quiet "$(decide "$(bash_ "$launcher os archive add-x --yes && rm -rf /")")"
check "nor openspec called directly" quiet "$(decide "$(bash_ "openspec archive add-x --yes")")"
check "nor git that writes" quiet "$(decide "$(bash_ "git commit -m x")")"

echo "hook: the contract"

session=s2
as_main
check "a request that fits goes through" quiet \
  "$(decide "$(send '{"kind": "plan", "request": "Magic-link sign-in"}')")"
check "one that doesn't is refused" deny "$(decide "$(send '{"kind": "plan"}')")"
check "saying what's missing" yes \
  "$(reason "$(send '{"kind": "plan"}')" | grep -q 'A plan needs "request", as text.' && echo yes)"
check "another agent's prompt is none of its business" quiet \
  "$(decide "$(event '{"hook_event_name": "PreToolUse", "tool_name": "Agent", "tool_input": {"subagent_type": "Explore", "prompt": "hi"}}')")"
check "answers by SendMessage are checked" deny "$(decide "$(message '{"kind": "answers", "answers": []}')")"
check "a chatty SendMessage isn't" quiet "$(decide "$(message 'Thanks, carry on.')")"

as_planner
check "the planner, sent a request, gets its rules" context "$(decide "$(start)")"
check "which name this project's openspec/" yes \
  "$(reason "$(start)" | grep -qF "$committed/openspec" && echo yes)"
session=s3
check "for a slash command, it's told to reply in Markdown" yes \
  "$(reason "$(start)" | grep -q "the user's slash command" && echo yes)"
session=s2
as_agent planner2 neoplanner:agent
decide "$(send '{"kind": "ask", "question": "What expires sessions?"}')" >/dev/null
decide "$(start)" >/dev/null
check "a reply that fits ends the task" quiet \
  "$(decide "$(stop_agent '{"status": "completed", "answer": "Inactivity."}')")"
check "one that doesn't is sent back" block "$(decide "$(stop_agent 'Sessions expire on inactivity.')")"
as_agent planner9 neoplanner:agent
check "a slash command's Markdown reply isn't checked" quiet "$(decide "$(stop_agent '# Status')")"

echo "hook: the stop reminder"

session=s4
as_main
check "code changed with no change in flight: no reminder" "quiet|quiet" \
  "$(decide "$(after "$committed/src/main.rs")")|$(decide "$(stop)")"
decide "$(send '{"kind": "plan", "request": "Magic-link sign-in"}')" >/dev/null
decide "$(after "$committed/src/main.rs")" >/dev/null
check "code changed while a change is in flight: remind" block "$(decide "$(stop)")"
check "only once" quiet "$(decide "$(stop)")"
decide "$(after "$committed/src/main.rs")" >/dev/null
decide "$(send '{"kind": "progress", "change": "add-x", "done": ["1.1"]}')" >/dev/null
check "a progress report covers it" quiet "$(decide "$(stop)")"
decide "$(after "$committed/src/main.rs")" >/dev/null
decide "$(send '{"kind": "archive", "change": "add-x"}')" >/dev/null
decide "$(after "$committed/src/main.rs")" >/dev/null
check "and after the archive, nothing is in flight" quiet "$(decide "$(stop)")"
mkdir -p "$XDG_CONFIG_HOME/neoplanner"
printf 'stop_reminder = false\n' >"$XDG_CONFIG_HOME/neoplanner/config.toml"
session=s5
decide "$(send '{"kind": "plan", "request": "x"}')" >/dev/null
decide "$(after "$committed/src/main.rs")" >/dev/null
check "stop_reminder = false turns it off" quiet "$(decide "$(stop)")"
rm "$XDG_CONFIG_HOME/neoplanner/config.toml"

echo "hook: validating delta specs"

as_planner
spec="$committed/openspec/changes/add-x/specs/auth/spec.md"
check "a valid delta spec says nothing" quiet "$(decide "$(after "$spec")")"
export FAKE_OPENSPEC_VALIDATE='{"items": [{"id": "add-x", "valid": false, "issues": [{"level": "ERROR", "path": "auth", "message": "Requirement needs a scenario"}, {"level": "WARNING", "path": "auth", "message": "Short"}]}]}'
check "an invalid one hands back its errors" "\`openspec validate add-x\` found errors after this write. Fix them now, while the spec is in hand:

- auth: Requirement needs a scenario" "$(reason "$(after "$spec")")"
check "other artifacts aren't validated" quiet "$(decide "$(after "$committed/openspec/changes/add-x/design.md")")"
project=$stealth
check "in the store too" context "$(decide "$(after "$store/openspec/changes/add-x/specs/auth/spec.md")")"
unset FAKE_OPENSPEC_VALIDATE

echo "hook: a project that isn't set up"

project="$tmp/bare"
mkdir -p "$project"
as_main
check "leaves everything alone" quiet "$(decide "$(tool Write "$project/openspec/changes/x/tasks.md")")"
check "and fails open on a broken config" "exit 1" \
  "$(mkdir -p "$project/.neoplanner" && printf 'mode = "sideways"\n' >"$project/.neoplanner/config.toml"
     printf '%s' "$(tool Read "$project/x")" | CLAUDE_PROJECT_DIR=$project "$bin" hook >/dev/null 2>&1 && echo "exit 0" || echo "exit $?")"

finish
