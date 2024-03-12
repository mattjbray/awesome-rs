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
      in {
        defaultPackage = naersk-lib.buildPackage {
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
              (with pkgs.darwin.apple_sdk.frameworks; [ AppKit CoreGraphics ])
            ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
      });
}
