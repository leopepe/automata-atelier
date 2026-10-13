## Context

See `proposal.md` for motivation and `specs/camera-photo-archiving/spec.md` for observable requirements. This imports `docs/superpowers/plans/2026-10-13-photo-archiver-uncharles.md`, including the operator's subsequent corrections: Python 3.14+, local pyproject, uv execution, no Python-script tests, and report-only uncharles failures/missing features.

Repository evidence: uncharles execute mode is already perpetual; `src/actors/goal_supervisor.rs` treats no-plan/goal-satisfied as nonterminal in watch mode. Sensor stdout capture reaches only actions whose `requires` mentions that captured fact (`src/run.rs`). Continuous sensors are parallel and action effects are optimistic. This is source inspection, not proof the complete photo workflow has run.

## Goals / Non-Goals

**Goals:** A macOS example demonstrating strict observe/mutate boundaries, source-preserving imports, bounded-memory integrity checks, per-session idempotence, and completion notification. Preserve the original plan's typed helper interfaces and filesystem/session contracts.

**Non-Goals:** Changes to uncharles, grafo, or goap-planner; new runtime/schema features; Python-script tests; a root Python workspace; background wrapper/LaunchAgent installation; file conversion, ejection, global deduplication, or post-import corruption monitoring. Runtime failures or missing features are findings, not fix assignments.

## Decisions

### Example-local Python project; uncharles launches through uv

Create `uncharles/examples/photo-archiver-uncharles/pyproject.toml` with `requires-python = ">=3.14"`, empty dependencies, and `[tool.uv] package = false`. Generate a local `uv.lock`; ignore the generated `.venv/` and bytecode. No root uv workspace or separate `.python-version` is needed.

Operator setup: `uv sync --project uncharles/examples/photo-archiver-uncharles --python 3.14`. Every YAML helper command is an argv list with this prefix:

`uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 --no-sync --frozen --offline --no-python-downloads python -B`

Append the workspace-relative script path and subcommand. This replaces the direct `.venv/bin/python` approach rejected by the operator. Prepared-environment operation must be checked for project/environment writes; uv launcher purity is not assumed from flag names. Missing setup is an explicit failure, not permission to provision from a sensor. YAML paths resolve from the workspace root.

### Identity-bound read-only discovery and inventory

Use read-only diskutil plist commands and `plistlib`; inspect parent physical disk and mounted partition. Automatic SD selection requires identifiable Secure Digital removable/ejectable media; explicit `PHOTO_ARCHIVER_VOLUME_UUID` selects a known qualifying USB reader/card. Refuse ambiguous or unknown selection rather than guessing from `/Volumes` or DCIM alone. Real reader plist classification remains a hardware acceptance item.

Inventory regular non-symlink files under DCIM with case-insensitive `.jpg`, `.jpeg`, `.cr2`, `.cr3`, `.crw` extensions. Detection is a directory/extension convention, not manufacturer/content-decoding proof. A compact captured batch contains volume UUID, normalized mount, deterministic metadata inventory ID, count, and bytes. Hashes of sorted relative paths/sizes/mtimes identify inventory changes but do not replace content SHA-256 verification.

### Session-safe copying and immutable completion evidence

Use schema version 1 `active-session.json` under `~/Library/Application Support/uncharles-photo-archiver/` and `.uncharles-photo-archive.json` inside `~/Pictures/YYYY-MM-DD_HH-MM-SS-<session-id>/`. Choose local extraction date/destination once. Preserve source-relative DCIM paths and source bytes; never delete/move/eject source data or overwrite unrelated Pictures files.

Only actions create state/locks/destinations/manifests. Serialize action writers with `fcntl.flock`, re-read current state under the lock, and atomically replace JSON. Stream photo data/hash in 1 MiB chunks through owned per-file temporary names; persist progress for matching interrupted sessions. Skip an existing completed file only after its recorded hash matches; reject different existing bytes. Validate source inventory before completion. Persist action failures for the matching session and avoid repeated automatic action retries.

The completed copy manifest is immutable. Notification proof/receipt is a separate `.uncharles-photo-notification.json`, referenced by active state, so recording notification does not invalidate the manifest digest in its proof. Reset only a matching active record and preserve all complete/partial archives.

### Verification is a sensor; notification rechecks its guard

`archive_verified` compares current source, copy manifest, and destination exact photo path set/count/size/SHA-256 and emits stable JSON proof bound to session/inventory/destination/manifest digest/count. It writes nothing. An integrity mismatch remains unverified/unnotified; the sensor cannot persist a failure marker. After successful notification, receipt proof may represent import-time verification without rehashing completed imports; this does not promise ongoing corruption detection.

