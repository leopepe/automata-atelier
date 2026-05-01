# Claude Code — Documentation Authoring Guidelines

These guidelines define how Claude Code should create new documentation files in this directory.

---

## File Format

- All documentation files must be written in **Markdown** (`.md` extension).
- Use UTF-8 encoding with Unix-style line endings (`\n`).
- Do not include a trailing newline beyond the final content block.

---

## Audience

- The target audience is **Claude Code itself** (an AI agent), not human readers.
- Write content that is **precise, unambiguous, and machine-parseable**.
- Avoid idioms, metaphors, or narrative prose. Prefer structured, declarative statements.
- Assume the reader has no prior context beyond what is explicitly stated in the file.

---

## File Naming

- Use lowercase letters and hyphens only: `kebab-case.md`.
- Names must be descriptive and reflect the file's exact purpose (e.g., `api-auth-flow.md`, `error-handling-rules.md`).
- Do not use spaces, underscores, or camelCase.

---

## Document Structure

Every document must follow this structure, in order:

1. **H1 Title** — A single, concise title describing the document's scope.
2. **Purpose block** — A short paragraph (1–3 sentences) stating what this document covers and when it should be referenced.
3. **Body sections** — Use H2 (`##`) for top-level sections and H3 (`###`) for subsections. Do not skip heading levels.
4. **Rules or steps** — Express actionable content as numbered lists (for ordered steps) or bullet lists (for unordered rules).
5. **Examples** — Provide fenced code blocks with an explicit language tag for all code or structured data examples.

---

## Writing Rules

- **Be explicit.** State every requirement fully. Do not rely on implied behavior.
- **Use imperative mood** for instructions: "Return X", "Do not use Y", "Always include Z".
- **One idea per bullet.** Do not combine multiple rules into a single list item.
- **Define terms inline** on first use if they may be ambiguous.
- **Avoid filler words**: "simply", "just", "easy", "straightforward", etc.
- **No motivational language**: Do not explain *why* a rule exists unless the reason directly affects how it should be applied.

---

## Code Examples

- Always use fenced code blocks with a language identifier:
  ````
  ```json
  { "key": "value" }
  ```
  ````
- Code examples must be minimal, correct, and directly illustrate the point being made.
- Do not include placeholder comments like `// TODO` or `// ...` unless explicitly demonstrating incomplete patterns.

---

## Metadata

- Do not include YAML front matter unless the build system in this directory explicitly requires it.
- If front matter is required, include only the fields specified by the project schema — do not add extra fields.

---

## Cross-References

- Reference other documents by relative file path: `[auth rules](./api-auth-flow.md)`.
- Do not use absolute URLs for internal documents.
- Do not reference documents that do not exist in this directory at the time of writing.

---

## What to Avoid

- Do not write conversational or explanatory prose aimed at a human reader.
- Do not include redundant section headers such as "Introduction", "Overview", or "Conclusion".
- Do not pad documents with background context that is not directly actionable.
- Do not duplicate content that already exists in another file in this directory — cross-reference it instead.
