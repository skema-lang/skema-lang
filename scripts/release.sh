#!/usr/bin/env bash
#
# Works out the next version, writes it into Cargo.toml, and pushes the
# commit and the tag that starts the actual release.
#
# The split matters: this script never builds or publishes anything. It only
# moves the version forward and pushes `vX.Y.Z`. Everything that ships — the
# binaries, the wheel, the crates, the image, and the downstream repos — hangs
# off the *tag* pipeline in .gitlab-ci.yml, which is the same pipeline you get
# by pushing a tag by hand. Automating the bump therefore adds a trigger, not
# a second release path.
#
#     scripts/release.sh --dry-run     # print what it would do, touch nothing
#     scripts/release.sh               # bump, commit, tag, push
#
# Environment:
#   BUMP=major|minor|patch   force a level instead of deriving one
#   RELEASE_VERSION=1.2.3    use exactly this version, derive nothing
#   RELEASE_TOKEN            GitLab token with write_repository (CI only)
#
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

say() { printf '%s\n' "$*" >&2; }
run() {
  if [ "$DRY_RUN" = 1 ]; then say "  would run: $*"; else "$@"; fi
}

# --------------------------------------------------------------------------
# Where we are now
# --------------------------------------------------------------------------

# The tag is the source of truth for "what was last released", not Cargo.toml:
# Cargo.toml is what the *next* release will carry, and on a fresh repo the two
# are deliberately the same (see below).
LAST_TAG=$(git tag -l 'v[0-9]*' --sort=-v:refname | head -n1)

CARGO_VERSION=$(sed -n '/^\[workspace\.package\]/,/^\[/p' Cargo.toml |
  sed -n -E 's/^version = "([^"]+)".*/\1/p' | head -n1)

if [ -z "$CARGO_VERSION" ]; then
  say "Could not read [workspace.package] version from Cargo.toml"
  exit 1
fi

# First release ever: ship the version the workspace already declares rather
# than bumping past it. v0.1.0 is a more honest first tag than v0.1.1 on a
# repo whose Cargo.toml has said 0.1.0 all along.
if [ -z "$LAST_TAG" ]; then
  NEXT="$CARGO_VERSION"
  say "No tag yet — releasing the version Cargo.toml already declares: v$NEXT"
else
  CURRENT="${LAST_TAG#v}"

  # --------------------------------------------------------------------------
  # How far to move
  # --------------------------------------------------------------------------
  if [ -n "${RELEASE_VERSION:-}" ]; then
    NEXT="${RELEASE_VERSION#v}"
    say "RELEASE_VERSION given — releasing v$NEXT"
  else
    LEVEL="${BUMP:-}"
    if [ -z "$LEVEL" ]; then
      # Conventional-commit markers when they are there, patch when they are
      # not. The history in this repo is not written that way today, so the
      # honest default is the smallest step; a `feat:` or a `!` earns more as
      # soon as commits start saying so.
      RANGE="$LAST_TAG..HEAD"
      if git log --format='%s%n%b' "$RANGE" | grep -qE '^[a-z]+(\([^)]*\))?!:|^BREAKING[ -]CHANGE'; then
        LEVEL=major
      elif git log --format='%s' "$RANGE" | grep -qE '^feat(\([^)]*\))?:'; then
        LEVEL=minor
      else
        LEVEL=patch
      fi
      say "Derived bump level from $RANGE: $LEVEL"
    else
      say "BUMP given: $LEVEL"
    fi

    IFS=. read -r MAJOR MINOR PATCH <<<"$CURRENT"
    case "$LEVEL" in
      # Pre-1.0, a breaking change moves the minor instead of declaring 1.0.
      # Reaching 1.0 is a decision someone makes on purpose, with
      # RELEASE_VERSION=1.0.0 — not something a `!` in a commit message does.
      major) if [ "$MAJOR" = 0 ]; then MINOR=$((MINOR + 1)); PATCH=0
             else MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0; fi ;;
      minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
      patch) PATCH=$((PATCH + 1)) ;;
      *) say "BUMP must be major, minor or patch — got '$LEVEL'"; exit 1 ;;
    esac
    NEXT="$MAJOR.$MINOR.$PATCH"
  fi
fi

if git rev-parse -q --verify "refs/tags/v$NEXT" >/dev/null; then
  say "Tag v$NEXT already exists — nothing to do"
  exit 0
fi

say "Releasing v$NEXT (was ${LAST_TAG:-none}, Cargo.toml says $CARGO_VERSION)"

# --------------------------------------------------------------------------
# Write it down
# --------------------------------------------------------------------------

