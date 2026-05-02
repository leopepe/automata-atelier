---
name: adr
description: >
  Draft, propose, or update an Architecture Decision Record (ADR) for this
  workspace. Triggers AUTOMATICALLY whenever the conversation is taking — or
  has just taken — a decision that touches: architecture or layering between
  crates (grafo, goap-planner, uncharles); core components (graph kernel,
  planner, executor) being introduced, swapped, or significantly reshaped;
  performance constraints (algorithm choice, data structure, allocation
  strategy, sync vs async vs rayon, benchmark gate thresholds); security
  posture (trust boundaries, auth, secret handling, sandboxing, supply chain);
  user interface (CLI verbs, resources, flags, output formats, exit codes
  beyond what coding-guidelines.md already covers); integration interface
  (YAML/config schema, HTTP/gRPC contract, plugin or extension API); or
  operability (logging, metrics, signal handling, replan triggers, dry-run
  semantics) — anything that changes how users or developers operate the
  system; or guidelines / process (adding, removing, or materially changing a
  workspace rule under docs/ or any CLAUDE.md, the PR / issue templates, the
  label scheme, the commit-convention policy, the lint / format toolchain, or
  CI gate semantics — but NOT typo fixes or pure rewording of an existing
  rule). Also use when the user explicitly says "ADR", "decision record",
  "let's record this decision", "supersede ADR NNNN", or invokes /adr.
  Do NOT invoke for routine bug fixes, internal refactors that do not change a
  public boundary, doc edits that only clarify or fix typos in an existing
  rule, dependency bumps without semantic change, or test additions.
tools: Read, Write, Edit, Bash, Glob, Grep
---

# adr — draft or update an Architecture Decision Record

This skill captures architectural decisions in the canonical format used by
this workspace (MADR 4.0.0). It is bound by — and must follow —
[`docs/adrs.md`](../../../docs/adrs.md). Read that file first; this skill is a
runner, not a substitute for the guideline.

## When to invoke

Invoke the skill when, in the current conversation, a decision is being or has
been taken in any of these categories:

- **Architecture / layering** — boundaries between `grafo`, `goap-planner`,
  `uncharles`; introduction or removal of a crate; foundational pattern choice.
- **Core components** — which library underpins a kernel responsibility (graph
  search, planner, executor, parser).
- **Performance** — algorithm, data structure, allocation strategy, concurrency
  model (sync / `async` / `rayon`), benchmark gate thresholds.
- **Security** — trust boundaries, auth, secret handling, sandbox of executed
  actions, supply-chain controls.
- **User interface** — CLI verb/resource grammar changes, output format
  additions, `--help` semantics, exit-code policy beyond
  `coding-guidelines.md`'s baseline.
- **Integration interface** — YAML/config schema, HTTP/gRPC contract, plugin
  or extension API.
- **Operability** — logging, metrics, signal handling, replan triggers, dry-run
  semantics.
- **Guidelines and process** — adding, removing, or materially changing a
  workspace rule. Includes `docs/coding-guidelines.md`,
  `docs/performance-tests.md`, `docs/issues.md`, `docs/adrs.md`, any
  `CLAUDE.md`, the PR / issue templates, the label scheme, branch-protection
  rules, the commit-convention policy, the lint or format toolchain, and the
  CI gate semantics.

Also invoke when the user explicitly asks for an ADR (`/adr`, "let's write an
ADR", "record this decision", "supersede NNNN").

**Do not invoke** for: bug fixes restoring documented behaviour, internal
refactors that do not cross a public boundary, typo fixes or rewording in an
existing guideline that does not change the underlying rule, doc-only
formatting, test additions, dependency bumps without semantic change.

## Procedure

### Step 1 — Confirm an ADR is warranted

When triggered automatically, first state out loud (one or two sentences) why
you think the current discussion qualifies and which category it falls into.
Then ask the user whether to proceed with drafting an ADR. Examples:

- "This looks like a **performance** decision (switching from `Vec` to
  `SmallVec` in the planner's hot loop). Per `docs/adrs.md`, that's
  ADR-worthy. Want me to draft one?"
- "We're taking an **integration interface** decision (adding a new YAML key
  `sensors[].timeout_ms`). That meets the ADR bar — should I draft it?"

