# Photo archiver — user-corrected code-quality ledger

These entries record structural/style weaknesses in the Conductor-generated
example that were corrected **by the user**, after the implementation cycle.
The baseline is commit `7d730e5c2c4eac5205be5b498856e1b670dfdaea`.
They are not fixes authored by Conductor or findings from another automated
review pass. The source diffs in this branch provide the before/after evidence.

This is the shareable extract of the review ledger. Private execution logs,
model receipts, local paths, and telemetry remain outside the repository.

## U1 — sensor dispatch and responsibility separation

**Conductor's weakness:** `scripts/sensors.py` combined repeated sensor-name
`if` branches, nested idle decisions, inventory comparisons, and receipt/hash
verification in the main dispatcher. A branch-local import and indirect
optional-state assumptions also made the control flow harder to follow.

**User correction:**

- Replace the two dispatch groups with `match name` / `case` blocks.
- Extract typed module-private `_sense_idle`, `_sense_photos_copied`, and
  `_sense_archive_verified` helpers for coherent observation responsibilities.
- Move `photo_files` to module imports and guard absent session/batch explicitly
  before calling helpers that require them.

The remaining Boolean guard clauses are intentional; not every conditional
needs pattern matching. Leading underscores communicate internal helpers by
Python convention, not enforced protected access.

## U2 — action decomposition and resource ownership

**Conductor's weakness:** `scripts/actions.py` duplicated low-level output
opening/regular-file validation/stream cleanup and embedded too many copy and
notification responsibilities in large orchestration functions. Notification
text and mode/timeout literals were repeated, and CLI dispatch used `if`/`elif`.

**User correction:**

- Centralize output lifecycle in `_open_output`, using `contextmanager` and
  `ExitStack` for explicit descriptor/stream ownership and append/truncate modes.
- Extract `_verify_copied_photo` and `_copy_to_temporary`; keep journaling and
  no-clobber publication visible in `copy_photo`.
- Extract `_notification_session`, `_restore_notification_receipt`, and
  `_record_notification`, separating guards, recovery, and durable bookkeeping.
- Reuse `_notification_body`; name file-mode and notification-timeout constants
  and declare notification constants `Final`.
- Use `match args.action` with supported cases and an explicit error case.

This includes resource-management restructuring, not only whitespace changes.
No reproduced resource leak in the old implementation is claimed.

## Review gap

Conductor's implementation worker consulted the Python guidelines, and its
completed lint/review checks did not identify these structural weaknesses.
Reading guidelines or passing a linter is not proof that code organization,
responsibility separation, or readability meets the user's expectations.

## Validation after both user refactors

Fresh pre-PR checks passed:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `ruff check --output-format concise scripts/` (from the example directory)
- Manual isolated `uv run` copy/resume, source-byte preservation, verification,
  immutable manifest, repeated notification without duplicate submission,
  archive-preserving reset, stale-reset rejection, and stale verification proof
  rejection after equal-count destination corruption.

No Python-script test suite was added. Manual checks used only synthetic media
and a mock notification sink. Real SD-reader classification and desktop
notification/banner acceptance remain **unverified**.

**Formatting warning:** `ruff format --check` reports manual wrapping/spacing
in the two user-refactored files. Their layout is preserved; this is not a
formatter pass. Functional checks above passed independently.

## Separate open orchestration question

The cycle used one implementation worker for five serialized tasks because
Conductor's current sequential fallback prescribes that granularity. Dependencies
and shared files prevent concurrent writers, but do not inherently prevent
serial per-task fresh contexts. This remains an open policy question; no
Conductor-policy change is included in this example PR.
