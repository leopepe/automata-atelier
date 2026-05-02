# `docs/` — workspace documentation index

This directory holds workspace-wide documentation that applies across every
crate (`grafo`, `goap-planner`, `uncharles`). Treat it as the first place to
look when you need to know "how do we do X here?".

## Read before coding

**Whenever you are about to write or modify code in this workspace, read
[`coding-guidelines.md`](./coding-guidelines.md) first.** It is the source of
truth for tests, lint and format gates, documentation duties, commit and PR
conventions, and the "when in doubt" defaults. Per-crate `CLAUDE.md` files may
add stricter rules on top of those guidelines but never weaken them.

If a change touches a core library's hot path (currently `grafo` or
`goap-planner`), also read
[`performance-tests.md`](./performance-tests.md) before starting — it defines
the required benchmark surface, the workflow, and what CI enforces.

If you are about to open or update a GitHub issue, read
[`issues.md`](./issues.md) for the templates, labels, and lifecycle.

## What lives in this directory

| File | Purpose |
| --- | --- |
| [`coding-guidelines.md`](./coding-guidelines.md) | Workspace coding rules: tests, lint/format, docs, commits, PRs. Read before any code change. |
| [`performance-tests.md`](./performance-tests.md) | Benchmark suite requirements for core libraries; flamegraphs, perf comparisons, CI. |
| [`issues.md`](./issues.md) | How to file and triage GitHub issues: templates, labels, linking to PRs. |
| `CLAUDE.md` | This index. |

When new workspace-wide documentation is added, list it here so it is
discoverable from the index and from the root `CLAUDE.md`.
