#!/usr/bin/env bash
#
# Replays the bump table (brain/release-chain.md, "Bump rules") against this
# repo's own cliff.toml: the version release.sh derives from conventional
# commits is the one thing nothing else checks, and a wrong one is a wrong
# tag pushed downstream and to the package registry.
#
# Runs entirely in a throwaway git repo, seeded with only cliff.toml - no
# origin, so it cannot reach the real remote even if release.sh's use of one
# were ever copied here.
#
#     scripts/release-test.sh
#
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
R="$(mktemp -d)"
trap 'rm -rf "$R"' EXIT

# Signing off, deliberately: a contributor with commit.gpgsign set globally
# and no usable key in this shell would otherwise get empty commits and a
# suite that passes by measuring nothing.
G() {
  git -C "$R" -c commit.gpgsign=false -c tag.gpgsign=false \
      -c user.email=test@localhost -c user.name=test "$@"
}

fresh() {
  rm -rf "$R"; mkdir -p "$R"
  G init --quiet --initial-branch=main
  # Pick up uncommitted edits to cliff.toml, so a run here says something
  # about the working tree rather than about the last commit.
  cp "$ROOT/cliff.toml" "$R/cliff.toml"
  G add cliff.toml
  G commit -q -m "test: baseline"
}

commit() { G commit -q --allow-empty -m "$1"; }

pass=0; fail=0
expect() {
  local label="$1" want="$2"
  local got
  got=$(cd "$R" && git cliff --bumped-version 2>/dev/null)
  if [ "$got" = "$want" ]; then
    pass=$((pass+1)); printf '  ok    %-52s -> %s\n' "$label" "$got"
  else
    fail=$((fail+1)); printf '  FAIL  %-52s -> %s (wanted %s)\n' "$label" "${got:-<none>}" "$want"
  fi
}

echo "from v0.2.1:"
fresh; G tag -a v0.2.1 -m v0.2.1; commit "chore: nothing"; commit "docs: typo"
expect "chore, docs => no bump" v0.2.1
fresh; G tag -a v0.2.1 -m v0.2.1; commit "fix: a bug"
expect "fix: => patch" v0.2.2
fresh; G tag -a v0.2.1 -m v0.2.1; commit "perf: faster"
expect "perf: => patch" v0.2.2
fresh; G tag -a v0.2.1 -m v0.2.1; commit "feat: a feature"
expect "feat: => minor" v0.3.0
fresh; G tag -a v0.2.1 -m v0.2.1; commit "feat!: breaking"
expect "feat!: => minor (pre-1.0)" v0.3.0
fresh; G tag -a v0.2.1 -m v0.2.1; commit "refactor(hir)!: breaking refactor"
expect "type(scope)!: => minor" v0.3.0
fresh; G tag -a v0.2.1 -m v0.2.1; commit $'chore: something\n\nBREAKING CHANGE: it broke'
expect "BREAKING CHANGE footer => minor" v0.3.0
fresh; G tag -a v0.2.1 -m v0.2.1; commit "non-conventional message only"
expect "non-conventional message => no bump" v0.2.1
fresh; G tag -a v0.2.1 -m v0.2.1; commit "fix(deps): adopt skema v0.3.0"
expect "fix(deps): adopt ... => patch" v0.2.2
fresh; G tag -a v0.2.1 -m v0.2.1; commit "chore: nothing"; commit "fix: a bug"
expect "chore then fix => patch" v0.2.2
fresh; G tag -a v0.2.1 -m v0.2.1; G tag 0.9.0; commit "fix: a bug"
expect "stray non-vX.Y.Z tag ignored" v0.2.2

echo
echo "from v1.2.3, where the pre-1.0 cap no longer applies:"
fresh; G tag -a v1.2.3 -m v1.2.3; commit "feat!: breaking"
expect "feat!: => major" v2.0.0
fresh; G tag -a v1.2.3 -m v1.2.3; commit "chore(release): v1.2.3"
expect "chore(release) commit alone => no bump" v1.2.3

echo
echo "with no tag at all:"
fresh; commit "feat: something"
expect "no tag => initial_tag" v0.1.0

echo
echo "$pass passed, $fail failed"
[ "$fail" = 0 ]
