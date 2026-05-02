# Architecture Decision Records (ADRs)

How architectural decisions are captured in this workspace. ADRs are short,
durable records of *significant* technical decisions: what was chosen, what was
considered, and why. They sit next to the code they describe and travel with
the repository.

ADRs are complementary to GitHub issues:

- **Issues** are where decisions get *discussed* and tracked through review
  (`type:design`, `type:feature`, `type:bug`). See [`issues.md`](./issues.md).
- **ADRs** are where decisions get *recorded* once made, in a stable, citable
  form. An ADR's `Refs` link points back at the issue(s) that drove it.

If a discussion never reaches a meaningful decision, it stays an issue. If a
decision is taken that meets the bar in [When to write an ADR](#when-to-write-an-adr),
it gets an ADR.

## Where ADRs live

```
docs/
  adrs.md                       # this guideline
  adrs/
    README.md                   # index — every ADR listed with status + date
    0000-template.md            # the MADR template; copy this to start a new ADR
    0001-<kebab-title>.md       # individual records, sequentially numbered
    0002-<kebab-title>.md
    ...
```

- **Filename**: `NNNN-kebab-case-title.md`. Four-digit zero-padded sequential
  number, hyphen, then the title in lowercase kebab case.
- **Numbering**: monotonic across the workspace. Pick the next free number;
  never re-use a number, even after a record is superseded.
- **Title** in the filename mirrors the `# {title}` heading at the top of the
  file. Keep it short and imperative-ish (`use-grafo-as-graph-kernel`,
  `forbid-async-in-goap-planner`).

## Format: MADR 4.0.0

We use [MADR 4.0.0](https://adr.github.io/madr/) (Markdown Any Decision Records)
short form. It is the most actively maintained ADR template, structured enough
to capture trade-offs (`Decision Drivers`, `Considered Options`, `Consequences`)
without ceremony.

The canonical template lives at [`adrs/0000-template.md`](./adrs/0000-template.md);
copy it when starting a new ADR. Required sections:

- **YAML front-matter** — `status`, `date`, `decision-makers`, optional
  `consulted` and `informed`, optional `supersedes` / `superseded-by`.
- **Title** — `# <short, declarative title>` matching the filename.
- **Context and Problem Statement** — two or three sentences. Link the
  driving issue with `Refs #N`.
- **Decision Drivers** — the forces / constraints that mattered.
- **Considered Options** — at least two; one of them may be "do nothing".
- **Decision Outcome** — chosen option plus a one-paragraph justification.
- **Consequences** — bulleted, both `Good, because …` and `Bad, because …`.

Optional but encouraged:

- **Confirmation** — how compliance is verified (a test, a benchmark guard,
  a lint, a code review checklist).
- **Pros and Cons of the Options** — flesh out the trade-off when more than
  two options were close.
- **More Information** — links to issues, PRs, prior art, follow-ups.

Keep ADRs short. A typical record is 30–80 lines of markdown; 200+ is a smell
that the decision is actually several decisions and should be split.

### Status lifecycle

The `status` field in the front-matter takes one of:

- `proposed` — drafted, not yet ratified. Discussion still open.
- `accepted` — ratified. The current behaviour of the workspace.
- `deprecated` — discouraged but not yet removed. The record stays for history.
- `superseded` — replaced by a newer ADR. The front-matter must include
  `superseded-by: NNNN`, and the superseding ADR must link back via
  `supersedes: NNNN`.

Once an ADR is `accepted`, do not edit its decision retroactively. If the
decision changes, write a new ADR that supersedes it. Typo fixes and link
updates are fine; semantic edits are not.

## Indexing

[`adrs/README.md`](./adrs/README.md) is the canonical index. Every ADR is
listed there in a table:

| #    | Title                              | Status     | Date       |
| ---- | ---------------------------------- | ---------- | ---------- |
| 0001 | [Use grafo as graph kernel](./adrs/0001-use-grafo-as-graph-kernel.md) | accepted   | 2026-05-02 |
| 0002 | [Forbid async in goap-planner](./adrs/0002-forbid-async-in-goap-planner.md) | accepted   | 2026-05-02 |

When you add or change an ADR's status, update the index in the same commit.
The index is the source of truth for "what decisions exist?"; the filesystem
listing is a fallback.

## When to write an ADR

Write an ADR whenever a decision lands in any of these categories. The bar is
"would a future contributor be confused, surprised, or tempted to undo this if
they didn't know why we did it?". If yes, ADR.

**You must write an ADR for:**

- **Architecture**: layering, ownership boundaries, the contract between the
  crates (`grafo` ↔ `goap-planner` ↔ `uncharles`), introduction or removal of
  a crate, choice of foundational pattern (event loop vs. actor vs. pipeline).
- **Core components**: which library underpins a kernel responsibility (graph
  search, planner, executor, parser). Swapping or significantly reshaping such
  a component is an ADR.
- **Performance**: any decision that constrains future work for performance
  reasons — choice of algorithm, data structure, allocation strategy,
  concurrency model (sync vs. `async` vs. `rayon`), benchmark gate thresholds.
- **Security**: trust boundaries, authentication/authorisation choices,
  secret-handling, sandboxing of executed actions, supply-chain controls
  (signed deps, vendoring policy).
- **User interface**: CLI verb/resource grammar changes that aren't already
  covered by [CLI conventions](./coding-guidelines.md#cli-conventions-posix--kubectl),
  output format additions, `--help` semantics, exit-code policy.
- **Integration interface**: YAML/config schema, HTTP/gRPC contract, plugin or
  extension API — anything an external consumer codes against.
- **Operability**: anything that changes how operators run, observe, debug,
  or recover the system in production (logging policy, metrics surface, signal
  handling, replan triggers, dry-run semantics).
- **Guidelines and process**: adding a new workspace guideline, removing one,
  or materially changing one. Includes `docs/coding-guidelines.md`,
  `docs/performance-tests.md`, `docs/issues.md`, `docs/adrs.md` itself, any
  `CLAUDE.md`, the PR / issue templates, the label scheme, branch-protection
  rules, the commit-convention policy, the lint or format toolchain, and the
  CI gate semantics. The ADR captures *why* the workspace works this way so a
  future contributor sees the rationale and not just the rule.

**You may skip an ADR for:**

- Purely internal refactors that do not change a public boundary.
- Bug fixes that restore documented behaviour.
- Adding a test, a benchmark, a docstring, or formatting changes.
- Dependency bumps that don't change semantics.
- Trivial new features that fit cleanly into an existing ADR's scope (note them
  in that ADR's `More Information` section instead of writing a new one).
- Tightening, clarifying, or fixing typos in an existing guideline without
  changing the underlying rule. New rules and rule reversals always need an
  ADR; rewording the same rule does not.

When in doubt, lean toward writing one. ADRs are cheap; rediscovering "why is
it like this?" six months later is not.

## Workflow

1. **Open or reference an issue** under the appropriate template (usually
   `type:design`). Discuss until a decision is reachable.
2. **Copy `docs/adrs/0000-template.md`** to `docs/adrs/NNNN-<title>.md` with
   the next free number. Set `status: proposed`.
3. **Fill in the record**, link the issue with `Refs #N`, and open a PR. The
   PR's purpose is to ratify the ADR; reviewers comment on the markdown.
4. **Update [`adrs/README.md`](./adrs/README.md)** in the same commit so the
   index reflects the new entry.
5. **Flip to `status: accepted`** once the PR is approved. Squash-merge.
6. **If a later ADR overrides this one**, set the front-matter to
   `status: superseded`, add `superseded-by: NNNN`, and update the index. Do
   not delete the file or rewrite its decision.

## Commits and PRs

ADR commits follow the workspace's [Conventional Commits](./coding-guidelines.md#commits-and-pull-requests)
rules. Use the `docs` type with the `adrs` scope:

- `docs(adrs): add ADR 0007 for executor sandboxing`
- `docs(adrs): supersede 0003 with 0011 (new bench gate policy)`
- `docs(adrs): mark 0004 deprecated`

PR titles follow the same convention. The PR body uses the workspace template
and links the driving issue with `Closes #N` (if the ADR fully resolves the
discussion) or `Refs #N`.
