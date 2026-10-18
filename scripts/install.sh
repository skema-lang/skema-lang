#!/bin/sh
# Installs the latest final `skema` CLI release (Linux x86_64) into
# ${SKEMA_INSTALL_DIR:-$HOME/.local/bin}.
#   SKEMA_VERSION=0.5.0  pin a version instead of resolving the latest
#   SKEMA_DOWNLOAD_BASE  override the package URL (mirrors, offline tests)
set -eu

PROJECT="skema-language%2Fskema"
API="https://gitlab.com/api/v4/projects/$PROJECT"
TARGET="x86_64-unknown-linux-musl"
ASSET="skema-$TARGET"
RELEASES_PAGE="https://gitlab.com/skema-language/skema/-/releases"

fail() {
  echo "skema install: $*" >&2
  exit 1
}

check_platform() {
  os=$(uname -s)
  arch=$(uname -m)
  if [ "$os" = "Linux" ] && { [ "$arch" = "x86_64" ] || [ "$arch" = "amd64" ]; }; then
    return 0
  fi
  fail "no prebuilt binary for $os $arch (only Linux x86_64).
Download one from $RELEASES_PAGE
or build from source: cargo install --git https://gitlab.com/skema-language/skema.git skema-cli"
}

resolve_version() {
  if [ -n "${SKEMA_VERSION:-}" ]; then
    echo "${SKEMA_VERSION#v}"
    return 0
  fi
  tag=$(curl -fsSL "$API/releases?per_page=50" |
    grep -o '"tag_name":"v[0-9]*\.[0-9]*\.[0-9]*"' |
    head -n 1 | cut -d'"' -f4) || tag=""
  [ -n "$tag" ] || fail "could not find the latest release; set SKEMA_VERSION or see $RELEASES_PAGE"
  echo "${tag#v}"
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    fail "need sha256sum or shasum to verify the download"
  fi
}

main() {
  check_platform
  command -v curl >/dev/null 2>&1 || fail "curl is required"

  version=$(resolve_version)
  base="${SKEMA_DOWNLOAD_BASE:-$API/packages/generic/skema/$version}"
  install_dir="${SKEMA_INSTALL_DIR:-$HOME/.local/bin}"

  workdir=$(mktemp -d)
  trap 'rm -rf "$workdir"' EXIT

  echo "Downloading skema $version ($TARGET)"
  curl -fsSL -o "$workdir/$ASSET" "$base/$ASSET" || fail "download failed: $base/$ASSET"
  curl -fsSL -o "$workdir/SHA256SUMS" "$base/SHA256SUMS" || fail "download failed: $base/SHA256SUMS"

  expected=$(grep " $ASSET\$" "$workdir/SHA256SUMS" | cut -d' ' -f1)
  [ -n "$expected" ] || fail "SHA256SUMS has no entry for $ASSET"
  actual=$(sha256_of "$workdir/$ASSET")
  [ "$expected" = "$actual" ] || fail "checksum mismatch for $ASSET (expected $expected, got $actual); nothing installed"

  mkdir -p "$install_dir"
  chmod +x "$workdir/$ASSET"
  mv "$workdir/$ASSET" "$install_dir/skema"
  echo "Installed $install_dir/skema"

  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *) echo "Add it to your PATH:  export PATH=\"$install_dir:\$PATH\"" ;;
  esac
}

main "$@"
