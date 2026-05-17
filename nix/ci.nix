# Local / CI entry point for building the `uncharles` Nix derivation.
#
# `nix-build nix/ci.nix` builds `package.nix` against the current workspace
# (overriding the release-tag default for `src` and the co-located default
# for `cargoLockFile`). Used by `.github/workflows/ci.yml` on every PR and
# by developers iterating on the derivation locally.
#
# Nixpkgs is taken from the channel the GitHub Action sets up
# (`nixos-unstable` in `ci.yml`); for local invocations without a configured
# channel, `<nixpkgs>` falls back to whatever the user's `nix-channel`
# points at.

{ pkgs ? import <nixpkgs> { } }:

pkgs.callPackage ./package.nix {
  src = pkgs.lib.cleanSource ../.;
  cargoLockFile = ../Cargo.lock;
}
