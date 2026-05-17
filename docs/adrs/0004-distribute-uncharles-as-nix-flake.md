---
status: proposed
date: 2026-05-17
decision-makers: ["@leopepe"]
consulted: []
informed: []
---

# Distribute `uncharles` as a Nix flake built with crane

## Context and Problem Statement

`uncharles` is currently distributable only via `cargo install` from source
or, once `.github/workflows/release.yml` is wired up, via crates.io. Nix users
who want to consume the CLI today either build it from source themselves or
shell out to a non-Nix toolchain. This ADR settles three coupled questions
about adding Nix as a first-class distribution channel: **where the package
definition lives**, **which Rust-on-Nix builder it uses**, and **what
"release" means** for the Nix artefact relative to the existing crates.io
release flow. An issue tracking this work is filed alongside this ADR — link
inserted on PR review.

## Decision Drivers

* **Solo-contributor velocity.** ADR 0001 already chose to relax `main`
  rulesets to keep iteration cheap for a single maintainer. Any Nix
  distribution path that adds external review (nixpkgs PRs gated on
  maintainer rotation) directly contradicts that posture for the steady-state
  release case.
* **Reproducibility and cache hits.** A Nix package whose every release
  rebuilds from source on each user's machine offers little over
  `cargo install`. The value of going Nix-native is hermetic builds and a
  binary cache; both must be addressed up front or the channel is
  unattractive.
* **Workspace structure.** `uncharles` is one of three crates in a Cargo
  workspace with path dependencies on `grafo` and `goap-planner`. The Nix
  builder must handle workspace builds cleanly, including vendoring or
  fetching dependencies without round-tripping through crates.io for the
  in-workspace deps.
* **Existing release flow.** `release.yml` already encodes the order
  `grafo → goap-planner → uncharles` and is gated by a `version` bump in
  `uncharles/Cargo.toml`. A Nix release that fires on a different signal
  (tag push, manual dispatch, file diff) splits the source of truth for
  "what version is current".
* **Maintenance surface.** Every distribution channel is something a future
  release has to remember to update. The Nix path should be observable from
  the existing release workflow, not a parallel system.
* **Layering and crate scope.** `uncharles` is the only artefact that makes
  sense as a runnable Nix package — `grafo` and `goap-planner` are libraries
  consumed via `Cargo.toml`. The Nix channel is `uncharles`-only; the lower
  crates are built transitively but not exposed as separate Nix packages.

## Considered Options

* **Option A — `flake.nix` in this repo, built with `crane`, released by
  extending `release.yml` and pushing to a Cachix binary cache.** The flake
  exposes `packages.uncharles`, `apps.uncharles`, and a `devShells.default`.
  CI builds it on every PR (gate) and pushes the closure to Cachix when the
  crates.io publish job succeeds.
* **Option B — `flake.nix` in this repo, built with `nixpkgs.buildRustPackage`,
  no binary cache.** Simpler `flake.nix`, no `crane` input, but every user
  rebuilds dependencies on first install and there is no caching of the
  workspace dependency closure between CI runs either.
* **Option C — Submit `uncharles` to nixpkgs.** Add `pkgs/by-name/un/uncharles/package.nix`
  upstream, ride nixpkgs' own release cadence. Each `uncharles` release
  requires a nixpkgs PR.
* **Option D — Do nothing; document `cargo install uncharles` and `nix run`
  against a flake template users copy themselves.** No first-party Nix
  distribution.

## Decision Outcome

Chosen option: **"Option A — `flake.nix` in-repo built with `crane`, released
by extending `release.yml` and pushing to Cachix"**, because it is the only
option that satisfies every driver simultaneously: it gives users a
zero-friction `nix run github:leopepe/automata-atelier#uncharles`, keeps
release cadence under this repo's sole control (no external maintainer
review), reuses the existing version-bump signal so there is one source of
truth for "what's released", and amortises the workspace's dependency build
through `crane`'s dep-only derivation so neither CI nor users pay the cost
twice. Cachix turns the channel from "Nix-flavoured `cargo install`" into a
real binary distribution.

The release contract is:

