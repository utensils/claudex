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

          commonArgs = {
            inherit src;
            pname = "claudex";
            version = "0.1.0";
            strictDeps = true;
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
          };

          devshells.default = {
            motd = ''
              {202}claudex{reset} — query, search, and analyze Claude Code sessions ({bold}${system}{reset})
              $(type menu &>/dev/null && menu)
            '';

            packages = [
              rustToolchain
              pkgs.rust-analyzer
              pkgs.git
              pkgs.gh
              pkgs.jq
            ];

            env = [
              {
                name = "RUST_BACKTRACE";
                value = "1";
              }
            ];

            commands = [
              {
                category = "build";
                name = "build";
                help = "cargo build (debug)";
                command = "cargo build \"$@\"";
              }
              {
                category = "build";
                name = "build-release";
                help = "cargo build --release";
                command = "cargo build --release \"$@\"";
              }
              {
                category = "check";
                name = "check";
                help = "cargo check";
                command = "cargo check \"$@\"";
              }
              {
                category = "check";
                name = "clippy";
                help = "cargo clippy -- -D warnings (matches CI)";
                command = "cargo clippy \"$@\" -- -D warnings";
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
                help = "cargo test (matches CI)";
                command = "cargo test \"$@\"";
              }
              {
                category = "check";
                name = "ci-local";
                help = "run the same sequence CI runs: fmt-check, check, clippy, test, build";
                command = ''
                  set -euo pipefail
                  cargo fmt --all -- --check
                  cargo check
                  cargo clippy -- -D warnings
                  cargo test
                  cargo build --release
                '';
              }
              {
                category = "run";
                name = "claudex";
                help = "run claudex";
                command = "cargo run -- \"$@\"";
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