# Two places, both in the root manifest: every crate takes its version from
# `[workspace.package]`, and the internal path dependencies repeat it because
# `cargo publish` refuses a dependency without a version (see Cargo.toml).
# Leaving the second set behind would publish 0.2.0 crates that ask each other
# for ^0.1.0.
if [ "$CARGO_VERSION" != "$NEXT" ]; then
  if [ "$DRY_RUN" = 1 ]; then
    say "  would set version $CARGO_VERSION -> $NEXT in Cargo.toml and Cargo.lock"
  else
    sed -i -E "s|^version = \"[0-9][^\"]*\"|version = \"$NEXT\"|" Cargo.toml
    sed -i -E "s|^(skema-[a-z]+ = \{ path = \"[^\"]+\", version = )\"[0-9][^\"]*\"|\1\"$NEXT\"|" Cargo.toml

    # Cargo.lock records the workspace members' own versions, so it goes stale
    # the moment the manifest moves. `cargo update --workspace` is the obvious
    # way to refresh it — and would drag a whole Rust toolchain into a job that
    # otherwise needs nothing but git. The members are exactly the crates/*
    # directories, their entries are the only ones that move, and no other
    # entry may be touched, so rewriting those version lines is both cheaper
    # and narrower than letting cargo re-resolve.
    MEMBERS=$(sed -n -E 's|^\s*"crates/([a-z-]+)",?\s*$|\1|p' Cargo.toml | paste -sd'|' -)
    awk -v members="$MEMBERS" -v next_version="$NEXT" '
      $0 ~ /^\[\[package\]\]$/ { member = 0 }
      /^name = "/ {
        name = $0; sub(/^name = "/, "", name); sub(/"$/, "", name)
        member = (name ~ "^(" members ")$")
      }
      member && /^version = "/ { print "version = \"" next_version "\""; next }
      { print }
    ' Cargo.lock >Cargo.lock.tmp && mv Cargo.lock.tmp Cargo.lock
  fi
fi

# --------------------------------------------------------------------------
# Commit, tag, push
# --------------------------------------------------------------------------

if [ -n "${CI:-}" ]; then
  git config user.email "${GITLAB_USER_EMAIL:-ci@skema-language}"
  git config user.name "${GITLAB_USER_NAME:-skema release}"
fi

# `chore(release):` is not decoration — .gitlab-ci.yml matches on this exact
# prefix to keep this job from triggering itself on its own commit. `[skip ci]`
# would be the usual trick and is wrong here: the tag points at this very
# commit, so GitLab would skip the tag pipeline too, which is the one that
# actually publishes.
MSG="chore(release): v$NEXT"

run git add Cargo.toml Cargo.lock
if [ "$DRY_RUN" = 0 ] && git diff --cached --quiet; then
  say "Nothing changed in the manifest — tagging the current commit"
else
  run git commit -m "$MSG"
fi
run git tag -a "v$NEXT" -m "$MSG"

if [ "$DRY_RUN" = 1 ]; then
  say "  would push HEAD and v$NEXT"
  exit 0
fi

if [ -n "${CI:-}" ]; then
  # The job token does the pushing — no personal or project access token
  # involved. Needs Settings → CI/CD → Job token permissions → "Allow Git push
  # requests to the repository" (GA in GitLab 18.4, all tiers, off by
  # default). Written into the remote URL rather than a header so the tag push
  # is authenticated too.
  git remote set-url origin \
    "https://gitlab-ci-token:${CI_JOB_TOKEN}@${CI_SERVER_HOST}/${CI_PROJECT_PATH}.git"
fi

git push origin "HEAD:${CI_DEFAULT_BRANCH:-main}"
git push origin "v$NEXT"

say "Pushed v$NEXT"

# ...and now ask for the pipeline that push did not start.
#
# "When you use a job token to push to the project, no CI/CD pipelines are
# triggered" — that is what stops a release job from re-triggering itself, and
# it is also why the tag alone publishes nothing. The tag pipeline is the one
# that builds the binaries, uploads them and triggers the downstream repos, so
# it has to be requested explicitly.
#
# A pipeline trigger token is the cheapest credential that can do it: project
# scoped, free, and able to do nothing except start a pipeline — unlike a
# personal access token, which would carry write access to the repository and
# belong to whoever created it.
if [ -n "${CI:-}" ]; then
  if [ -z "${RELEASE_TRIGGER_TOKEN:-}" ]; then
    say "RELEASE_TRIGGER_TOKEN is not set — v$NEXT is tagged, but nothing will"
    say "publish it. Add a trigger token (Settings → CI/CD → Pipeline trigger"
    say "tokens), or start a pipeline for v$NEXT by hand."
    exit 1
  fi
  say "Starting the release pipeline for v$NEXT"
  curl --fail --silent --show-error --output /dev/null \
    --form "token=${RELEASE_TRIGGER_TOKEN}" \
    --form "ref=v$NEXT" \
    "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/trigger/pipeline"
fi
