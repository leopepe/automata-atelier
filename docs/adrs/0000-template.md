---
status: proposed
date: YYYY-MM-DD
decision-makers: []
consulted: []
informed: []
# supersedes: NNNN          # set if this ADR replaces a previous one
# superseded-by: NNNN       # set when a later ADR replaces this one
---

# {short, declarative title — matches the filename}

## Context and Problem Statement

{Two or three sentences. What is the situation, and what decision is forced?
Link the driving issue: `Refs #N`. If a perf bench, security review, or user
report triggered this, mention it.}

## Decision Drivers

* {force / constraint 1 — why it matters}
* {force / constraint 2}
* {force / constraint 3}

## Considered Options

* {option 1}
* {option 2}
* {option 3 — may be "do nothing"}

## Decision Outcome

Chosen option: **"{option N}"**, because {one-paragraph justification —
which drivers it satisfies and which trade-offs were accepted}.

### Consequences

* Good, because {positive consequence}.
* Good, because {positive consequence}.
* Bad, because {negative consequence — what we now have to live with}.
* Bad, because {negative consequence}.

### Confirmation

{How is compliance with this decision verified? A test, a benchmark guard,
a clippy lint, a CI workflow, a code-review checklist item? If nothing
automated exists, say so explicitly. Optional but encouraged.}

## Pros and Cons of the Options

### {option 1}

{One-line description or pointer.}

* Good, because {…}.
* Neutral, because {…}.
* Bad, because {…}.

### {option 2}

{One-line description or pointer.}

* Good, because {…}.
* Bad, because {…}.

### {option 3}

{One-line description or pointer.}

* Good, because {…}.
* Bad, because {…}.

## More Information

{Links to related ADRs, issues, PRs, prior art, follow-up tasks. If this ADR
should be revisited under a specific condition (a benchmark threshold, a
release milestone, a new dependency), record that trigger here.}
