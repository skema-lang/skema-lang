{ pkgs, ... }:

{
  # ---------------------------------------------------------------------------
  # Rust toolchain
  # ---------------------------------------------------------------------------
  # On your system, Rust is on the PATH via `nix profile`, but without rustfmt,
  # clippy, and rust-analyzer. Here we define the toolchain per-project:
  # devenv pins the exact version in devenv.lock, so all contributors (and CI)
  # use bit-for-bit the same tools. That matters more for a language project
  # than elsewhere – rustfmt and clippy change their behavior between
  # releases, and you don't want CI to go red because of a different rustfmt
  # version.
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
      "rust-src" # rust-analyzer needs the std sources for go-to-definition
    ];
    # `crates/skema-wasm` compiles the core to WASM for the browser.
    targets = [ "wasm32-unknown-unknown" ];
  };

  packages = [
    # Snapshot tests. Once the parser is in place, this is *the* tool for
    # testing AST output without typing out expected trees by hand.
    pkgs.cargo-insta
    # Builds/tests crates/skema-wasm's browser bindings.
    pkgs.wasm-pack
    pkgs.nodejs
  ];

  # ---------------------------------------------------------------------------
  # Shortcuts – just type `check`, `t`, `fix` in the shell
  # ---------------------------------------------------------------------------
  scripts = {
    check.exec = ''
      set -e
      cargo fmt --all --check
      cargo clippy --workspace --all-targets -- -D warnings
      cargo test --workspace
    '';
    t.exec = "cargo test --workspace \"$@\"";
    fix.exec = ''
      set -e
      cargo fmt --all
      cargo clippy --workspace --all-targets --fix --allow-dirty
    '';
    # `check`/`t` above already build, clippy, and test `skema-wasm` for the
    # *host* target (wasm-bindgen supports that for tooling). This actually
    # runs its `#[wasm_bindgen_test]`s under wasm32, in Node.
    wasm-test.exec = ''
      set -e
      cargo clippy -p skema-wasm --target wasm32-unknown-unknown --all-targets -- -D warnings
      (cd crates/skema-wasm && wasm-pack test --node)
    '';
  };

  # `devenv test` – the same as `check`, for CI
  enterTest = ''
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
  '';

  enterShell = ''
    echo "skema · $(rustc --version)"
    echo "  check  – fmt + clippy + tests"
    echo "  t      – tests only"
    echo "  fix    – fmt + clippy --fix"
    echo "  wasm-test – runs skema-wasm's tests for real, under wasm32"
    echo "  cargo run -p skema-cli -- check <file.skema>"
  '';
}
