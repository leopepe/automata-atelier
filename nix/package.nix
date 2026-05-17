# Nix derivation for the `uncharles` CLI.
#
# Per ADR 0004 (docs/adrs/0004-publish-uncharles-to-nixpkgs.md), this file is
# the in-repo mirror of the derivation that ships in nixpkgs at
# `pkgs/by-name/un/uncharles/package.nix`. It is built on every PR by
# `ci.yml`'s `nix-build` job, against the current workspace source, to catch
# derivation breakage before it reaches the nixpkgs PR queue.
#
# Two parameters exist so the file's *default shape* matches what nixpkgs
# expects, while `nix/ci.nix` can override them to build the local checkout:
#
#   * `src` — defaults to `fetchFromGitHub` pinned to a release tag.
#   * `cargoLockFile` — defaults to a `./Cargo.lock` co-located with this
#     file, matching the nixpkgs convention of copying the upstream lockfile
#     next to `package.nix`. This default file does not exist in this repo
#     (the workspace lockfile lives at `../Cargo.lock`); `nix/ci.nix`
#     overrides this parameter so `nix-build nix/ci.nix` works without a
#     copy. A direct `nix-build nix/package.nix` would fail until a lockfile
#     is co-located — which is exactly what happens when this file is copied
#     into nixpkgs.
#
# When submitting (or updating) the nixpkgs derivation, copy this file to
# `pkgs/by-name/un/uncharles/package.nix`, copy the workspace's `Cargo.lock`
# next to it, drop the `src` parameter so the default `fetchFromGitHub`
# applies, and replace `lib.fakeHash` with the real source hash
# (`nix-prefetch-github` against the release tag).

{
  lib,
  rustPlatform,
  fetchFromGitHub,
  src ? fetchFromGitHub {
    owner = "leopepe";
    repo = "automata-atelier";
    rev = "uncharles-v${version}";
    hash = lib.fakeHash;
  },
  version ? "0.1.0",
  cargoLockFile ? ./Cargo.lock,
}:

rustPlatform.buildRustPackage {
  pname = "uncharles";
  inherit src version;

  cargoLock.lockFile = cargoLockFile;

  cargoBuildFlags = [ "-p" "uncharles" ];
  cargoTestFlags = [ "-p" "uncharles" ];

  meta = {
    description = "Sense → plan → act runtime that drives goap-planner from a YAML config to automate shell tasks";
    homepage = "https://github.com/leopepe/automata-atelier";
    license = lib.licenses.mit;
    mainProgram = "uncharles";
    platforms = lib.platforms.unix;
    # `maintainers` is populated when the nixpkgs PR lands; the maintainer
    # entry has to exist in `maintainers/maintainer-list.nix` first.
  };
}
