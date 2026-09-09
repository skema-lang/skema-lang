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
      "llvm-tools" # needed by `cargo llvm-cov` (test coverage)
    ];
    # `crates/skema-wasm` compiles the core to WASM for the browser.
    targets = [
      "wasm32-unknown-unknown"
      # Release targets for the VSCode extension's bundled server. musl
      # links statically, so one Linux binary runs on every distribution
      # regardless of its glibc.
      "x86_64-unknown-linux-musl"
      "x86_64-pc-windows-gnu"
    ];
  };

  # Cross linkers for the release targets above. Rust can *compile* for a
  # target on its own; linking needs that platform's toolchain.
  env = {
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER =
      "${pkgs.pkgsCross.musl64.stdenv.cc}/bin/x86_64-unknown-linux-musl-cc";
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER =
      "${pkgs.pkgsCross.mingwW64.stdenv.cc}/bin/x86_64-w64-mingw32-cc";
    # Rust's windows-gnu target links against a static pthreads that the
    # mingw compiler does not carry itself.
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS =
      "-L ${pkgs.pkgsCross.mingwW64.windows.mcfgthreads}/lib -L ${pkgs.pkgsCross.mingwW64.windows.pthreads}/lib";
  };

  packages = [
    pkgs.pkgsCross.musl64.stdenv.cc
    pkgs.pkgsCross.mingwW64.stdenv.cc

    # Snapshot tests. Once the parser is in place, this is *the* tool for
    # testing AST output without typing out expected trees by hand.
    pkgs.cargo-insta
    # Builds/tests crates/skema-wasm's browser bindings.
    pkgs.wasm-pack
    pkgs.nodejs

    # crates/skema-py (pyo3) needs a Python to build and link against.
    pkgs.python3
    # Packages crates/skema-py into an actual wheel (`maturin build`); doing
    # this via Nix instead of `pip install maturin` keeps the version pinned
    # in devenv.lock like everything else here.
    pkgs.maturin

    # Renders docs/ (mdBook) and generates the rustdoc API reference for
    # GitLab Pages.
    pkgs.mdbook

    # Test coverage (`cargo llvm-cov`) and dependency vulnerability
    # scanning (`cargo audit`) for CI.
    pkgs.cargo-llvm-cov
    pkgs.cargo-audit
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
