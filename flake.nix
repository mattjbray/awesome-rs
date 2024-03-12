{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
        buildInputs = with pkgs.darwin.apple_sdk.frameworks; [
          AppKit
          CoreGraphics
        ];
      in {
        defaultPackage = naersk-lib.buildPackage {
          inherit buildInputs;
          src = ./.;
          gitSubmodules = true;
        };
        devShell = with pkgs;
          mkShell {
            buildInputs = [
              cargo
              iconv
              pre-commit
              rustPackages.clippy
              rustc
              rustfmt
              rust-analyzer
              buildInputs
            ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
      });
}
