---
status: accepted
date: 2026-05-17
decision-makers: ["@leopepe"]
consulted: []
informed: []
---

# Publish `uncharles` to nixpkgs

## Context and Problem Statement

`uncharles` is currently distributable only via `cargo install` from source
or, once `.github/workflows/release.yml` is wired up, via crates.io. Nix
users who want the CLI either build from source themselves or maintain
out-of-tree overlays. This ADR settles **how `uncharles` is distributed to
Nix users**: the package definition's location, the builder it uses, and
the release trigger relative to the existing crates.io flow. The framing
constraint, supplied during design discussion, is that `uncharles` should
be installable by Nix users with the same ergonomics as any other "official"
tool — one command, no flake knowledge required, prebuilt binaries from a
trusted cache. The implementation of this ADR is tracked in
[issue #52](https://github.com/leopepe/automata-atelier/issues/52).

## Decision Drivers

* **"Official tool" install ergonomics.** The explicit user goal is that
  `uncharles` is installable as `nix-env -iA nixpkgs.uncharles` (or the
  flake-style equivalent through nixpkgs), discoverable on
  `search.nixos.org`, and serviced by the standard Nix toolchain without
  out-of-band cache configuration. Any channel that requires users to add
  a third-party flake input or `cachix use` line is a half-step on this
  axis.
* **Trusted binary cache, free of charge.** `cache.nixos.org` is the
  default substituter on every Nix install, signed and audited. Distribution
  through nixpkgs gets that for free; a self-managed flake does not.
* **Long-term stability of the install command.** A GitHub-flake URL is
  pinned to this repo's lifetime; the nixpkgs attribute path
  (`pkgs.uncharles`) survives forks, owner moves, and renames at the
  ecosystem level.
* **Workspace structure.** `uncharles` is one of three crates in a Cargo
  workspace with path dependencies on `grafo` and `goap-planner`. The
  nixpkgs derivation must handle workspace builds cleanly, including
  resolving the in-workspace deps without round-tripping through crates.io
  for them.
* **Existing release flow.** `release.yml` already encodes the order
  `grafo → goap-planner → uncharles` for crates.io publishing, gated by a
  `version` bump in `uncharles/Cargo.toml`. The nixpkgs release step is
  downstream of that bump and rides the same version number.
* **Layering and crate scope.** Only `uncharles` is a runnable artefact;
  `grafo` and `goap-planner` are libraries consumed via `Cargo.toml`. The
  Nix channel is `uncharles`-only.
* **Maintenance surface, explicitly traded.** Per-release nixpkgs PRs are
  slower and externally reviewed; this cost is consciously accepted in
  exchange for legitimacy as an official tool. This is a deliberate
  trade-off against ADR 0001's solo-contributor velocity stance.

## Considered Options

* **Option A — `flake.nix` in this repo, built with `crane`, released by
  extending `release.yml` and pushing to a Cachix binary cache.** Users
  install with `nix run github:leopepe/automata-atelier#uncharles` after
  `cachix use automata-atelier`.
* **Option B — `flake.nix` in this repo, built with
  `nixpkgs.buildRustPackage`, no binary cache.** Same flake-URL install
  command, no cache — users rebuild from source on first install.
* **Option C — Submit `uncharles` to nixpkgs.** Add the derivation at
  `pkgs/by-name/un/uncharles/package.nix` upstream. Users install with
  `nix-env -iA nixpkgs.uncharles` (legacy CLI) or
  `nix profile install nixpkgs#uncharles` (new CLI). Binaries served by
  `cache.nixos.org`. Each `uncharles` release requires a nixpkgs PR.
* **Option D — Do nothing; document `cargo install uncharles` and let Nix
  users assemble their own overlays.** No first-party Nix distribution.

## Decision Outcome

Chosen option: **"Option C — submit `uncharles` to nixpkgs as the sole Nix
distribution channel"**, because it is the only option that satisfies the
"official tool" driver without caveats. `nix-env -iA nixpkgs.uncharles`
needs no flake input, no Cachix configuration, no trust-on-first-use
prompt for a third-party cache, and surfaces on `search.nixos.org`
alongside every other tool a Nix user would reach for. The release-cadence
cost (nixpkgs PR per release, external review) and maintainership cost
(at least one nixpkgs maintainer listed in `meta.maintainers`) are
accepted as the price of legitimacy.

This is explicitly an **all-or-nothing** choice: there is no in-repo
`flake.nix` in v1, and no Cachix cache. The nixpkgs derivation is the only
Nix-facing artefact this repo ships. Options A and B remain in the design
space as fallbacks if Option C's costs prove unsustainable; until then,
running both channels in parallel is rejected because it splits the
"how do I install this?" answer and undercuts the "official" framing.

The release contract is:

* **Where the derivation lives.** Canonical copy in nixpkgs at
  `pkgs/by-name/un/uncharles/package.nix`. A mirror in this repo at
  `nix/package.nix` (or similar) carries the same derivation so CI can
  build it locally; the mirror is what gets copied into a nixpkgs PR.
  The mirror is the *test* copy, nixpkgs is the *shipped* copy; if they
  diverge, the nixpkgs copy wins until a follow-up sync.
* **Builder.** `rustPlatform.buildRustPackage` — the nixpkgs idiomatic
  builder, matching what every other Rust CLI in nixpkgs uses. Source
  fetched via `fetchFromGitHub` pinned to the release tag created by
  `release.yml`. Dependency resolution pinned via
  `cargoLock.lockFile = ./Cargo.lock;` (preferred over `cargoHash` because
  the workspace's path deps would otherwise need crates.io versions
  resolved in the vendoring step).
* **CI gate (this repo).** `ci.yml` gains a job that runs
  `nix-build nix/package.nix` (using `nixpkgs` from a pinned channel
  reference) on every PR. Catches derivation breakage before it reaches
  the nixpkgs PR queue.
* **Release trigger.** `release.yml` is extended after the existing
  `cargo publish -p uncharles` step with:
  1. Create and push a git tag `uncharles-v<version>` on the merge commit.
  2. Create a GitHub Release for that tag.
  3. Open (or auto-update) a nixpkgs PR bumping `version`, `rev`, and the
     `cargoLock` outputs. The first submission is opened by the maintainer
     by hand to absorb nixpkgs-style review feedback; subsequent updates
     rely on the existing `nixpkgs-update` bot once the package is in
     nixpkgs and properly tagged for it.
* **Maintainership.** At least one entry in `meta.maintainers` (the
  package owner). Listed via the standard `maintainers/maintainer-list.nix`
  process. If the maintainer goes inactive, the package is at risk of
  being marked broken; this is an accepted risk and a trigger to revisit.
* **`meta` surface.** Per nixpkgs conventions: `description`, `homepage`,
  `license = licenses.mit` (matches `uncharles/Cargo.toml`), `maintainers`,
  `mainProgram = "uncharles"`, `platforms = platforms.unix`.
* **Documentation.** `uncharles/README.md` gains an "Install via Nix"
  section once the first nixpkgs PR merges and propagates to
  `nixpkgs-unstable`. Until then, the section documents the timeline
  ("pending nixpkgs PR").
* **Out of scope for v1.** In-repo `flake.nix` (devshell or otherwise);
  Cachix or any self-managed binary cache; exposing `grafo` and
  `goap-planner` as separate nixpkgs entries (libraries, not tools);
  NixOS module wrapping `uncharles` as a system service; cross-channel
  backports (we ship to `nixpkgs-unstable`; stable channels pick up at
  channel cut, which is fine).

### Consequences

* Good, because the install command is `nix-env -iA nixpkgs.uncharles` or
  `nix profile install nixpkgs#uncharles`, one line, no clone, no compile,
  binaries from `cache.nixos.org` which is already trusted on every Nix
  install.
* Good, because `uncharles` becomes discoverable on `search.nixos.org` and
  shows up in the same listings as every other Nix-distributed tool. The
  "official tool" framing is delivered.
* Good, because nixpkgs handles multi-arch builds (`x86_64-linux`,
  `aarch64-linux`, `x86_64-darwin`, `aarch64-darwin`), cache signing, and
  channel branching. We inherit all of that without operating any of it.
* Good, because the channel choice is forward-compatible with becoming a
  Linux-distro citizen: nixpkgs presence is a prerequisite for landing in
  NixOS modules and is often a prerequisite for downstream packagers
  (e.g. Determinate's hosted Nix) to pick up a tool.
* Bad, because release cadence loses its tight coupling with crates.io
  publishing. There is a lag between a `cargo publish` succeeding and
  `nix-env -iA nixpkgs.uncharles` returning the new version — measured in
  hours to days for `nixpkgs-unstable` (after `nixpkgs-update` opens the
  PR and a reviewer merges it), and weeks for the stable channel after
  channel cut. We accept the lag; "official" is worth the slowdown.
* Bad, because the first nixpkgs submission goes through nixpkgs review:
  package naming, `meta` completeness, style conformity (`nixpkgs-fmt`,
  `statix`, `deadnix`), platform coverage, and a clean build on `ofborg`.
  This is a one-time tax with ongoing smaller follow-ups.
* Bad, because we are now a nixpkgs maintainer. If the package breaks
  upstream (e.g. a transitive dep is removed, the builder is rewritten),
  someone here has to respond. If we go inactive, the package risks
  being marked `broken` and eventually removed.
* Bad, because the `cargoLock` (or `cargoHash`) values need to be
  regenerated on every release. Forgetting this fails the nixpkgs build
  with a hash mismatch — a noisy failure mode, but recoverable.
* Bad, because this ADR consciously tensions ADR 0001's solo-contributor
  velocity stance. The cost is itemised; the choice is deliberate.
* Bad, because we lose the optionality of distributing pre-release builds
  through a flake URL. If we ever want "install the tip-of-main bleeding
  edge" as a Nix command, we will need to revisit Option A as a
  *complement*, not a replacement.

### Confirmation

Compliance is verified by four gates:

1. **`ci.yml` Nix build job** — `nix-build nix/package.nix` runs on every
   PR using a pinned `nixpkgs` reference (matching whatever branch we
   target for upstream submissions, typically `nixpkgs-unstable`).
   Failure blocks merge.
2. **`release.yml` nixpkgs-PR step** — for the first submission, the
   maintainer's manual PR is the gate (link captured in the GitHub
   Release notes for that version). For subsequent releases, success of
   the `nixpkgs-update` bot's auto-PR is the gate; a release where the
   bot fails to PR within 72 hours is surfaced as a release-incomplete
   state (workflow check, not silent).
3. **Smoke test post-merge** — `nix-shell -p uncharles --run 'uncharles
   --help'` on a fresh machine pinned to `nixpkgs-unstable` after the
   nixpkgs PR has merged and `cache.nixos.org` has built. Verifies the
   user-facing install command works end-to-end.
4. **`search.nixos.org` listing** — manually verified to show
   `uncharles` with the correct `meta` after the first submission lands.
   No automated gate; a one-time check captured in the rollout
   checklist.

## Pros and Cons of the Options

### Option A — in-repo flake + `crane` + Cachix

* Good, because release cadence stays under this repo's control.
* Good, because `crane`'s dep-caching makes CI iteration fast.
* Bad, because the install command requires either a flake URL or
  `cachix use` — neither is the "official" ergonomics target.
* Bad, because Cachix is a third-party vendor; trust-on-first-use is
  required and pricing/availability is out of our control.
* Bad, because `search.nixos.org` does not surface flake-only packages
  by default. Discoverability is weaker.

### Option B — in-repo flake + `buildRustPackage`, no cache

* Good, because it is the simplest flake possible.
* Bad, because there is no binary distribution: users rebuild from source
  on first install, same as `cargo install`. Fails the explicit goal.

### Option C — publish to nixpkgs (chosen)

* Good, because the install command is `nix-env -iA nixpkgs.uncharles` —
  the official Nix idiom.
* Good, because `cache.nixos.org` ships signed prebuilt binaries with no
  user configuration.
* Good, because `search.nixos.org`, NixOS module ecosystem, and stable
  channels are all unlocked by being in nixpkgs.
* Bad, because release cadence is gated on nixpkgs review (one-time for
  first submission, then automation-assisted but still external).
* Bad, because we inherit nixpkgs-maintainer responsibilities.
* Bad, because the `cargoLock` regeneration step is an extra release
  chore that, if skipped, breaks the build with a hash-mismatch error.

### Option D — do nothing

* Good, because zero new infrastructure.
* Bad, because there is no first-party Nix story. Users build from
  source or maintain ad-hoc overlays; "official tool" framing fails
  outright on the Nix side.

## More Information

* **Driving discussion**: in-conversation request to publish `uncharles`
  as an official Nix tool, no clone, no local build. Tracking issue:
  [#52](https://github.com/leopepe/automata-atelier/issues/52).
* **Implementation PR (planned, this repo)**: introduces
  `nix/package.nix` (the mirror), a Nix build job in `ci.yml`, the
  tag/release step in `release.yml`, and a placeholder install section
  in `uncharles/README.md`. Closes the issue this ADR refs.
* **Implementation PR (nixpkgs)**: separate PR against `nixos/nixpkgs`
  introducing `pkgs/by-name/un/uncharles/package.nix`, opened manually
  for the first submission. Link captured in the GitHub Release notes
  for the first tagged version.
* **Composes with `release.yml`'s existing TODOs**: the workflow header
  already calls out that workspace path deps need explicit `version`
  fields before crates.io will accept them. That work is a prerequisite
  for the nixpkgs derivation too, since `cargoLock.lockFile` resolves
  workspace deps through the same `Cargo.lock` cargo uses.
* **Composes with ADR 0001**: this ADR consciously trades the
  solo-contributor velocity stance of ADR 0001 for "official tool"
  ergonomics on the Nix axis. Other axes (crates.io, GitHub flow)
  retain the ADR 0001 posture; only the Nix channel adopts the slower,
  externally-reviewed release cadence.
* **Trigger to revisit**: nixpkgs review or `nixpkgs-update`-bot
  maintenance becomes prohibitive (PR backlog grows, reviewers
  unresponsive, repeated build breakage from upstream churn); or a
  concrete need for "install tip-of-main" emerges. At that point this
  ADR is *complemented* (not superseded) by adding Option A as a
  bleeding-edge channel alongside nixpkgs.
