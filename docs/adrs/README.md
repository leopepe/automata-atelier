# ADR index

Every architectural decision record (ADR) in this workspace, listed by number.
This file is the canonical index — keep it in sync with the filesystem in the
same commit that adds, supersedes, or deprecates an ADR.

For the format, lifecycle, and "when to write one" rules see
[`../adrs.md`](../adrs.md). To start a new ADR, copy
[`0000-template.md`](./0000-template.md) to `NNNN-<kebab-title>.md` using the
next free number.

## Records

| #    | Title | Status | Date |
| ---- | ----- | ------ | ---- |
| 0001 | [Relax `main` ruleset for solo-contributor workflow](./0001-relax-main-ruleset-for-solo-contributor.md) | accepted | 2026-05-02 |
| 0002 | [Use uncharles as a post-apply IaC convergence watcher, not a CI replacement](./0002-uncharles-as-post-apply-iac-convergence-watcher.md) | accepted | 2026-05-03 |
| 0003 | [Value-carrying facts live in the uncharles runtime, not in goap-planner's State](./0003-value-carrying-facts-runtime-side.md) | accepted | 2026-05-03 |
| 0004 | [Publish `uncharles` to nixpkgs](./0004-publish-uncharles-to-nixpkgs.md) | accepted | 2026-05-17 |

## Status legend

- `proposed` — drafted, not yet ratified.
- `accepted` — ratified; current behaviour of the workspace.
- `deprecated` — discouraged but not yet removed.
- `superseded` — replaced by a newer ADR (linked via `superseded-by`).
