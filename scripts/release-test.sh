#!/usr/bin/env bash
#
# Tests the version derivation in release.sh.
#
# The bump is the one part of the release that nothing else checks: a wrong
# version is not a build failure, it is a wrong tag pushed to three repos and
# a package registry, which is expensive to take back. So it gets a test even
# though the rest of this directory does not.
#
# Runs entirely in a throwaway clone, with --dry-run, and with the origin
# removed - it cannot reach the real remote even if a guard in release.sh
# broke.
#
#     scripts/release-test.sh
#
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
R="$(mktemp -d)"
trap 'rm -rf "$R"' EXIT

# Signing off, deliberately: a contributor with commit.gpgsign set globally and
# no usable key in this shell would otherwise get empty commits and a suite
# that passes by measuring nothing.
G() {
  git -C "$R" -c commit.gpgsign=false -c tag.gpgsign=false \
      -c user.email=test@localhost -c user.name=test "$@"
}

fresh() {
  rm -rf "$R"; mkdir -p "$R"
  git clone --quiet "$ROOT" "$R"
  G remote remove origin
  mkdir -p "$R/scripts"
  cp "$ROOT/scripts/release.sh" "$R/scripts/release.sh"
  G add -A
  G commit -qm "test: baseline"
  G tag -a v0.1.0 -m v0.1.0
}

commit() { G commit -q --allow-empty -m "$1"; }

pass=0; fail=0
expect() {
  local label="$1" want="$2"; shift 2
  local got
  got=$(cd "$R" && env "$@" bash scripts/release.sh --dry-run 2>&1 |
        sed -n -E 's/^Releasing v([0-9.]+) .*/\1/p')
  if [ "$got" = "$want" ]; then
    pass=$((pass+1)); printf '  ok    %-38s -> %s\n' "$label" "$got"
  else
    fail=$((fail+1)); printf '  FAIL  %-38s -> %s (wanted %s)\n' "$label" "${got:-<none>}" "$want"
  fi
}

refuses() {
  local label="$1" needle="$2"; shift 2
  local out
  out=$(cd "$R" && env "$@" bash scripts/release.sh --dry-run 2>&1)
  if printf '%s' "$out" | grep -q "$needle"; then
    pass=$((pass+1)); printf '  ok    %s\n' "$label"
  else
    fail=$((fail+1)); printf '  FAIL  %s\n%s\n' "$label" "$out"
  fi
}

echo "from v0.1.0:"
fresh; commit "Correct a thing the old way"
expect "history that says nothing => patch" 0.1.1
fresh; commit "fix: a bug"
expect "fix: => patch" 0.1.1
fresh; commit "feat: a feature"
expect "feat: => minor" 0.2.0
fresh; commit "feat(parser): a scoped feature"
expect "feat(scope): => minor" 0.2.0
fresh; commit "feat!: a breaking feature"
expect "feat!: pre-1.0 => minor, not 1.0.0" 0.2.0
fresh; commit "refactor(hir)!: breaking refactor"
expect "type(scope)!: pre-1.0 => minor" 0.2.0
fresh; commit $'feat: something\n\nBREAKING CHANGE: it broke'
expect "BREAKING CHANGE trailer => minor" 0.2.0
fresh; commit "feat: one"; commit "fix: two"
expect "the highest level in the range wins" 0.2.0
fresh; commit "fix: a bug"
expect "BUMP overrides the derivation" 0.2.0 BUMP=minor
fresh; commit "fix: a bug"
expect "BUMP=major is still capped pre-1.0" 0.2.0 BUMP=major
fresh; commit "fix: a bug"
expect "RELEASE_VERSION wins outright" 1.0.0 RELEASE_VERSION=1.0.0
fresh; commit "fix: a bug"
expect "RELEASE_VERSION accepts a v prefix" 2.3.4 RELEASE_VERSION=v2.3.4

echo
echo "from v1.2.3, where the cap no longer applies:"
fresh; G tag -a v1.2.3 -m v1.2.3; commit "feat!: breaking"
expect "breaking => major" 2.0.0
fresh; G tag -a v1.2.3 -m v1.2.3; commit "feat: additive"
expect "feat => minor" 1.3.0
fresh; G tag -a v1.2.3 -m v1.2.3; commit "chore: nothing much"
expect "anything else => patch" 1.2.4

echo
echo "guards:"
fresh
refuses "a nonsense BUMP is refused" "must be major, minor or patch" BUMP=sideways
fresh
refuses "an existing tag is not re-released" "already exists" RELEASE_VERSION=0.1.0

echo
echo "$pass passed, $fail failed"
[ "$fail" = 0 ]
