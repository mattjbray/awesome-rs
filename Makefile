.PHONY: build
build:
	nix build '.?submodules=1' --extra-experimental-features 'nix-command flakes'
