{
  description = "Omegon — AI coding agent daemon and TUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    nix2container = {
      url = "github:nlewo/nix2container";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, nix2container, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        workspaceVersion = "0.15.26";

        commitSha =
          if self ? shortRev then self.shortRev
          else if self ? dirtyShortRev then self.dirtyShortRev
          else "unknown";

        # Crane source filtering — workspace root with crates under core/
        src = pkgs.lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = path: type:
            (craneLib.filterCargoSources path type)
            || builtins.match ".*\\.md$" path != null
            || builtins.match ".*\\.toml$" path != null
            || builtins.match ".*\\.json$" path != null
            || builtins.match ".*\\.pkl$" path != null;
        };

        commonArgs = {
          inherit src;
          pname = "omegon";
          strictDeps = true;
          buildInputs = with pkgs; [
            openssl.dev
            sqlite
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];
          nativeBuildInputs = with pkgs; [
            pkg-config
            perl       # required by openssl-sys build script
            cmake      # for libgit2-sys
          ];
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        omegon = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "-p omegon";
        });

        # Toolset profiles for container images
        profiles = import ./nix/profiles.nix { inherit pkgs; };

        # OCI images (Linux only)
        images = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux (
          import ./nix/oci.nix {
            inherit nix2container pkgs omegon profiles commitSha;
            version = workspaceVersion;
          }
        );
      in
      {
        packages = {
          default = omegon;
          omegon = omegon;
        } // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
          oci-chat = images.oci-chat;
          oci-coding = images.oci-coding;
          oci-coding-python = images.oci-coding-python;
          oci-coding-node = images.oci-coding-node;
          oci-coding-rust = images.oci-coding-rust;
          oci-infra = images.oci-infra;
          oci-full = images.oci-full;
        };

        # mkOmegonImage for custom compositions:
        #   nix build .#mkOmegonImage --override-input profiles '[base dev python]'
        lib = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
          inherit (images) mkOmegonImage;
          inherit profiles;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            cargo-watch
            cargo-zigbuild
            just
            sqlite
          ];
        };
      }
    );
}
