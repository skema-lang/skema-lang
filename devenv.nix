{ pkgs, lib, ... }:

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
      "llvm-tools" # needed by `cargo llvm-cov` (test coverage)
    ]
    # Editor tooling, deliberately left out of CI. Five build-stage jobs run
    # at once, each unpacking its own copy of this toolchain into its own
    # container, and the runner ran out of disk doing it (pipeline 58,
    # "No space left on device" while unpacking rustc). These two are the
    # only components nothing in CI ever invokes. The reproducibility the
    # header above cares about is unaffected: rustc/cargo/clippy/rustfmt are
    # still pinned identically everywhere, and neither of these can change a
    # build's output.
    ++ lib.optionals (builtins.getEnv "CI" == "") [
      "rust-analyzer"
      "rust-src" # rust-analyzer needs the std sources for go-to-definition
    ];
    # `crates/skema-wasm` compiles the core to WASM for the browser.
    #
    # The two *release* targets (musl, windows-gnu) deliberately do not live
    # here — see the `cross` profile at the bottom of this file.
    targets = [
      "wasm32-unknown-unknown"
    ];
  };

  packages = [
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

  # ---------------------------------------------------------------------------
  # cross – the release toolchain, off by default
  # ---------------------------------------------------------------------------
  # `devenv --profile cross-musl shell -- cargo build --target …`
  #
  # Only `build-release-binaries` (tags only) cross-compiles; fmt, clippy,
  # tests, coverage, audit, the wheel and the wasm bundle all build for the
  # host. Keeping the cross toolchain in the default shell made every one of
  # those jobs evaluate and realise two foreign stdenvs — a full mingw-w64
  # and a full musl64 GCC — to run `cargo test`.
  #
  # That is not merely slow. `pkgs.pkgsCross.mingwW64` drags nixpkgs' Windows
  # tree into the evaluation, and that tree adds plain source files (e.g.
  # `win-dll-link.sh`) to the store *during evaluation*. Those additions have
  # no GC root of their own, and on a store where one of them is missing the
  # evaluation dies with `path '/nix/store/…-win-dll-link.sh' is not valid`
  # long before a single test has run — which is exactly how `check` failed
  # in CI while it kept working on a developer machine whose store had the
  # file from an earlier evaluation.
  #
  # A profile is the smallest fix that keeps the cross setup declarative and
  # pinned: the jobs that need it ask for it, the jobs that don't never
  # evaluate it.
  #
  # One profile *per target*, not one for both: `build-release-binaries` is a
  # matrix and each of its jobs builds exactly one target, so a combined
  # profile made each job realise the other platform's GCC for nothing. With
  # five build-stage jobs unpacking their own container-local Nix store at
  # once, that surplus is what the runner ran out of disk on (pipeline 58).
  # It also keeps the musl job out of the Windows tree described above, so
  # the `win-dll-link.sh` hazard cannot reach it at all.
  profiles.cross-musl.module = {
    # musl links statically, so one Linux binary runs on every distribution
    # regardless of its glibc.
    languages.rust.targets = [ "x86_64-unknown-linux-musl" ];

    # Rust can *compile* for a target on its own; linking needs that
    # platform's toolchain.
    env.CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER =
      "${pkgs.pkgsCross.musl64.stdenv.cc}/bin/x86_64-unknown-linux-musl-cc";

    packages = [ pkgs.pkgsCross.musl64.stdenv.cc ];
  };

  profiles.cross-windows.module = {
    languages.rust.targets = [ "x86_64-pc-windows-gnu" ];

    env = {
      CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER =
        "${pkgs.pkgsCross.mingwW64.stdenv.cc}/bin/x86_64-w64-mingw32-cc";
      # Rust's windows-gnu target links against a static pthreads that the
      # mingw compiler does not carry itself.
      CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS =
        "-L ${pkgs.pkgsCross.mingwW64.windows.mcfgthreads}/lib -L ${pkgs.pkgsCross.mingwW64.windows.pthreads}/lib";
    };

    packages = [ pkgs.pkgsCross.mingwW64.stdenv.cc ];
  };
}
