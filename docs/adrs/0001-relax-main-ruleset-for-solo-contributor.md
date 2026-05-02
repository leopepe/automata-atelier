---
status: accepted
date: 2026-05-02
decision-makers: ["@leopepe"]
consulted: []
informed: []
---

# Relax `main` ruleset for solo-contributor workflow

## Context and Problem Statement

The repository's `main` branch was governed by a GitHub ruleset whose
`Require a pull request before merging` rule blocked PR #30 from being merged
even though all required CI checks were green and `requiredApprovingReviewCount`
was set to `0`. With CODEOWNERS listing a single owner (`* @leopepe`) and that
owner being the PR author, GitHub still treated the PR as `BLOCKED` because no
non-author reviewer existed. The bypass actor was set to `pull_request` mode,
forcing a deliberate "merge with bypass" click on every self-merge. This is the
first ADR for the workspace — it both establishes the ruleset baseline and
demonstrates the format adopted in [`../adrs.md`](../adrs.md). Refs PR #30.

## Decision Drivers

* This is a solo-maintained repository today; there is no second reviewer to
  satisfy a "Require a PR" rule, and adding one purely to satisfy automation is
  cargo-cult governance.
* The actual safety net for `main` is **CI** (`rustfmt`, `clippy`, `test`,
  `build (release)`) and **history shape** (linear history, no force-push, no
  deletion) — not the requirement that changes flow through a PR.
* Friction on every self-merge erodes the value of the PR-based discipline,
  because routing around the friction (bypass click, or skipping PRs) becomes
  easier than continuing to use it.
* If/when the project gains a second active maintainer, this decision should
  be revisited and likely reversed.

## Considered Options

* **Option A** — Keep the `pull_request` rule unchanged; bypass the rule on
  every self-merge.
* **Option B** — Keep the `pull_request` rule but change the bypass actor mode
  from `pull_request` to `always`, so the owner can self-merge silently.
* **Option C** — Remove the `pull_request` rule from the ruleset entirely,
  relying on CI checks and linear-history rules for protection.

## Decision Outcome

Chosen option: **"Option C — remove the `pull_request` rule"**, because for a
solo-maintained repository the rule's friction outweighs its protective value.
The remaining rules (`required_status_checks`, `required_linear_history`,
`non_fast_forward`, `deletion`, `creation`, `update`) cover the failure modes
that actually matter: broken `main`, force-pushed history, accidental branch
deletion. The PR-based workflow is preserved as a habit and remains the default
path; it is no longer a gate.

### Consequences

* Good, because self-authored PRs merge without bypass clicks once CI is green.
* Good, because the CI gate is now the single source of truth for "may this
  land on `main`?" — easier to reason about than a multi-rule chain.
* Good, because the ruleset is simpler and more maintainable.
* Bad, because direct pushes to `main` are now technically allowed (still
  rejected if they would create a non-linear history or fail CI on the push,
  but the PR-funnel is no longer enforced).
* Bad, because if a second contributor joins, the ruleset must be tightened
  again — this ADR will need to be superseded.

### Confirmation

Compliance is verified by inspecting the active ruleset:

```sh
gh api repos/leopepe/automata-atelier/rulesets/15865474 \
  --jq '.rules[] | .type'
```

Expected output (no `pull_request` line):

```
deletion
non_fast_forward
update
creation
required_linear_history
required_status_checks
```

The `required_status_checks` rule must continue to list `rustfmt`, `clippy`,
`test`, and `build (release)`.

## Pros and Cons of the Options

### Option A — keep the rule, bypass each merge

* Good, because it preserves the strictest possible posture without any
  configuration change.
* Bad, because every self-merge requires a bypass click — friction with no
  protective benefit when the bypass is always available to the same person.
* Bad, because bypass clicks accumulate as a blank "I overrode the rule"
  signal in the audit log, draining the signal value of an actual override.

### Option B — bypass actor in `always` mode

* Good, because it removes the per-merge friction while keeping the rule on
  paper.
* Bad, because the rule then exists only as a no-op for the one actor who
  matters; the configuration claims a posture it does not enforce.
* Bad, because if a second contributor joins, the bypass-actor mode would have
  to be tightened in lockstep with the rule — easy to forget.

### Option C — remove the rule (chosen)

* Good, because the configuration matches reality: the project relies on CI
  and linear-history rules, not on a PR funnel.
* Good, because there is one fewer rule to maintain or misconfigure.
* Neutral, because direct pushes to `main` are now permitted in principle.
  In practice the PR workflow remains the default and CI still gates the
  result.
* Bad, because reintroducing PR-funnel enforcement when a second contributor
  joins requires a deliberate re-tightening (see "More Information").

## More Information

* **Trigger to revisit**: when a second active maintainer joins the project,
  reopen this decision and likely supersede it with a new ADR that restores
  `pull_request` enforcement (with `requiredApprovingReviewCount: 1` and
  `require_code_owner_review: true`).
* **Audit pointer**: the ruleset is `repos/leopepe/automata-atelier/rulesets/15865474`,
  manageable via Settings → Rules → Rulesets → `main` in the GitHub UI.
* **Related**: PR #30 (which surfaced the friction) and the merge that landed
  the ADR scaffolding itself.
