{
  description = "hyprcwd-rs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        craneLib = crane.mkLib pkgs;

        rustMuslToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "x86_64-unknown-linux-musl" ];
        };
        craneLibMusl = (crane.mkLib pkgs).overrideToolchain rustMuslToolchain;
        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = [
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        hyprcwd = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
          }
        );

        muslArgs = commonArgs // {
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        };

        cargoArtifactsMusl = craneLibMusl.buildDepsOnly muslArgs;

        hyprcwd-static = craneLibMusl.buildPackage (
          muslArgs
          // {
            cargoArtifacts = cargoArtifactsMusl;
          }
        );
      in
      {

        packages = {
          default = hyprcwd;
          static = hyprcwd-static;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = hyprcwd;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            rust-analyzer
            rustfmt
            rustMuslToolchain
          ];
        };
      }
    );
}
