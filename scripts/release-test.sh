#!/bin/bash
set -euo pipefail

# Tests git-cliff's bump logic against cliff.toml by replaying the scenario table
# from release-chain.md in scratch clones. Does not call release.sh.

REPO_ROOT="$(dirname "$0")/.."
WORKDIR=$(mktemp -d)
trap "rm -rf '$WORKDIR'" EXIT

# Install git-cliff if needed.
if ! command -v git-cliff &> /dev/null; then
  echo "Installing git-cliff..."
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
fi

echo "Testing git-cliff bump logic..."
passed=0
failed=0

test_one() {
  local desc="$1"
  local base_tag="$2"
  local commits="$3"
  local expected="$4"

  local test_repo="$WORKDIR/test-$RANDOM"
  mkdir -p "$test_repo"
  cp "$REPO_ROOT/cliff.toml" "$test_repo/"

  local result
  result=$(
    cd "$test_repo"
    git init > /dev/null 2>&1
    git config user.email "test@example.com"
    git config user.name "Test"
    git config commit.gpgsign false

    # Set up baseline tag if provided.
    if [[ -n "$base_tag" ]]; then
      echo "initial" > README.md
      git add README.md
      git commit -m "initial" > /dev/null 2>&1
      git tag "$base_tag" > /dev/null 2>&1
    else
      # Need at least one commit for git-cliff to work on a repo with no tags
      echo "initial" > README.md
      git add README.md
      git commit -m "chore: init" > /dev/null 2>&1
    fi

    # Add test commits.
    while IFS='|' read -r msg; do
      msg="${msg#"${msg%%[![:space:]]*}"}"
      msg="${msg%"${msg##*[![:space:]]}"}"
      if [[ -n "$msg" ]]; then
        echo "$msg" >> README.md
        git add README.md
        git commit -m "$msg" > /dev/null 2>&1
      fi
    done <<< "$commits"

    # Get bumped version, extract only the vX.Y.Z part from stdout
    git-cliff --bumped-version 2>/dev/null | grep -oP '^v\d+\.\d+\.\d+' || echo "v0.1.0"
  )

  if [[ "$result" == "$expected" ]]; then
    echo "✓ $desc"
    ((++passed))
  else
    echo "✗ $desc (expected $expected, got $result)"
    ((++failed))
  fi
}

# Base v0.2.1 tests.
test_one "v0.2.1 → v0.2.2 (fix)" "v0.2.1" "fix: a" "v0.2.2"
test_one "v0.2.1 → v0.3.0 (feat)" "v0.2.1" "feat: a" "v0.3.0"
test_one "v0.2.1 → v0.3.0 (breaking in 0.x)" "v0.2.1" "feat!: a" "v0.3.0"
test_one "v0.2.1 → v0.2.1 (chore)" "v0.2.1" "chore: a" "v0.2.1"
test_one "v0.2.1 → v0.2.2 (fix+chore)" "v0.2.1" "fix: a|chore: b" "v0.2.2"
test_one "v0.2.1 → v0.3.0 (feat+fix)" "v0.2.1" "feat: a|fix: b" "v0.3.0"

# Base v1.2.3 tests.
test_one "v1.2.3 → v1.2.4 (fix)" "v1.2.3" "fix: a" "v1.2.4"
test_one "v1.2.3 → v1.3.0 (feat)" "v1.2.3" "feat: a" "v1.3.0"
test_one "v1.2.3 → v2.0.0 (breaking)" "v1.2.3" "feat!: a" "v2.0.0"
test_one "v1.2.3 → v1.2.3 (chore)" "v1.2.3" "chore: a" "v1.2.3"

# No tag tests.
test_one "no tag → v0.1.0 (empty)" "" "" "v0.1.0"
test_one "no tag → v0.1.0 (fix)" "" "fix: a" "v0.1.0"
test_one "no tag → v0.1.0 (feat)" "" "feat: a" "v0.1.0"
test_one "no tag → v0.1.0 (chore)" "" "chore: a" "v0.1.0"

echo ""
echo "Results: $passed passed, $failed failed"
[[ $failed -eq 0 ]] || exit 1
