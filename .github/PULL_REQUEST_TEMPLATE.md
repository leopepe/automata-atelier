## Summary

<!-- 1–3 bullets: what this PR does and why. Focus on the why; the diff shows the what. -->

-

## Conventional commit

<!-- The squash/merge commit message must follow Conventional Commits 1.0.0. The PR title should already match. -->

Type (check one):

- [ ] `feat` — new user-visible capability
- [ ] `fix` — bug fix
- [ ] `docs` — documentation only
- [ ] `style` — formatting, whitespace, no behaviour change
- [ ] `refactor` — internal change, no behaviour or interface change
- [ ] `perf` — performance improvement
- [ ] `test` — adding or fixing tests
- [ ] `build` — build system, dependencies, tooling
- [ ] `ci` — CI configuration
- [ ] `chore` — other non-source/non-test changes
- [ ] `revert` — reverts a prior commit

Scope (crate or area, e.g. `grafo`, `goap-planner`, `uncharles`, `ci`, `docs`):

`<scope>`

## Breaking changes

<!-- If breaking, the title must use `!` (e.g. `feat(grafo)!: ...`) AND include a `BREAKING CHANGE:` footer in the merge commit. Otherwise write "None". -->

None.

## Test plan

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Manual verification (describe):

## Linked issues

<!-- e.g. Closes #123, Refs #456 -->
