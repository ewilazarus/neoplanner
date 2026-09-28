#!/usr/bin/env bash
# Tests for bootstrap.sh, against a fake `claude` CLI that logs what it's asked to do.

set -eu
. "$(dirname "$0")/lib.sh"

bootstrap="$repo/bootstrap.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
# The script writes ~/.local/bin/neoplanner, so give it a home of its own.
export HOME="$tmp/home"
mkdir -p "$HOME"

mkdir -p "$tmp/bin" "$tmp/nojq" "$tmp/project"
git -C "$tmp/project" init -q
project=$(cd "$tmp/project" && pwd -P)

# The fake CLI answers the JSON listings from $FAKE_MARKETPLACES and $FAKE_PLUGINS, and logs
# every other command.
cat >"$tmp/bin/claude" <<'EOF'
#!/bin/sh
case "$*" in
  "plugin marketplace list --json") printf '%s\n' "$FAKE_MARKETPLACES" ;;
  "plugin list --json") printf '%s\n' "$FAKE_PLUGINS" ;;
  *) echo "$*" >>"$FAKE_LOG" ;;
esac
EOF
chmod +x "$tmp/bin/claude"
# Without jq: the same fake claude and git, and nothing else from outside /usr/bin and /bin.
ln -s "$tmp/bin/claude" "$tmp/nojq/claude"
ln -s "$(command -v git)" "$tmp/nojq/git"

export FAKE_LOG="$tmp/log" FAKE_MARKETPLACES='[]' FAKE_PLUGINS='[]'

# run <args>: runs bootstrap.sh in the project, piped the way curl would, and prints the
# commands the fake CLI ran.
run() {
  : >"$FAKE_LOG"
  (cd "$project" && PATH="$tmp/bin:$PATH" bash -s -- "$@" <"$bootstrap" >/dev/null 2>&1) || echo "exit $?"
  cat "$FAKE_LOG"
}

echo "bootstrap.sh: installing"

check "a fresh install adds the marketplace and installs neoplanner" "\
plugin marketplace add ewilazarus/neoplanner --scope user
plugin install neoplanner@neoplanner --scope user" "$(run)"

# fake <marketplaces> <plugins>: what the fake CLI lists. (Bash 3.2 doesn't pass a
# `VAR=x function` prefix on to the function's commands, so these are set globally.)
fake() { FAKE_MARKETPLACES=$1 FAKE_PLUGINS=$2; }

fake '[{"name":"neoplanner"}]' '[{"id":"neoplanner@neoplanner","scope":"user"}]'
check "an existing install is updated instead" "\
plugin marketplace update neoplanner
plugin update neoplanner@neoplanner --scope user" "$(run)"

fake '[]' '[]'

echo "bootstrap.sh: --project"

check "a project install declares the marketplace in the project" "\
plugin marketplace add ewilazarus/neoplanner --scope project
plugin install neoplanner@neoplanner --scope project" "$(run --project)"

fake '[]' '[{"id":"neoplanner@neoplanner","scope":"project","projectPath":"/elsewhere"}]'
check "another project's install doesn't count" "\
plugin marketplace add ewilazarus/neoplanner --scope project
plugin install neoplanner@neoplanner --scope project" "$(run --project)"

mkdir -p "$project/.claude"
echo '{"extraKnownMarketplaces": {"neoplanner": {}}}' >"$project/.claude/settings.json"
fake '[]' "[{\"id\":\"neoplanner@neoplanner\",\"scope\":\"project\",\"projectPath\":\"$project\"}]"
check "this project's install is updated, and its declared marketplace kept" "\
plugin marketplace update neoplanner
plugin update neoplanner@neoplanner --scope project" "$(run --project)"
fake '[]' '[]'

echo "bootstrap.sh: options and problems"

check "--dry-run runs nothing" "" "$(run --dry-run)"
check "an unknown option fails" "exit 1" "$(run --bogus)"
check "no claude CLI fails with a pointer" \
  "neoplanner: The claude CLI isn't on your PATH. Install Claude Code first: https://docs.claude.com/en/docs/claude-code" \
  "$(cd "$project" && PATH=/usr/bin:/bin bash "$bootstrap" 2>&1 || true)"
check "without jq it stops, and runs nothing" "no jq|" \
  "$(: >"$FAKE_LOG"; out=$(cd "$project" && PATH="$tmp/nojq:/usr/bin:/bin" bash "$bootstrap" 2>&1)
     if [ -x /usr/bin/jq ]; then echo "no jq|"; else printf '%s' "$out" | grep -q "jq isn't installed" && printf 'no jq|%s' "$(cat "$FAKE_LOG")"; fi)"

echo "bootstrap.sh: the command on the PATH"

mkdir -p "$tmp/install/bin"
printf '#!/bin/sh\necho plugin-launcher "$@"; echo "$@" >>"%s"\n' "$tmp/launches" >"$tmp/install/bin/neoplanner" && chmod +x "$tmp/install/bin/neoplanner"
fake '[]' "[{\"id\":\"neoplanner@neoplanner\",\"scope\":\"user\",\"installPath\":\"$tmp/install\"}]"
run >/dev/null
check "it points ~/.local/bin/neoplanner at the installed plugin" "plugin-launcher status" \
  "$("$HOME/.local/bin/neoplanner" status)"
check "after the launcher fetched the binary, once" "--version" "$(head -n 1 "$tmp/launches")"
fake '[]' '[]'
rm -f "$HOME/.local/bin/neoplanner"
run >/dev/null
check "and leaves it alone when it can't find the plugin" "missing" \
  "$([ -e "$HOME/.local/bin/neoplanner" ] && echo present || echo missing)"

check "a truncated download runs nothing" "" \
  "$(: >"$FAKE_LOG"; (cd "$project" && head -n 40 "$bootstrap" | PATH="$tmp/bin:$PATH" bash >/dev/null 2>&1) || true; cat "$FAKE_LOG")"

finish