* **Package surface.** The flake exposes `packages.uncharles` (the binary),
  `apps.uncharles` (for `nix run`), and `devShells.default` (a Rust toolchain
  + workspace deps for contributors). `grafo` and `goap-planner` are built
  transitively as dependencies of `uncharles` and are **not** exposed as
  separate package outputs in v1; if a Nix consumer wants them as libraries,
  they go through `Cargo.toml` like everyone else.
* **Builder.** `crane` (with `rust-overlay` for toolchain pinning, sourced
  from the workspace's `rust-toolchain.toml` once that file exists; until
  then, pinned in the flake). `crane`'s split between a `cargoArtifacts`
  derivation (workspace deps) and the final crate build is what makes CI
  iteration affordable.
* **Reproducibility.** `flake.lock` is committed. The Rust toolchain is
  pinned (either via `rust-toolchain.toml` or in the flake). `Cargo.lock` is
  the input to `crane`'s vendor step — the same lockfile that drives
  `cargo build` drives the Nix build.
* **CI gate.** `ci.yml` gains a `nix flake check` + `nix build .#uncharles`
  job that runs on every PR. Breakage in the Nix path is caught at PR time,
  not at release time.
* **Release trigger.** `release.yml` is extended with a `publish-nix` job
  that runs **after** the existing `cargo publish -p uncharles` step
  succeeds (or after its dry-run, when `dry-run=true`). It builds
  `packages.uncharles`, pushes the closure to the Cachix cache, and attaches
  the store path + cache URL to a GitHub Release whose tag matches the
  crates.io version. The tag itself is created by the release job, not
  pre-existing — keeping the trigger inside the workflow keeps "what
  version is current" gated on a single workflow run.
* **Cache.** A Cachix cache named `automata-atelier` (or similar — final
  name decided at implementation time). The signing key is stored as the
  repo secret `CACHIX_AUTH_TOKEN`. The cache URL and public key are
  documented in `uncharles/README.md` so users can opt in with one
  `cachix use` command.
* **Documentation.** `uncharles/README.md` gains a short "Install via Nix"
  section with `nix run github:leopepe/automata-atelier#uncharles --` plus
  the Cachix configuration line. `flake.nix` carries a header comment
  pointing back at this ADR.
* **Out of scope for v1.** Submitting to nixpkgs (Option C remains
  available as a follow-up once the flake stabilises and a tagged release
  has shipped); exposing `grafo` and `goap-planner` as separate Nix
  packages; macOS-specific or aarch64-specific binaries beyond what
  `crane`'s default `flake-utils.lib.eachDefaultSystem` provides;
  reproducibility audits beyond `flake.lock` pinning.

### Consequences

* Good, because users get `nix run github:leopepe/automata-atelier#uncharles`
  with one command and (after `cachix use`) a binary install rather than a
  source rebuild.
* Good, because the release trigger is the same `uncharles` version bump
  that already gates crates.io publishing. There is one version number, one
  release workflow, and one GitHub Release per release.
* Good, because `crane`'s cargo-artifacts derivation means CI's Nix job
  caches the entire workspace dep build between runs, so the per-PR Nix
  gate is fast (seconds, not minutes) once warmed.
* Good, because the choice is reversible. The flake is additive; if a
  future ADR decides nixpkgs is the right channel, the flake stays as the
  source the nixpkgs derivation pins against.