Every mutating action revalidates its captured identities. Notification additionally rechecks current integrity before using a fixed osascript program with message text passed as argv. Title: `Photo import complete`; body: `Copied and verified <count> photos to <destination>`. Record a receipt only after submission succeeds. Submission is not evidence the user saw a banner; Focus/permissions can suppress it. A crash between submission and receipt persistence can cause a repeat.

### Staged daemon goal on the existing planner

Sensors: `sd_card_present`, `photo_batch` (adds/removes `photos_present` together with its own fact), `photos_copied`, `archive_verified`, `user_notified`, `import_failed`, `session_obsolete`, `idle`. Sensors independently observe disk/session state and do not consume other sensors' env values or rely on declaration order. Sensor exit 0 means true, 1 false, 2 observation/config error; uncharles maps both nonzero outcomes to failure effects. Only proven absence/change makes an existing session obsolete.

- `copy_photos`, cost 2: requires `[sd_card_present, photo_batch, photos_present]`, forbids `[photos_copied, import_failed, session_obsolete]`, adds `[photos_copied, idle]`.
- `notify_user`, cost 1: requires `[sd_card_present, photo_batch, photos_copied, archive_verified]`, forbids `[user_notified, import_failed, session_obsolete]`, adds `[user_notified, idle]`.
- `reset_session`, cost 1: requires `[session_obsolete]`, adds `[idle]`, removes `[photo_batch, photos_present, photos_copied, archive_verified, user_notified, import_failed, session_obsolete]`; use the captured obsolete ID as its guard.
- Copy/notify failure effects: add `[import_failed]`, remove `[idle]`, without applying success effects. Reset failure preserves evidence and is terminal.

Goal requires `[idle]`. Idle is false while a nonfailed unnotified batch or obsolete bookkeeping remains, true for cold/no-photo idle or an already notified/explicitly failed matching session. Copy's optimistic idle is a reachability bridge: the idle sensor removes it until notification is actually recorded; while verification is pending the daemon can have no plan. Never add `archive_verified` from an action and never make waiting/verification an action. Use daemon mode, not `--once`, which could exit at the copy-stage optimistic idle. Evaluate the actual workflow in a manual mock run; report a runtime blocker rather than modifying the planner.

### Validation, scope, and Conductor control

Python helpers get manual isolated checks, not unit/integration test files or a Cargo wrapper. The retained Rust tests exercise only compiled `run --dry-run`/`inspect`, YAML surface, and verification-fact wiring without executing helpers. Run workspace format/clippy/test gates and an isolated workflow demo, preserving exact commands/logs and honest blocked results.

`PHOTO_ARCHIVER_TEST_ROOT` confines synthetic source to `<root>/card`, Pictures/state to that root, and notifications to `<root>/notifications.jsonl`; ignore production output overrides in mock mode. Real hardware/photo access requires separate approval. Once authorized, hardware acceptance uses a dedicated test card and isolated output/state paths.

The Conductor is the sole cycle controller. Import is external to its Explore/Propose phases; the operator explicitly skips those dispatches, with `ran: false` and reason in metrics. Run remaining applicable gates against the canonical artifacts. Preserve metrics, ledger, routing, and command evidence under `.conductor/photo-archiver-uncharles/`, never stage private state. Local exact-scope commits are allowed to checkpoint stable review evidence; push/PR/merge/public publication is not authorized. Side-quest budget is zero for out-of-scope work; assigned validation is not a side quest. Existing uncharles defects/missing features are reported at cycle end, not repaired.

## Risks / Trade-offs

- SD-reader plist fields vary → fail closed on unknown classification, support explicit known-card UUID, and do not claim hardware acceptance without the dedicated card check.
- A polling interval can miss unplug/replug entirely between polls or while stopped → import once per observed insertion; same persisted unchanged session resumes/skips after restart; global deduplication is out of scope.
- Continuous parallel readings may be delayed or refer to an old session → capture compact identity/proof, guard every action with current observations, and use token-bound reset.
- Full hashing adds SD/disk I/O → use bounded chunks, avoid rehashing already notified sessions, and document import-time rather than continuous verification.
- Runtime setup or staged-goal behavior may fail → retain evidence and stop affected checks; report missing features without changing runtime behavior or claiming verification success.
- No automated Python-script coverage is requested → keep manual source-preservation/corruption/no-repeat checks visible and distinguish them from retained config/CLI tests.

## Migration Plan

No existing app/config migration. Prepare uv, preview/inspect the example, then run the daemon from the workspace root at a 2000 ms cadence with an explicit finite action cap. Stop gracefully before manual reset/removal. Rollback removes/disables the new example; never remove user archives or source photos as rollback. Archive OpenSpec planning state only if verification permits; leave blocked/unverified changes unarchived with telemetry intact.
