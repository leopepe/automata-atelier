# Automata Atelier Workspace

Cargo workspace housing three crates that together implement a Goal-Oriented
Action Planning system:

- **grafo** — fast directed-acyclic-graph library with Dijkstra search; the kernel underneath everything else.
- **goap-planner** — pure GOAP library on top of grafo. Defines `State`, `Action`, `Goal`, `Plan`, `Planner`. No I/O, no opinion on how state is observed.
- **uncharles** — sense → plan → act runtime CLI. Loads a YAML config describing sensors and actions, drives the planner, optionally executes the plan and replans on divergence.

## Looking for further instructions

Before doing work in any subdirectory, **read every `CLAUDE.md` on the path from this workspace root down to the file you're touching**. Sub-directory CLAUDE.md files carry domain-specific guidance and may extend or override what's written here.

Specifically, check these locations whenever they exist:

- **Member crates**: `grafo/CLAUDE.md`, `goap-planner/CLAUDE.md`, `uncharles/CLAUDE.md` — per-crate conventions, public API rules, dependency policies.
- **Domain subdirs inside a crate**: `<crate>/docs/CLAUDE.md`, `<crate>/examples/CLAUDE.md`, `<crate>/tests/CLAUDE.md`, `<crate>/benches/CLAUDE.md` — instructions scoped to that activity (how to write tests, when to run benches, etc.).
- **Workspace-level subdirs**: `docs/CLAUDE.md` — guidance that applies across crates.

If a sub-directory has no `CLAUDE.md`, fall back to the closest ancestor that does. A `CLAUDE.md` applies to its own directory and everything beneath it.

These files are populated lazily: directories without one have no domain-specific rules yet. The user adds a `CLAUDE.md` when a particular directory needs its own instructions; absence does not mean "no instructions exist" globally — it just means none scoped to that path.

## Layered architecture (don't blur the boundaries)

The crates have deliberately clean separation; preserve it when adding code:

- **grafo** has no idea GOAP exists. Keep it general-purpose.
- **goap-planner** has no idea about the real world. No shell-outs, no async, no I/O. Pure CPU work over `State` / `Action` / `Goal`.
- **uncharles** is where shell exec, YAML parsing, signal handling, and side effects live. Cloud-specific adapters belong here (or in optional sibling crates), never in `goap-planner`.

If a change feels like it should live in a lower layer, that's almost always the wrong direction — push it up, not down.

## Pending work

`docs/todo.md` tracks deferred design notes captured during sessions. Read it before proposing changes that may overlap with already-tracked work, so what you do composes with what's already planned.