Do not draft silently. The user must opt in. If they decline, drop the topic;
do not nag.

If the user invokes the skill explicitly (`/adr` or "write an ADR"), skip the
warrant check and go straight to Step 2.

### Step 2 — Gather inputs

Ask the user for these, all at once:

1. **Title** — short, declarative (will become the heading and the kebab
   filename). Example: "Forbid async in goap-planner".
2. **Driving issue** — GitHub issue number(s) if one exists. Optional but
   strongly preferred; use `Refs #N`.
3. **Decision drivers** — the forces that mattered (1–4 bullets).
4. **Considered options** — at least two; one may be "do nothing".
5. **Chosen option** plus a one-paragraph justification.
6. **Consequences** — both positive and negative bullets.
7. **Confirmation** — how compliance is verified (test, bench guard, lint,
   review checklist). Optional but encouraged.
8. **Decision-makers** — the people / roles. Optional.
9. **Supersedes** — ADR number(s) being replaced, if any.

If the conversation already contains enough material to fill these in, draft
the ADR and ask the user to confirm or correct, rather than re-asking.

### Step 3 — Determine the next free ADR number

Run from the workspace root:

```sh
ls docs/adrs/ | grep -E '^[0-9]{4}-' | sort | tail -1
```

Increment the leading number by one and zero-pad to four digits. If
`docs/adrs/` has only `0000-template.md` and `README.md`, the next number is
`0001`.

### Step 4 — Create the ADR file

Copy `docs/adrs/0000-template.md` to `docs/adrs/NNNN-<kebab-title>.md` and
fill it in. Required fields:

- Front-matter: `status: proposed`, `date: <today, YYYY-MM-DD>`,
  `decision-makers`, optional `consulted` / `informed`, optional
  `supersedes: NNNN`.
- Body: `Context and Problem Statement` (linking the issue with `Refs #N`),
  `Decision Drivers`, `Considered Options`, `Decision Outcome`, `Consequences`.
- Optional: `Confirmation`, `Pros and Cons of the Options`, `More Information`.

Keep it short. Aim for 30–80 lines of markdown.

### Step 5 — Update the index

Add a row to `docs/adrs/README.md` under the **Records** table. Replace the
placeholder row if this is the first ADR. The row format is:

```
| 0001 | [<Title>](./0001-<kebab-title>.md) | proposed | YYYY-MM-DD |
```

Keep the table sorted by ADR number ascending.

### Step 6 — If superseding an existing ADR

If the new ADR replaces an older one, edit the older ADR in the same change:

- Set `status: superseded` in its front-matter.
- Add `superseded-by: NNNN` (the new ADR's number).
- Update its row in `docs/adrs/README.md` to reflect the new status.

### Step 7 — Hand back to the user

Report:

1. The new ADR's path and number.
2. The index entry that was added or updated.
3. A reminder that the commit should follow the workspace Conventional
   Commits convention with the `docs(adrs):` scope, e.g.
   `docs(adrs): add ADR NNNN for <short title>`.
4. A reminder that `status: proposed` must be flipped to `accepted` only after
   PR ratification.

Do not commit, push, or open a PR yourself unless the user explicitly asks.

## Reference

- Guideline: [`docs/adrs.md`](../../../docs/adrs.md)
- Index: [`docs/adrs/README.md`](../../../docs/adrs/README.md)
- Template: [`docs/adrs/0000-template.md`](../../../docs/adrs/0000-template.md)
- Format: [MADR 4.0.0](https://adr.github.io/madr/)
- Commit / PR conventions:
  [`docs/coding-guidelines.md`](../../../docs/coding-guidelines.md)
  under "Commits and pull requests".
