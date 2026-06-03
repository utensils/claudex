{
  description = "claudex — query, search, and analyze Claude Code sessions";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.devshell.flakeModule
        inputs.treefmt-nix.flakeModule
      ];

      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      perSystem =
        {
          system,
          lib,
          ...
        }:
        let
          pkgs = import inputs.nixpkgs {
            localSystem = system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rustfmt"
              "clippy"
            ];
          };

          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

          src = craneLib.path ./.;

          darwinInputs = lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];

          # Cargo manifests are the source of truth for package metadata; the
          # root workspace carries the shared version, while the CLI crate owns
          # the published binary package description.
          workspaceToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
          cliCargoToml = builtins.fromTOML (builtins.readFile ./crates/claudex-cli/Cargo.toml);

          claudexMeta = {
            description = cliCargoToml.package.description;
            homepage = workspaceToml.workspace.package.homepage;
            license = lib.licenses.mit;
            mainProgram = "claudex";
            maintainers = [
              {
                name = "James Brink";
                email = "brink.james@gmail.com";
                github = "jamesbrink";
                githubId = 28793;
              }
            ];
            platforms = lib.platforms.unix;
          };

          commonArgs = {
            inherit src;
            pname = "claudex";
            version = workspaceToml.workspace.package.version;
            strictDeps = true;
            buildInputs = darwinInputs;
            meta = claudexMeta;
            cargoExtraArgs = "-p claudex-cli --bin claudex";
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            # On Darwin the Xcode clang invoked by crane can't find Nix-provided
            # libiconv without an explicit library path. Set both so the rustc
            # link step and bindgen's clang find the library.
            LIBRARY_PATH = "${pkgs.libiconv}/lib";
            NIX_LDFLAGS = "-L${pkgs.libiconv}/lib";
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          claudex = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
            }
          );
        in
        {
          _module.args.pkgs = pkgs;

          packages = {
            inherit claudex;
            default = claudex;
          };

          apps.default = {
            type = "app";
            program = "${claudex}/bin/claudex";
            meta = claudexMeta;
          };

          devshells.default = {
            motd = ''
              {202}claudex{reset} — query, search, and analyze Claude Code sessions ({bold}${system}{reset})
              $(type menu &>/dev/null && menu)
            '';

            packages = [
              rustToolchain
              pkgs.rust-analyzer
              pkgs.cargo-llvm-cov
              pkgs.git
              pkgs.gh
              pkgs.jq
              # Docs site (VitePress, Tailwind v4, Vue 3) lives in ./website.
              # prettier is a devDependency installed by `bun install`.
              pkgs.bun
            ]
            ++ darwinInputs;

            env = [
              {
                name = "RUST_BACKTRACE";
                value = "1";
              }
            ]
            ++ lib.optionals pkgs.stdenv.isDarwin [
              {
                name = "LIBRARY_PATH";
                value = "${pkgs.libiconv}/lib";
              }
            ];

            commands = [
              {
                category = "build";
                name = "build";
                help = "cargo build -p claudex-cli --bin claudex (debug)";
                command = "cargo build -p claudex-cli --bin claudex \"$@\"";
              }
              {
                category = "build";
                name = "build-release";
                help = "cargo build --release -p claudex-cli --bin claudex";
                command = "cargo build --release -p claudex-cli --bin claudex \"$@\"";
              }
              {
                category = "check";
                name = "check";
                help = "cargo check --workspace --all-targets";
                command = "cargo check --workspace --all-targets \"$@\"";
              }
              {
                category = "check";
                name = "clippy";
                help = "cargo clippy --workspace --all-targets -- -D warnings (matches CI)";
                command = "cargo clippy --workspace --all-targets \"$@\" -- -D warnings";
              }
              {
                category = "check";
                name = "fmt";
                help = "cargo fmt";
                command = "cargo fmt \"$@\"";
              }
              {
                category = "check";
                name = "fmt-check";
                help = "cargo fmt --check (matches CI)";
                command = "cargo fmt --check \"$@\"";
              }
              {
                category = "check";
                name = "run-tests";
                help = "cargo test --workspace (matches CI)";
                command = "cargo test --workspace \"$@\"";
              }
              {
                category = "check";
                name = "ci-local";
                help = "run the same sequence CI runs: fmt-check, check, clippy, test, build";
                command = ''
                  set -euo pipefail
                  cargo fmt --all -- --check
                  cargo check --workspace --all-targets
                  cargo clippy --workspace --all-targets -- -D warnings
                  cargo test --workspace
                  cargo build --release -p claudex-cli --bin claudex
                '';
              }
              {
                category = "check";
                name = "coverage";
                help = "test coverage summary (pass --html for a browsable report)";
                command = ''
                  set -euo pipefail
                  # llvm-cov/llvm-profdata ship with the rustc toolchain but
                  # aren't on PATH; point cargo-llvm-cov at them explicitly.
                  LLVM_COV="$(find /nix/store -maxdepth 3 -name llvm-cov 2>/dev/null | head -1)"
                  LLVM_PROFDATA="$(find /nix/store -maxdepth 3 -name llvm-profdata 2>/dev/null | head -1)"
                  export LLVM_COV LLVM_PROFDATA
                  if [ "''${1:-}" = "--html" ]; then
                    cargo llvm-cov --workspace --html --output-dir target/coverage
                    echo "Report: target/coverage/html/index.html"
                  else
                    cargo llvm-cov --workspace --summary-only
                  fi
                '';
              }
              {
                category = "run";
                name = "claudex";
                help = "run claudex";
                command = "cargo run -p claudex-cli --bin claudex -- \"$@\"";
              }
              {
                category = "docs";
                name = "docs-dev";
                help = "start the VitePress dev server for the docs site";
                command = "cd website && bun install && bun run dev \"$@\"";
              }
              {
                category = "docs";
                name = "docs-build";
                help = "build the documentation site (static output in website/.vitepress/dist)";
                command = "cd website && bun install && bun run build";
              }
              {
                category = "docs";
                name = "docs-preview";
                help = "preview the built documentation site";
                command = "cd website && bun run preview \"$@\"";
              }
              {
                category = "docs";
                name = "docs-fmt";
                help = "format documentation with prettier";
                command = "cd website && bun run fmt";
              }
              {
                category = "docs";
                name = "docs-fmt-check";
                help = "check documentation formatting (matches CI)";
                command = "cd website && bun run fmt:check";
              }
            ];
          };

          treefmt = {
            projectRootFile = "flake.nix";
            programs.nixfmt.enable = true;
            programs.rustfmt = {
              enable = true;
              edition = "2024";
            };
          };
        };
    };
}
