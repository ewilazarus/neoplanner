#!/usr/bin/env bash
# Tests for `neoplanner init`, in both modes: what plan reports, what apply writes, and
# what it keeps.

set -eu
. "$(dirname "$0")/lib.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tmp=$(cd "$tmp" && pwd -P)

# Keep the user's own git settings, and their neoplanner and OpenSpec state, out of it.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export XDG_CONFIG_HOME="$tmp/config" XDG_DATA_HOME="$tmp/data" FAKE_OPENSPEC_STATE="$tmp/openspec"
unset CLAUDE_PROJECT_DIR

# project <name> [git]: makes an empty project, a git repo with one commit if asked, and
# prints its path.
project() {
  mkdir -p "$tmp/$1"
  if [ "${2:-}" = git ]; then
    git -C "$tmp/$1" init -q
    printf '# App\n' >"$tmp/$1/README.md"
    git -C "$tmp/$1" add -A
    git -C "$tmp/$1" -c user.name=t -c user.email=t@t commit -qm init
  fi
  printf '%s\n' "$tmp/$1"
}

# run <project> <args>: runs init in the project, and prints its output and any error.
run() { local p=$1; shift; (cd "$p" && "$bin" --project "$p" init "$@" 2>&1 | sed 's/^neoplanner: //') || true; }

echo "init: choosing a mode"

p=$(project undecided)
check "plan without a mode asks for one" "\
mode: needs a choice
  --mode committed: the specs live in openspec/, in the repo, and everyone shares them
  --mode stealth: the specs live in an OpenSpec store outside the repo, with its own history, and no tracked file changes" \
  "$(run "$p" plan)"
check "apply without a mode refuses" "Choose a mode: --mode committed or --mode stealth." "$(run "$p" apply)"
check "an old OpenSpec is refused" \
  "OpenSpec 1.2.0 is too old: neoplanner needs 1.6.0 or later, for stores. Update it with \`npm install -g @fission-ai/openspec@latest\`." \
  "$(FAKE_OPENSPEC_VERSION=1.2.0 run "$p" plan --mode committed)"

echo "init: committed"

