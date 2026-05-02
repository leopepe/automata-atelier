# Issues

How to file and triage issues in this workspace. Pull requests follow a separate set of rules — see [`docs/coding-guidelines.md`](./coding-guidelines.md) under **Commits and pull requests** and the template at [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md) for those.

## When to open an issue

Open an issue when:

- A behaviour is broken or has regressed → **bug report**.
- A new user-visible capability is wanted → **feature request**.
- A design discussion needs durable capture (architectural choices, API shape, "we should reconsider X") → **design proposal**. Once such a discussion reaches a decision that meets the bar in [`adrs.md`](./adrs.md), record it as an ADR and reference the issue with `Refs #N`.

If a thing is already a tiny PR you're about to open and the discussion fits in the PR description, skip the issue. Issues exist for things that need to be discussed, prioritised, or carried across multiple PRs.

## Issue templates

Three templates live under [`.github/ISSUE_TEMPLATE/`](../.github/ISSUE_TEMPLATE/):

| Template | When to use |
|---|---|
| [`bug_report.md`](../.github/ISSUE_TEMPLATE/bug_report.md) | Broken or regressed behaviour |
| [`feature_request.md`](../.github/ISSUE_TEMPLATE/feature_request.md) | New user-visible capability |
| [`design_proposal.md`](../.github/ISSUE_TEMPLATE/design_proposal.md) | Design discussion / proposal |

Each template fills in starter labels (`type:*` plus `status:triage`) automatically. Don't open blank issues — `blank_issues_enabled` is off via [`.github/ISSUE_TEMPLATE/config.yml`](../.github/ISSUE_TEMPLATE/config.yml).

## Labels

The label set is small and orthogonal: every issue gets exactly one **type**, one or more **scopes**, optionally a **priority**, and exactly one **status**. Labels are managed via the GitHub UI (or `gh label create`); the canonical list is the one below.

### Type (one per issue, applied by the template)

- `type:bug` — broken or regressed behaviour
- `type:feature` — new user-visible capability
- `type:design` — design discussion / proposal
- `type:docs` — documentation-only changes
- `type:infra` — CI, build, dev tooling

### Scope (one or more per issue)

- `scope:grafo` — the graph kernel
- `scope:goap-planner` — the GOAP planner
- `scope:uncharles` — the runtime CLI
- `scope:ci` — CI / GitHub workflows
- `scope:docs` — workspace documentation

### Priority (zero or one per issue)

- `priority:high` — blocking real work; needs to land soon
- `priority:low` — would be nice; low urgency
- *(unlabelled)* — normal priority, the default

There is deliberately no `priority:medium`. Normal priority is the default and should not need a label.

### Status (one per issue, lifecycle)

- `status:triage` — newly opened, not yet reviewed (default from templates)
- `status:accepted` — agreed worth doing, ready to be picked up
- `status:blocked` — waiting on something external (decision, dependency, upstream change)

When work is in progress, the convention is to **assign** the issue rather than label it — the assignee is the signal. When the issue is closed, no status label is needed; the closed/open state and the closing PR are the trail.

### Special

- `nice-to-have` — opportunistic; would be nice but no real consumer is driving it
- `good-first-issue` — small, well-scoped, low-context entry points for new contributors
- `help-wanted` — explicit invitation for someone outside the regular maintainer set to take it
- `bench:allow-regression` — applied to **PRs only**, not issues. Downgrades the bench-regression CI gate from failing to a warning. See [`docs/performance-tests.md`](./performance-tests.md).

## Lifecycle

```
opened (template applies type:* + status:triage)
  │
  ▼
triage  ──▶ close (won't fix / duplicate / out of scope)
  │
  ▼
accepted (status:triage → status:accepted; scope and priority labels added)
  │
  ▼
in progress (someone is assigned; issue stays open and accepted)
  │
  ▼
closed by a merging PR (use "Closes #N" in the PR body)
```

A `status:blocked` issue stays open with a comment explaining what it is waiting for. When unblocked, drop `status:blocked` and either re-triage or move it back to `status:accepted`.

## Linking issues and PRs

In the PR description (or commit body), use:

- `Closes #123` — auto-closes the issue when the PR merges. Use this when the PR fully resolves the issue.
- `Refs #123` — references the issue without auto-closing. Use this for partial progress, related work, or context.

Multiple references in one PR are fine: `Closes #1, Closes #2, Refs #3`.

## Examples

| Scenario | Template | Labels |
|---|---|---|
| A typo in `goap-planner`'s public API documentation | bug | `type:bug`, `scope:goap-planner`, `scope:docs` |
| Add a `--watch` flag for long-lived configs | feature | `type:feature`, `scope:uncharles` |
| Add `forbids` to `ActionSpec` for negative preconditions | design | `type:design`, `scope:goap-planner` |
| Parallelise the bench-base / bench-head matrix jobs | design or feature | `type:infra`, `scope:ci`, `priority:low` |
| Document the bench-gate fragility follow-up | design | `type:design`, `scope:ci` |
