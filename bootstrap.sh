#!/usr/bin/env bash
# Install neoplanner for Claude Code.
#
#   curl -fsSL https://raw.githubusercontent.com/ewilazarus/neoplanner/main/bootstrap.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/ewilazarus/neoplanner/main/bootstrap.sh | bash -s -- --project
#
# By default the plugin is installed for you, in every project. With --project, run from a
# project's root, it is declared in its .claude/settings.json instead, so everyone who
# opens the project is offered it. --dry-run prints the commands without running them.
#
# It adds the marketplace, then installs neoplanner, or updates it if it's already there,
# so it is safe to run again. It then downloads the plugin's neoplanner binary from the
# GitHub release matching its version, so nothing waits for it later, and puts
# `neoplanner` in ~/.local/bin. Then run /neoplanner:init in a project. Needs the `claude`
# CLI, git and jq, and OpenSpec's `openspec` CLI, 1.6 or later, to plan.

set -euo pipefail

main() {
  local scope=user dry_run="" arg
  for arg in "$@"; do
    case "$arg" in
      --project) scope=project ;;
      --dry-run) dry_run=1 ;;
      -h | --help) sed -n '2,15p' "${BASH_SOURCE[0]:-/dev/null}" 2>/dev/null | sed 's/^# \{0,1\}//'; return 0 ;;
      *) fail "Unknown option: $arg. Use --project, --dry-run or --help." ;;
    esac
  done

  command -v claude >/dev/null 2>&1 ||
    fail "The claude CLI isn't on your PATH. Install Claude Code first: https://docs.claude.com/en/docs/claude-code"
  command -v jq >/dev/null 2>&1 ||
    fail "jq isn't installed, and this script needs it: install it with your package manager (brew install jq, apt install jq)."
  command -v git >/dev/null 2>&1 ||
    warn "git isn't installed. neoplanner uses it to find a project, and to keep a stealth store's history."
  command -v openspec >/dev/null 2>&1 ||
    warn "OpenSpec's CLI isn't installed, and neoplanner runs it: npm install -g @fission-ai/openspec@latest"

  if [ "$scope" = project ]; then
    git rev-parse --show-toplevel >/dev/null 2>&1 ||
      warn "This isn't a git repository. --project writes .claude/settings.json here, in $(pwd)."
  fi

  # A project-scope install is listed once per project, with its projectPath.
  local marketplaces plugins here
  here=$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)
  marketplaces=$(listing "claude plugin marketplace list --json" '.[].name')
  plugins=$(listing "claude plugin list --json" \
    '.[] | select(.scope == $scope and (.scope == "user" or .projectPath == $here)) | .id')

  add_marketplace neoplanner ewilazarus/neoplanner

  if printf '%s\n' "$plugins" | grep -qx 'neoplanner@neoplanner'; then
    say "neoplanner is installed ($scope); updating it."
    run claude plugin marketplace update neoplanner
    run claude plugin update neoplanner@neoplanner --scope "$scope"
  else
    say "Installing neoplanner ($scope)."
    run claude plugin install neoplanner@neoplanner --scope "$scope"
  fi

  link_cli

  echo
  say "Done. Restart Claude Code, open a project, and run /neoplanner:init to set up its planning."
  [ "$scope" = user ] ||
    say "Commit .claude/settings.json, so everyone who opens the project is offered the plugin."
}

say() { printf 'neoplanner: %s\n' "$*"; }
warn() { printf 'neoplanner: warning: %s\n' "$*" >&2; }
fail() { printf 'neoplanner: %s\n' "$*" >&2; exit 1; }

# run <command...>: runs it, or only prints it with --dry-run. stdin is closed, so nothing
# reads the rest of this script when it arrives through a pipe.
run() {
  printf '  $ %s\n' "$*"
  [ -n "$dry_run" ] || "$@" </dev/null
}

# listing <command> <jq filter>: the names a JSON listing reports, one per line, or nothing
# when the listing fails, in which case every step simply runs.
listing() {
  $1 </dev/null 2>/dev/null | jq -r --arg scope "$scope" --arg here "${here:-}" "$2" 2>/dev/null || true
}

add_marketplace() {
  local name=$1 repo=$2
  if [ "$scope" = user ] && printf '%s\n' "$marketplaces" | grep -qx "$name"; then
    say "The $name marketplace is already added."
    return 0
  fi
  if [ "$scope" = project ] && [ -f .claude/settings.json ] &&
    jq -e --arg n "$name" '.extraKnownMarketplaces[$n]' .claude/settings.json >/dev/null 2>&1; then
    say "The $name marketplace is already declared in .claude/settings.json."
    return 0
  fi
  say "Adding the $name marketplace ($repo)."
  run claude plugin marketplace add "$repo" --scope "$scope"
}

# Puts `neoplanner` on the PATH, as a wrapper around the installed plugin's own launcher,
# so the shell runs the same version Claude does. Rerunning this script after an
# update points it at the new version.
link_cli() {
  local install target="$HOME/.local/bin/neoplanner"
  if [ -n "$dry_run" ]; then
    printf '  $ write %s\n' "$target"
    return 0
  fi
  install=$(claude plugin list --json </dev/null 2>/dev/null |
    jq -r '[.[] | select(.id == "neoplanner@neoplanner")][0].installPath // empty' 2>/dev/null || true)
  if [ -z "$install" ] || [ ! -x "$install/bin/neoplanner" ]; then
    warn "Couldn't find the installed plugin, so neoplanner isn't on your PATH. The plugin itself still works."
    return 0
  fi
  mkdir -p "$(dirname "$target")"
  # The launcher downloads and checks the release matching the plugin's version, once.
  if ! version=$("$install/bin/neoplanner" --version </dev/null); then
    warn "Couldn't download the neoplanner binary. The plugin retries when it's first needed."
    return 0
  fi
  say "Downloaded $version."
  printf '#!/bin/sh\nexec "%s/bin/neoplanner" "$@"\n' "$install" >"$target"
  chmod +x "$target"
  say "neoplanner is on your PATH at $target."
  case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) warn "Add ~/.local/bin to your PATH for git to find it." ;; esac
}

main "$@"