p=$(project committed git)
check "plan lists every change" "\
mode: committed (OpenSpec 1.6.0)
.neoplanner/config.toml: new file, with mode = \"committed\"
openspec/: new, from \`openspec init --tools none\`
.claude/settings.json: new file
  add extraKnownMarketplaces.neoplanner
  add enabledPlugins.neoplanner@neoplanner
CLAUDE.md: new file, with the neoplanner section" "$(run "$p" plan --mode committed)"
check "and changes nothing" "README.md" "$(cd "$p" && find . -path ./.git -prune -o -type f -print | sed 's|^\./||')"
run "$p" apply --mode committed >/dev/null
check "apply writes them" "\
.claude/settings.json
.neoplanner/config.toml
CLAUDE.md
README.md
openspec/config.yaml" "$(cd "$p" && find . -path ./.git -prune -o -type f -print | sed 's|^\./||' | sort)"
check "the settings say committed" 'mode = "committed"' "$(grep '^mode' "$p/.neoplanner/config.toml")"
check "the section points at openspec/" "yes" \
  "$(grep -q '`openspec/`, in this repository' "$p/CLAUDE.md" && echo yes)"
check "the root is the project's openspec/" "$p/openspec" "$(cd "$p" && "$bin" --project "$p" root)"
check "a second run changes nothing" "\
mode: committed (OpenSpec 1.6.0)
Nothing to change: the project already has everything init sets up." "$(run "$p" plan)"
check "switching to stealth is refused" \
  "This project is already set up in committed mode. Switching modes isn't supported yet: move the specs by hand, and remove .neoplanner/config.toml first." \
  "$(run "$p" plan --mode stealth)"

p=$(project agents git)
printf '# Agents\n' >"$p/AGENTS.md"
check "AGENTS.md alone needs a choice" "yes" \
  "$(run "$p" plan --mode committed | grep -q '^CLAUDE.md: needs a choice' && echo yes)"
check "and apply refuses without it" \
  "The project has AGENTS.md but no CLAUDE.md. Pass --agents-md import or --agents-md append." \
  "$(run "$p" apply --mode committed)"
run "$p" apply --mode committed --agents-md import >/dev/null
check "import creates CLAUDE.md importing it" "@AGENTS.md" "$(head -n 1 "$p/CLAUDE.md")"

p=$(project opsx git)
mkdir -p "$p/.claude/commands/opsx" "$p/openspec"
printf 'old\n<!-- neoplanner:start -->\nstale\n<!-- neoplanner:end -->\nkept\n' >"$p/CLAUDE.md"
out=$(run "$p" plan --mode committed)
check "an existing openspec/ is adopted" "" "$(printf '%s\n' "$out" | grep '^openspec/' || true)"
check "the old section is replaced" "yes" "$(printf '%s\n' "$out" | grep -q '^CLAUDE.md: update the neoplanner section' && echo yes)"
check "OpenSpec's own commands are noted" "yes" "$(printf '%s\n' "$out" | grep -q '/opsx commands' && echo yes)"
run "$p" apply --mode committed >/dev/null
check "and the text around the section is kept" "old|kept" \
  "$(head -n 1 "$p/CLAUDE.md")|$(tail -n 1 "$p/CLAUDE.md")"

echo "init: stealth"

p=$(project stealthy git)
store="$tmp/data/neoplanner/stores/stealthy"
check "plan lists every change" "\
mode: stealth (OpenSpec 1.6.0)
store stealthy: new, at $store, with its own git repository (openspec store setup)
$tmp/config/neoplanner/projects.toml: map $p to the store stealthy
CLAUDE.local.md: new file, with the neoplanner section
.claude/settings.local.json: new file
  add extraKnownMarketplaces.neoplanner
  add enabledPlugins.neoplanner@neoplanner
.git/info/exclude:
  add CLAUDE.local.md
  add .claude/settings.local.json" "$(run "$p" plan --mode stealth)"
run "$p" apply --mode stealth >/dev/null
check "apply leaves git with nothing to commit" "" "$(git -C "$p" status --porcelain)"
check "the store is set up, with its own history" "Set up the store" "$(git -C "$store" log -1 --format=%s)"
check "the project maps to it" "stealth|$store/openspec" \
  "$(cd "$p" && "$bin" --project "$p" config | sed -n 's/^mode: *//p')|$(cd "$p" && "$bin" --project "$p" root)"
check "the section points at the store" "yes" \
  "$(grep -q "$store/openspec" "$p/CLAUDE.local.md" && echo yes)"
check "a second run changes nothing" "\
mode: stealth, store stealthy (OpenSpec 1.6.0)
Nothing to change: the project already has everything init sets up." "$(run "$p" plan)"

wt="$tmp/stealthy-wt"
git -C "$p" worktree add -q "$wt" 2>/dev/null
check "a worktree of the clone shares the store" "$store/openspec" "$(cd "$wt" && "$bin" --project "$wt" root)"

p2=$(project other git)
check "a store already registered is adopted" "store stealthy:
  kept, already registered at $store" \
  "$(run "$p2" plan --mode stealth --store-id stealthy | sed -n '2,3p')"
check "but not moved" \
  "The store stealthy is already registered at $store, not /elsewhere. Pick another --store-id, or leave out --store-path." \
  "$(run "$p2" plan --mode stealth --store-id stealthy --store-path /elsewhere)"
check "a bad store id is refused" \
  "A store id is lowercase letters, digits and single dashes, such as \"my-project\", not \"My Store\"." \
  "$(run "$p2" plan --mode stealth --store-id "My Store")"

p=$(project tracked git)
printf 'mine\n' >"$p/CLAUDE.local.md"
git -C "$p" add CLAUDE.local.md
check "a tracked CLAUDE.local.md is refused" \
  "CLAUDE.local.md is tracked by git, so stealth mode would change a committed file. Untrack it, or use --mode committed." \
  "$(run "$p" plan --mode stealth)"

p=$(project nogit)
check "outside git, there's nothing to exclude" "yes" \
  "$(run "$p" plan --mode stealth | grep -q 'not a git repository' && echo yes)"

echo "init: both at once"

p=$(project both git)
run "$p" apply --mode stealth >/dev/null
mkdir -p "$p/.neoplanner" && printf 'mode = "committed"\n' >"$p/.neoplanner/config.toml"
check "a project set up twice is an error" "yes" \
  "$(cd "$p" && "$bin" --project "$p" config 2>&1 | grep -q 'set up twice' && echo yes)"

finish
