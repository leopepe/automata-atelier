<!--
nixpkgs uses a PR template that is auto-applied when you open a PR via the
GitHub web UI or `gh pr create`. The headings below mirror that template;
do not rename them. Tick the boxes that apply by replacing `[ ]` with `[x]`.

Open the PR with `gh pr create --repo nixos/nixpkgs --base master --body-file PR_BODY.md`.
-->

## Description of changes

Initial submission of `uncharles` at version `0.1.0`.

`uncharles` is a sense → plan → act runtime that drives a Goal-Oriented Action Planner from a YAML config to automate shell tasks. It reads sensors (shell probes that read the world) and actions (shell commands with preconditions, effects, and a cost), plans the cheapest sequence to a declared goal, and either prints the plan or executes it step by step — re-sensing between steps so divergence triggers a replan rather than blind execution.

Upstream is a small Rust workspace (`grafo-dag`, `goap-planner`, `uncharles`); only the runnable CLI is being packaged here. Architecture decision tracking the Nix distribution is recorded upstream as [ADR 0004](https://github.com/leopepe/automata-atelier/blob/main/docs/adrs/0004-publish-uncharles-to-nixpkgs.md), and the implementation tracking issue is [leopepe/automata-atelier#52](https://github.com/leopepe/automata-atelier/issues/52).

- **Upstream release**: https://github.com/leopepe/automata-atelier/releases/tag/uncharles-v0.1.0
- **crates.io**: https://crates.io/crates/uncharles
- **License**: MIT

## Things done

- Built on platform(s)
  - [ ] x86_64-linux
  - [ ] aarch64-linux
  - [ ] x86_64-darwin
  - [ ] aarch64-darwin
- [ ] For non-Linux: Is `sandbox = true` set in `nix.conf`? (See [Nix manual](https://nixos.org/manual/nix/stable/command-ref/conf-file.html))
- [ ] Tested, as applicable:
  - [ ] NixOS test(s) for change in `nixos/` *(not applicable — leaf package, no module)*
  - [ ] and/or package tests for the package(s) added/changed
  - [ ] passthru.tests *(not applicable — leaf CLI)*
  - [ ] Tested compilation of all packages that depend on this change using `nix-shell -p nixpkgs-review --run "nixpkgs-review rev HEAD"` *(no reverse deps yet — first submission)*
- [ ] Tested basic functionality of all binary files (usually in `./result/bin/`)
  - Smoke test: `./result/bin/uncharles --help`, `./result/bin/uncharles --version`, and `./result/bin/uncharles inspect --config <example>` against one of the side-effect-free smoke configs from the upstream repo.
- [ ] 25.05 Release Notes *(not applicable — leaf package, no release-note-worthy change)*
- [ ] Fits [CONTRIBUTING.md](https://github.com/NixOS/nixpkgs/blob/master/CONTRIBUTING.md).

## Add a 👍 [reaction] to [pull requests you find important].

[reaction]: https://github.blog/2016-03-10-add-reactions-to-pull-requests-issues-and-comments/
[pull requests you find important]: https://github.com/NixOS/nixpkgs/pulls?q=is%3Aopen+sort%3Areactions-%2B1-desc