* Bad, because the workspace gains a new toolchain (Nix) that contributors
  may need locally to debug release failures. We mitigate this by keeping
  the flake invokable through `nix run` without a system-wide Nix install
  (Nix's own installer is enough) and by documenting the manual reproducer
  in the release docs.
* Bad, because Cachix is a third-party service. If it goes down or pricing
  changes adversely, the Nix channel degrades to "Nix-flavoured `cargo
  install`" until we migrate to a self-hosted cache (e.g. `nix-serve` on
  GitHub Pages or a Garage instance). We accept that risk in v1; Cachix's
  free tier is sufficient for current artefact sizes.
* Bad, because adding a Nix gate to `ci.yml` lengthens the PR feedback loop
  on cold cache (first build per branch). The warm-cache cost is small but
  non-zero.
* Bad, because users who consume the flake without configuring Cachix will
  rebuild from source on first install — the same cost as `cargo install`.
  Documentation needs to make the Cachix step prominent or that user gets
  the worst of both worlds.

### Confirmation

Compliance is verified by four gates:

1. **`ci.yml` Nix job** — `nix flake check` and `nix build .#uncharles` run
   on every PR. Failure blocks merge to `main`.
2. **`release.yml` Nix job** — the `publish-nix` step's success is required
   for a release to be considered complete. A crates.io publish that is
   not followed by a successful Cachix push is a release-incomplete state
   to be surfaced (workflow failure, not silent).
3. **Manual smoke test on release** — `nix run github:leopepe/automata-atelier/v<version>#uncharles -- --help` from a
   clean machine without the Cachix cache configured, then again with it
   configured. The first proves the source build works; the second proves
   the cache is reachable.
4. **`uncharles/README.md` install section** — kept in sync with the actual
   flake output names. A PR that renames `packages.uncharles` must update
   the README in the same commit.

## Pros and Cons of the Options

### Option A — in-repo flake + `crane` + Cachix (chosen)

* Good, because release cadence stays under this repo's control: bump
  version, push, the workflow does the rest.
* Good, because `crane`'s dep-caching derivation matches the workspace
  shape exactly — `cargoExtraArgs = "-p uncharles"` is a one-liner.
* Good, because `flake.lock` + a pinned toolchain give bit-reproducibility
  without ceremony.
* Bad, because Cachix is a vendor dependency.
* Bad, because the flake adds a Nix learning curve for contributors who
  ever need to debug a release-time Nix failure.

### Option B — in-repo flake + `buildRustPackage`, no cache

* Good, because the flake is small (no `crane` input).
* Good, because `buildRustPackage` is the nixpkgs-default builder, which
  eases a future Option-C migration.
* Bad, because there is no shared dep-cache derivation: every workspace
  source change rebuilds every dep in CI. Per-PR Nix gate becomes minutes,
  not seconds.
* Bad, because without a binary cache there is no real distribution
  story — users rebuild from source, which is what `cargo install` already
  does.

### Option C — submit to nixpkgs

* Good, because users get `nix-env -iA nixpkgs.uncharles` with no flake
  knowledge required.
* Good, because nixpkgs handles the binary cache (cache.nixos.org) for
  free.
* Bad, because every release requires a separate nixpkgs PR with external
  maintainer review. Release latency is no longer under our control.
* Bad, because nixpkgs has its own version-bump cadence and "stable
  channel" semantics; reconciling those with this repo's release
  workflow needs ongoing attention.
* Bad, because nixpkgs review surface (style, naming, lint) is a long
  feedback loop for the first submission.
* Neutral, because this option is **not** foreclosed by Option A; the
  flake can serve as the source of truth that the nixpkgs derivation
  pins.

### Option D — do nothing

* Good, because zero new infrastructure.
* Bad, because Nix users continue to either build from source or maintain
  their own out-of-tree overlay. There is no first-party Nix story.
* Bad, because each user's overlay drifts from the others; bug reports
  become "which overlay are you using?" before they become actionable.

## More Information

* **Driving discussion**: in-conversation request to add a Nix distribution
  channel with CI-driven releases. A `type:design` issue is filed alongside
  this ADR's PR; link inserted on review.
* **Implementation PR (planned)**: introduces `flake.nix`, `flake.lock`,
  a Nix job in `ci.yml`, a `publish-nix` job in `release.yml`, and the Nix
  install section in `uncharles/README.md`. Closes the issue this ADR
  refs.
* **Composes with `release.yml`'s existing TODOs**: the workflow header
  already calls out that workspace path deps need explicit `version`
  fields before crates.io will accept them. That work is a prerequisite
  for Option A's release trigger too, since `crane`'s vendor step relies
  on a well-formed `Cargo.lock` and the path deps' resolved versions.
* **Trigger to revisit**: a first concrete request for "I want
  `goap-planner` as a Nix library" (re-evaluates the v1 single-package
  scope), or sustained pain with Cachix availability/cost (re-evaluates
  the cache choice), or a contributor stepping forward to maintain a
  nixpkgs entry (re-opens Option C as a *complement* to the flake, not a
  replacement).
