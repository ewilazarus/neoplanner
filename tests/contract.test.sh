#!/usr/bin/env bash
# Tests for `neoplanner contract`, and for the promises the plugin's files make about it.

set -eu
. "$(dirname "$0")/lib.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# verdict <request|reply> <message>: "ok", or the problems, one per line.
verdict() { printf '%s' "$2" | "$bin" contract check "$1" 2>&1 || true; }

echo "contract: requests"

check "a plan" ok "$(verdict request '{"kind": "plan", "request": "Magic-link sign-in", "constraints": ["No new dependencies"]}')"
check "a plan continuing a change, in a json fence" ok "$(verdict request '
```json
{"kind": "plan", "request": "Finish the tasks", "change": "add-magic-link"}
```
')"
check "an ask" ok "$(verdict request '{"kind": "ask", "question": "What expires sessions?", "why": "Changing it"}')"
check "a revise" ok "$(verdict request '{"kind": "revise", "change": "add-x", "feedback": "The mailer is sync"}')"
check "progress" ok "$(verdict request '{"kind": "progress", "change": "add-x", "done": ["1.1"], "blocked": [{"task": "2.1", "why": "No server"}]}')"
check "an archive" ok "$(verdict request '{"kind": "archive", "change": "add-x"}')"
check "answers" ok "$(verdict request '{"kind": "answers", "answers": ["Yes"]}')"
check "prose" "The message isn't one JSON object: send only the object, alone or in a \`\`\`json fence." \
  "$(verdict request 'Please plan magic links.')"
check "an unknown kind" '"kind" must be "plan", "ask", "revise", "progress", "archive" or "answers".' \
  "$(verdict request '{"kind": "record"}')"
check "a plan without a request" 'A plan needs "request", as text.' "$(verdict request '{"kind": "plan", "request": " "}')"
check "a change that isn't kebab-case" \
  '"change" must be a change'"'"'s kebab-case name, such as "add-login", not "Add Login".' \
  "$(verdict request '{"kind": "archive", "change": "Add Login"}')"
check "a revise without its change or feedback" '
A revise needs "change", as text.
A revise needs "feedback", as text.' \
  "$(printf '\n'; verdict request '{"kind": "revise"}')"
check "a misspelt field and a deviation without its why" '
A progress report has an unknown field "dun".
"deviations"[0] needs "why", as text.' \
  "$(printf '\n'; verdict request '{"kind": "progress", "change": "add-x", "dun": [], "deviations": [{"what": "x"}]}')"
check "an empty progress report" \
  "A progress report needs at least one non-empty list: done, deviations or blocked." \
  "$(verdict request '{"kind": "progress", "change": "add-x", "done": []}')"
check "done that isn't a list of text" '"done" must be a list of text.' \
  "$(verdict request '{"kind": "progress", "change": "add-x", "done": [1.1]}')"

echo "contract: replies"

check "a plan's reply" ok "$(verdict reply '{"status": "completed", "change": "add-x", "summary": "Planned.", "artifacts": {"tasks": "/s/tasks.md", "specs": ["/s/specs/a/spec.md"]}}')"
check "nothing to do" ok "$(verdict reply '{"status": "completed", "reason": "Already ticked."}')"
check "questions" ok "$(verdict reply '{"status": "input-required", "questions": [{"q": "Which?", "options": ["a", "b"]}]}')"
check "a relative artifact path" '"artifacts"."tasks" must be an absolute path, or a list of them.' \
  "$(verdict reply '{"status": "completed", "summary": "x", "artifacts": {"tasks": "tasks.md"}}')"
check "a completed reply with nothing in it" \
  "A completed reply needs a summary, an answer, what was written, or a reason nothing was." \
  "$(verdict reply '{"status": "completed", "change": "add-x"}')"
check "input-required without questions" "An input-required reply needs its questions." \
  "$(verdict reply '{"status": "input-required", "questions": []}')"
check "failed without a reason" 'A failed reply needs "reason", as text.' "$(verdict reply '{"status": "failed"}')"
check "an unknown status and field" '
The reply has an unknown field "extra".
"status" must be "completed", "input-required" or "failed".' \
  "$(printf '\n'; verdict reply '{"status": "done", "extra": 1}')"

echo "contract: the plugin's files keep to it"

# Every ```json block in the files that teach the contract must pass as a request or reply.
for file in agents/agent.md skills/contract/SKILL.md skills/apply/SKILL.md assets/CLAUDE-section.md; do
  awk -v out="$tmp/block" '
    /^[ \t]*```json[ \t]*$/ { n++; inside = 1; next }
    inside && /^[ \t]*```[ \t]*$/ { inside = 0; next }
    inside { sub(/^  /, ""); print > (out "." n) }' "$repo/$file"
  bad=""
  for block in "$tmp"/block.*; do
    [ -f "$block" ] || continue
    if [ "$(verdict request "$(cat "$block")")" != ok ] && [ "$(verdict reply "$(cat "$block")")" != ok ]; then
      bad="$bad $(basename "$block")"
    fi
  done
  rm -f "$tmp"/block.*
  check "every example in $file fits" "" "$bad"
done

# The wrapped OpenSpec commands run in the agent; only apply, which writes code, runs in
# the main conversation, and the main agent sees no skill but the contract.
forked=$(grep -l '^context: fork' "$repo"/skills/*/SKILL.md | wc -l | tr -d ' ')
in_agent=$(grep -l '^agent: neoplanner:agent' "$repo"/skills/*/SKILL.md | wc -l | tr -d ' ')
check "every forked command runs in the agent" "$forked" "$in_agent"
main=$(for f in "$repo"/skills/*/SKILL.md; do grep -q '^context: fork' "$f" || basename "$(dirname "$f")"; done | tr '\n' ' ')
check "only apply and the contract run in the main conversation" "apply contract " "$main"
check "the agent exists" "name: agent" "$(grep -m1 '^name:' "$repo/agents/agent.md")"
visible=$(for f in "$repo"/skills/*/SKILL.md; do grep -q '^disable-model-invocation: true' "$f" || basename "$(dirname "$f")"; done)
check "the main agent sees only the contract skill" contract "$visible"
check "every opsx command has its wrapper" "" \
  "$(for c in propose explore apply update sync archive; do [ -f "$repo/skills/$c/SKILL.md" ] || echo "$c"; done)"
check "no file still names neoarchivist" "" \
  "$(grep -rIl -i 'neoarchivist\|vault' "$repo/agents" "$repo/skills" "$repo/rules" "$repo/assets" "$repo/hooks" "$repo/bin" "$repo/src" "$repo/.claude-plugin" "$repo/bootstrap.sh" || true)"

finish
