.PHONY: build
build:
	nix build '.?submodules=1'
