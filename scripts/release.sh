#!/usr/bin/env bash
#
# Works out the next version from conventional commits using git-cliff,
# writes it into Cargo.toml, and pushes the commit and the tag that starts
# the actual release.
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
#   TRIGGER_DOWNSTREAM    whether to request the tag pipeline (true or false)
#   RELEASE_TRIGGER_TOKEN GitLab pipeline trigger token (CI only)
#
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

say() { printf '%s\n' "$*" >&2; }
run() {
  if [ "$DRY_RUN" = 1 ]; then say "  would run: $*"; else "$@"; fi
}

# Install git-cliff if not present
if ! command -v git-cliff &> /dev/null; then
  say "Installing git-cliff..."
  CLIFF_VERSION="2.10.1"
  CLIFF_ARCH="x86_64-unknown-linux-musl"
  CLIFF_URL="https://github.com/orhun/git-cliff/releases/download/v${CLIFF_VERSION}/git-cliff-${CLIFF_VERSION}-${CLIFF_ARCH}.tar.gz"
  CLIFF_SHA_URL="${CLIFF_URL}.sha512"

  mkdir -p /tmp/git-cliff-install
  cd /tmp/git-cliff-install
  curl -fL "$CLIFF_URL" -o git-cliff.tar.gz
  curl -fL "$CLIFF_SHA_URL" -o git-cliff.tar.gz.sha512
  sha512sum -c git-cliff.tar.gz.sha512 > /dev/null
  tar -xzf git-cliff.tar.gz
  mkdir -p ~/.local/bin
  mv git-cliff ~/.local/bin/
  export PATH="$HOME/.local/bin:$PATH"
  cd - > /dev/null
fi

# Compute the next version using git-cliff
NEXT_WITH_V=$(git-cliff --bumped-version)
NEXT="${NEXT_WITH_V#v}"

say "Next version from git-cliff: v$NEXT"

# Nothing to release: exit cleanly (no commits matching conventional patterns)
if [ -z "$NEXT" ] || [ "$NEXT" = "v0.1.0" ]; then
  # Check if this is truly a no-op release by seeing if we're already at the computed tag
  if git rev-parse -q --verify "refs/tags/v$NEXT" >/dev/null 2>&1; then
    say "Nothing to release (already at v$NEXT)"
    exit 0
  fi
fi

if git rev-parse -q --verify "refs/tags/v$NEXT" >/dev/null 2>&1; then
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
  TRIGGER_DOWNSTREAM="${TRIGGER_DOWNSTREAM:-true}"
  say "Starting the release pipeline for v$NEXT (trigger_downstream=$TRIGGER_DOWNSTREAM)"
  curl --fail \
    --header "Content-Type: application/json" \
    --data "{\"token\":\"${RELEASE_TRIGGER_TOKEN}\",\"ref\":\"v${NEXT}\",\"inputs\":{\"trigger_downstream\":${TRIGGER_DOWNSTREAM}}}" \
    "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/pipeline" || true
fi
