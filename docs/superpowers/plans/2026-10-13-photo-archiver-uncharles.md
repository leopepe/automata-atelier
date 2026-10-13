# Photo Archiver Uncharles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans for direct implementation, or superpowers:subagent-driven-development only if the user authorizes delegation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a macOS example that senses a mounted camera SD card, copies its JPEG/Canon RAW photographs into a dated directory under Pictures, verifies the complete archive, and then submits a completion notification.

**Architecture:** Use the existing perpetual uncharles runtime, a declarative YAML config, and Python-only helper scripts. Sensors observe disks, photo inventories, copy progress, integrity, and notification receipts without writing anything; actions copy photographs, submit notifications, and reset obsolete session bookkeeping. No changes to Rust runtime behavior or the core libraries are planned.

**Tech Stack:** Python **3.14+** standard library (`plistlib`, `pathlib`, `hashlib`, `json`, `subprocess`, `fcntl`), **uv** for interpreter/project setup, macOS `diskutil` and `osascript`, existing uncharles YAML/fact capture, Rust compiled-binary **config/CLI integration tests only**.

**Spec:** The user's five-step brief, refined by the [proposed design below](#proposed-design-pending-approval). Unanswered choices are explicitly proposals, not approved requirements.

## Global Constraints

- **Conductor implementation authorized.** The operator requests conversion into canonical OpenSpec proposal artifacts, skipping Explore/Propose dispatches, then implementation controlled by the Conductor agent at `~/Workspace/conductor/`. Retain remaining required gates and persist metrics/ledger throughout; do not launch an unsupported Prepared Change admission route.
- The example belongs at `uncharles/examples/photo-archiver-uncharles/`, with helper scripts in its `scripts/` subdirectory. `uncharles/examples/` does not currently exist and will be created during implementation.
- Use **Python only** for helper scripts; no shell wrapper and no inline shell/Python programs in YAML. A fixed, minimal AppleScript expression passed to macOS `osascript` is solely the notification API adapter, not a second helper-script implementation.
- Require **Python 3.14+** with a local `pyproject.toml` inside `uncharles/examples/photo-archiver-uncharles/`; do not create a root uv workspace. Prepare the environment explicitly before the daemon. Uncharles invokes every helper through `uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 --no-sync --frozen --offline --no-python-downloads python -B <script> <command>`. Environment setup belongs to operator setup, not sensor polling.
- Sensors may read and print observations but must not create directories, manifests, receipts, locks, caches, or files. Run Python with `-B` to prevent implicit `__pycache__` writes, including during previews.
- **Do not create tests for the Python scripts:** no Python test files, test framework, script-specific automated suite, or Cargo wrapper for such a suite. Use manual isolated smoke checks for helper behavior; retain Rust tests only for the compiled uncharles config/CLI surface.
- **Do not fix uncharles or add missing runtime features.** If the existing binary fails or lacks a required feature, preserve the failure evidence, mark the affected example checks blocked, and report the findings at the end of the cycle. Do not weaken verification or sensor/action separation to hide a limitation.
- All mutations are actions. The SD card is read-only from this example's perspective: never delete, rename, move, format, or eject source files/media.
- Preserve photo bytes and source-relative paths. Existing unrelated Pictures contents must never be overwritten.
- Use the current schema: `capture: stdout`, action `requires`/`forbids`, action effects, and `on_failure`. No new YAML fields or runtime features.
- No new Rust or Python dependencies; no changes to `grafo`, `goap-planner`, or their performance-sensitive paths.
- Follow root `CLAUDE.md` and `docs/coding-guidelines.md`; check for new scoped instructions again before implementation.

## Review Focus

These remain implementation/manual-acceptance review criteria, not a request to create Python-script tests.

1. Source ejection/replacement and delayed sensor readings: never apply one card's inventory or verification to another card/session (Tasks 1–3).
2. Spaces, quotes, Unicode, newlines, duplicate basenames, and symlinks: preserve valid paths, never execute filename text, and never escape source/destination roots (Tasks 1–3).
3. Equal counts with different bytes, missing files, or a changed inventory: these must fail integrity checks and must not produce a success notification (Tasks 1 and 3).
4. Interrupted copies, full disks, and restarts: preserve source data and partial evidence, resume only a matching session, and never overwrite mismatching existing archive files (Task 2).
5. Parallel sensor timing and repeated observations: copy and notification must be independently guarded and idempotent; verification must be sensor-derived, not an optimistic claim of success (Tasks 3–4).

---

## Design for the authorized Conductor cycle

### Scope and proposed defaults

- Monitor in the foreground using uncharles' existing daemon mode, polling every **2 seconds**. A card already mounted at startup is eligible. No LaunchAgent installer, background shell loop, Photos.app integration, image conversion, or automatic ejection.
- Recognize regular files recursively under `DCIM/` with case-insensitive extensions **`.jpg`, `.jpeg`, `.cr2`, `.cr3`, `.crw`**. This covers Canon camera naming/formats without decoding images or requiring EXIF libraries. Videos, other files, and symlinks are excluded. Document that recognition is based on directory/extension, not proof of camera manufacturer.
- Archive the whole supported inventory into `~/Pictures/YYYY-MM-DD_HH-MM-SS-<session-id>/`, using the **local extraction date/time**, not EXIF capture dates. Preserve paths such as `DCIM/100CANON/IMG_0001.JPG` inside it. Choose the destination once per session; a restart must not change it.
- Verify the **exact relative-path set, file count, byte sizes, and SHA-256 hashes** against both the saved copy manifest and the still-mounted source. Counting alone is insufficient. Hash in bounded chunks, not by loading RAW files into memory.
- **Proposed reinsertion policy:** one import per observed insertion, no repeat while the same unchanged card remains mounted, and a fresh dated import after an observed ejection/reinsertion. A restart with the same persisted session resumes/skips that session. Global cross-import deduplication is out of scope. Polling cannot detect an unplug/replug that happens entirely between polls or while uncharles is stopped.
- A changed photo inventory is treated as a new batch: retire only obsolete active bookkeeping, then import the new complete inventory. Previously archived directories remain untouched.
- Support **one selected photo card at a time**. If selection is ambiguous, refuse to copy rather than guess; an explicit volume-UUID selector can disambiguate a known card/USB SD reader.
- Copy/notification errors fail closed: the action preserves partial archives and persists a failed session rather than repeatedly retrying. A sensor-detected integrity mismatch stays unverified/unnotified and reports diagnostics; the sensor cannot persist a failure marker. Do not automatically recopy a matching completed batch to fix mismatching bytes. Recovery is an explicit action command after the operator fixes the cause.
- Normal operation sends one completion notification per session. A crash between notification submission and receipt persistence can cause a repeat after restart; do not promise exactly-once desktop delivery.

### Disk discovery and identities

Use only read-only `diskutil list -plist physical` and `diskutil info -plist <device>` invocations, parsed with `plistlib`, including information about a partition's parent physical disk. Reject the startup volume, disk images, unmounted partitions, and unrelated disks.

Automatic selection requires identifiable SD media (a parent `BusProtocol` of `Secure Digital`, with removable/ejectable media). Do not guess from a `/Volumes/` directory or the presence of `DCIM` alone. USB readers may report only `USB`; permit those only through the explicit `PHOTO_ARCHIVER_VOLUME_UUID` selector, still requiring a mounted removable/ejectable physical device. Missing identity/classification information fails closed. Confirm the real plist fields on the target Mac during the hardware acceptance test; do not claim hardware discovery is validated by synthetic fixtures alone.

Identify a batch using **volume UUID + normalized mount path + deterministic inventory ID**. The inventory ID hashes sorted source-relative paths, sizes, and modification times; it is a change/identity token, **not** an integrity hash. Content SHA-256 hashes live in the copy manifest and verification proof.

Each sensor independently observes the selected device or persisted active session. Sensors do not receive other sensors' captured values and must not rely on YAML declaration order. Every action revalidates the captured batch/session against current disk and filesystem observations before mutation.

### Files and configuration

| Path (workspace-relative) | Responsibility |
| --- | --- |
| `uncharles/examples/photo-archiver-uncharles/config.yaml` | Sensors, actions, fact effects, and perpetual idle goal; argv arrays calling Python files only |
| `uncharles/examples/photo-archiver-uncharles/README.md` | Prerequisites, workflow, defaults, run/preview/mock commands, limits, safe recovery |
| `uncharles/examples/photo-archiver-uncharles/scripts/common.py` | Typed settings/observations; read-only discovery, inventory, source/destination integrity checks |
| `uncharles/examples/photo-archiver-uncharles/scripts/session.py` | Session schema, read-only loading, and action-only locking/atomic persistence helpers |
| `uncharles/examples/photo-archiver-uncharles/scripts/sensors.py` | Read-only sensor CLI |
| `uncharles/examples/photo-archiver-uncharles/scripts/actions.py` | `copy`, `notify`, and `reset-session` action CLI |
| `uncharles/examples/photo-archiver-uncharles/pyproject.toml` | Dependency-free uv project with `requires-python = ">=3.14"` and packaging disabled |
| `uncharles/examples/photo-archiver-uncharles/uv.lock` | Generated uv project lockfile; no third-party runtime/test dependencies |
| `uncharles/tests/photo_archiver.rs` | Compiled-binary config parsing/CLI inspection tests; does not run or test Python scripts |
| `uncharles/README.md` | Discoverable link to the example |
| `uncharles/examples/photo-archiver-uncharles/.gitignore` | Ignore `.venv/` and Python bytecode; never commit a generated environment |

Commands in YAML are relative to the **workspace root**, matching existing configs. Document that working directory explicitly; do not assume uncharles resolves script paths relative to the config.

**uv setup:** Create a dependency-free **example-local `pyproject.toml`**, with `requires-python = ">=3.14"`, an empty dependency list, and `[tool.uv] package = false`; do not add a root uv workspace or a separate `.python-version`. Generate the local lock/environment with `uv sync --project uncharles/examples/photo-archiver-uncharles --python 3.14` before starting uncharles. Runtime commands use `uv run` with the flags in Global Constraints to prevent synchronization, lockfile updates, network access, and interpreter downloads during polling. Confirm the existing prepared environment and interpreter version through the same uv invocation before startup. Do not claim uv launcher purity without checking project/environment artifacts during the manual smoke check; report any remaining launcher-side mutation as a limitation instead of adding a workaround feature.

Production defaults: Pictures = `Path.home() / "Pictures"`; state = `~/Library/Application Support/uncharles-photo-archiver/`. Allow explicit `PHOTO_ARCHIVER_PICTURES_DIR`, `PHOTO_ARCHIVER_STATE_DIR`, and `PHOTO_ARCHIVER_VOLUME_UUID` overrides. Validate that state/destination paths cannot overlap the selected source tree.

An explicit **`PHOTO_ARCHIVER_TEST_ROOT`** provides a safe hardware-free mode: `<root>/card` is the synthetic mount, `<root>/Pictures` the only output root, `<root>/state` the only state root, and `<root>/notifications.jsonl` the notification sink. This mode works on non-macOS hosts, ignores production output overrides, and never uses the real Pictures directory or sends desktop notifications. Sensors must not create any of these paths. The synthetic identity is stable within the same test root.

### State and verification contracts

Use schema version **1** for `active-session.json` in the state directory and `.uncharles-photo-archive.json` inside each archive. Record session ID, source identity, inventory ID, extraction timestamp, destination, per-file size/hash, active-session status (`copying`, `copied`, `failed`), and failure detail. The completed copy manifest is immutable; notification proof/receipt goes in a separate `.uncharles-photo-notification.json` file and is referenced by the active session. Never add a receipt/proof to the manifest whose digest the proof contains, which would invalidate that digest.

Only actions create the state directory, lock, destination, or manifests. Action writers acquire an exclusive `fcntl.flock` lock, re-read state, and persist JSON via temporary-file + atomic replacement. Sensor readers see either a complete old record or a complete new one. Malformed or unsupported records are errors, not evidence of completion or permission to delete anything.

Copy through per-file temporary names and rename only after a complete write. Persist enough progress to resume a matching interrupted session. Existing completed files may be skipped only after matching their recorded SHA-256; mismatching files must never be silently overwritten. A partial file owned by the same session may be retried. Complete the manifest/session only after every expected source photo was copied and the source inventory still matches.

The verification sensor performs full source/manifest/destination comparison after copying. Its captured proof is stable JSON containing session ID, inventory ID, destination, manifest digest, and verified count. It writes no verification marker. After notification, the sensor may report the receipt's saved proof without rehashing completed imports; this is an **import-time verification record**, not ongoing archive-corruption monitoring.

### Sensors and world facts

All entry points are launched by uncharles through uv: `uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 --no-sync --frozen --offline --no-python-downloads python -B uncharles/examples/photo-archiver-uncharles/scripts/sensors.py <sensor-name>`.

| Sensor/fact | Read-only observation and output |
| --- | --- |
| `sd_card_present` | Selected qualifying mounted card exists |
| `photo_batch` | Supported nonempty photo inventory; capture compact JSON `{volume_uuid, mount_path, inventory_id, count, total_bytes}`; add `photo_batch` and `photos_present` together, remove both on failure |
| `photos_copied` | Current matching session/manifest is complete and its expected archive files exist; not a content-integrity claim |
| `archive_verified` | Full exact-set/count/size/hash comparison succeeds for the current matching copied session; capture verification proof |
| `user_notified` | Successful notification-submission receipt matches the current session/batch |
| `import_failed` | Persisted failure matches the active batch |
| `session_obsolete` | Active session belongs to an absent/replaced card or changed inventory; capture the obsolete session ID |
| `idle` | No actionable photo batch, already notified, or explicitly failed/blocked; **false** while obsolete bookkeeping needs reset or an unnotified batch remains |

Sensor exit codes: `0` predicate true, `1` predicate false, `2` observation/configuration error. Uncharles maps all nonzero codes to failure effects, so no control flow may depend on it distinguishing 1 and 2. Observation errors must not be treated as proven ejection; `session_obsolete` is true only after a successful discovery establishes absence/change. Print diagnostics to stderr for direct troubleshooting; successful captured stdout contains machine data only. No timestamps/random values in repeated capture output.

### Actions and planner reachability

| Action | Preconditions | Success effects |
| --- | --- | --- |
| `copy_photos` (cost 2) | requires `sd_card_present`, `photo_batch`, `photos_present`; forbids `photos_copied`, `import_failed`, `session_obsolete` | adds `photos_copied`, `idle` |
| `notify_user` (cost 1) | requires `sd_card_present`, `photo_batch`, `photos_copied`, `archive_verified`; forbids `user_notified`, `import_failed`, `session_obsolete` | adds `user_notified`, `idle` |
| `reset_session` (cost 1) | requires `session_obsolete` | adds `idle`; removes old `photo_batch`, `photos_present`, `photos_copied`, `archive_verified`, `user_notified`, `import_failed`, `session_obsolete` facts |

For copy/notify failures, persist a failed session when the action owns the matching session and apply `on_failure: {add: [import_failed], remove: [idle]}`. The next observation settles the real blocked/obsolete state. Reset errors are terminal and preserve bookkeeping rather than hiding a corrupt state file.

**Important runtime constraint:** the planner cannot plan across a future sensor result. Requiring `archive_verified` for notification while never adding it optimistically would otherwise make copying unreachable from a single final-completion goal. Use **staged plans** with the existing `idle` convention: copying adds an optimistic `idle` solely to make the copy stage reachable; the read-only `idle` sensor then removes it until the batch is actually notified. While verification is pending the daemon may report no plan, which is nonterminal. Once `archive_verified` is observed, it plans notification.

Never add `archive_verified` in any action's effects, and never make verification an action. The notification action must nevertheless revalidate the captured proof and current source/destination integrity immediately before invoking the notification API; this is a safety precondition on a mutating action, not a separate verify/wait action. Stale asynchronous readings cannot authorize a false-success notification.

**Use daemon mode, not `--once`.** `--once` can exit after copying because of the staged optimistic `idle` effect. A preview may show only the currently reachable copy stage, not the complete eventual workflow. The README must explain this explicitly, and tests must not confuse `idle`/empty-plan events with successful archiving.

`reset-session` clears only a matching active bookkeeping record, never archive contents. The obsolete-session fact passes the expected ID via captured env so stale cleanup cannot clear a new session. Manual recovery uses `reset-session --session-id <ID>` after stopping the daemon; it retains partial archives and lets the next run create a fresh import.

Notification text: title **`Photo import complete`**, body **`Copied and verified <count> photos to <destination>`**. Invoke `osascript` with a fixed program and pass text as argv, never interpolate paths into AppleScript or shell source. Persist a receipt only after exit 0. Receipt means **submitted to Notification Center**, not proof the user saw a banner; Focus and notification permissions can suppress display.

### Acceptance criteria

A mounted supported camera card leads to a new dated archive containing every recognized photo with preserved bytes/paths, followed by notification submission only after integrity checks succeed. Empty/unrelated/ambiguous media is never copied. Continuous ticks do not duplicate a completed import. Ejection/replacement, corruption, copy failure, and notification failure cannot create a success receipt. Preview/sensing writes nothing. Hardware-free tests cannot touch real photos or send real notifications.

---

## Implementation tasks

Estimated implementation and verification: **about 1 hour 30 minutes** for the authorized cycle. Python helper tasks use manual isolated smoke checks, not new tests; Task 4 retains compiled-binary config/CLI tests. Commit suggestions are for the implementation phase only. Failed runtime checks are reported at cycle end, not repaired in uncharles.

### Task 1: uv setup, read-only discovery, inventory, and sensors — about 25 min

**Files:** create `pyproject.toml`, `uv.lock`, `scripts/common.py`, the read-only schema/loading portion of `scripts/session.py`, and `scripts/sensors.py` under the example path above.

**Interfaces:**
- Define `Settings(pictures_dir: Path, state_dir: Path, selected_volume_uuid: str | None, test_root: Path | None)` and `Settings.from_env() -> Settings`.
- Define `Card(volume_uuid: str, mount_path: Path, device_id: str)`, `Photo(relative_path: str, size: int, mtime_ns: int)`, and `Batch(card: Card, files: tuple[Photo, ...], inventory_id: str, total_bytes: int)`.
- `discover_card(settings: Settings) -> Card | None`; ambiguous, unknown, or failed observations raise `ObservationError`, not `None`.
- `scan_photos(card: Card) -> Batch`; a valid empty card returns a batch with zero files.
- Define `Session` with the versioned fields specified above, per-file `ArchiveEntry(relative_path: str, size: int, sha256: str)`, and `Verification(session_id: str, inventory_id: str, destination: Path, manifest_sha256: str, count: int)`.
- `read_session(settings: Settings) -> Session | None`; absence is `None`, invalid JSON/schema is an error.
- `verify_archive(batch: Batch, session: Session) -> Verification`; mismatch/unreadability raises `ObservationError`.
- `sense(name: str, settings: Settings) -> ProbeResult`, with `ProbeResult(exit_code: int, stdout: str)`; CLI maps exceptions to exit 2 with stderr diagnostics.

- [ ] **Step 1: Configure and prepare uv.** Create only the example-local metadata described above, run `uv sync --project uncharles/examples/photo-archiver-uncharles --python 3.14`, then run `uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 --no-sync --frozen --offline --no-python-downloads python --version`. The version must be 3.14 or newer; do not create a root uv workspace, script tests, or test dependencies.
- [ ] **Step 2: Implement the typed observation interfaces.** Define the listed settings/card/batch/session/proof types, schema loading, diskutil plist discovery, deterministic Canon photo enumeration, and exact-set/count/size/hash verification. Normalize/contain paths, use subprocess argv arrays with a 10-second diskutil timeout, and hash in 1 MiB chunks.
- [ ] **Step 3: Implement all sensor predicates and the CLI.** Keep captured JSON compact, with no full filename manifest in env values. Verification/hash mismatches produce a false verification predicate; unexpected discovery/schema/configuration failures produce exit 2. Enforce Python 3.14+ before helper I/O and preserve sensor purity in real/mock modes.
- [ ] **Step 4: Perform a manual isolated sensor smoke check.** Under a temporary `PHOTO_ARCHIVER_TEST_ROOT`, invoke the sensor CLI with an absent card, an empty `card/DCIM`, then synthetic JPG/CR3 files in separate camera directories. Check false/true outcomes and a stable capture with the expected count; inspect the tree before/after repeated probes to confirm no state, output, notification, or bytecode files were created. Keep fixtures/logs temporary; do not turn this check into a script test suite.
- [ ] **Step 5: Review and commit the observation slice.** Suggested subject: `feat(uncharles): add read-only photo archiver sensors`.

### Task 2: Safe, resumable copy action — about 25 min

**Files:** finish action-only persistence helpers in `scripts/session.py`; create `scripts/actions.py`.

**Interfaces:**
- Consumes Task 1 `Settings`, `Batch`, `Session`, and observation/error types.
- `action_lock(settings: Settings) -> ContextManager[None]`, `write_session_atomic(settings: Settings, session: Session) -> None`.
- `copy_batch(settings: Settings, captured_batch: dict[str, object]) -> Session`.
- Action CLI `copy` consumes `UNCHARLES_FACT_PHOTO_BATCH` (the compact capture), not positional shell-expanded paths; exits 0 only on successful/idempotent completion, otherwise 1 with stderr.

- [ ] **Step 1: Implement action locking and atomic session persistence.** Create state/lock files only from actions, validate all source/output boundaries, and re-read the active session after acquiring the lock. Preserve immutable completed manifests and enough per-file progress for resume.
- [ ] **Step 2: Implement `copy_batch` and the `copy` CLI.** Bind the captured batch to freshly observed source identity/inventory, allocate destination/session ID once, stream-copy/hash into owned temporary files, and complete only after every expected file and the final inventory check succeed. Matching existing completed files may be skipped after hashing; mismatching files are never silently overwritten.
- [ ] **Step 3: Implement failure and idempotence handling.** Preserve source/partial evidence and persist a matching failed session on action errors. Under the lock, repeated/stale copy invocations must not allocate a duplicate archive for a completed matching session. Reject disallowed output/source overlap.
- [ ] **Step 4: Perform a manual isolated copy smoke check.** Copy a temporary JPG/RAW inventory containing duplicate basenames in different directories and a filename with spaces. Inspect the extraction-date folder, preserved paths, byte sizes, source/destination hashes, and completed manifest; repeat the same action and confirm the destination is unchanged. Copy alone must not create a notification receipt. Review resume/failure branches for source preservation without adding script tests.
- [ ] **Step 5: Review and commit the copy slice.** Suggested subject: `feat(uncharles): add resumable camera photo copying`.

### Task 3: Verified notification and session reset — about 15 min

**Files:** extend `scripts/actions.py` and receipt observations in `scripts/sensors.py` if necessary.

**Interfaces:**
- Consumes Task 1 `Verification` and Task 2 persistence/locking helpers.
- `notify_verified(settings: Settings, captured_batch: dict[str, object], captured_verification: dict[str, object]) -> None`.
- `reset_session(settings: Settings, expected_session_id: str) -> None`.
- CLI `notify` consumes `UNCHARLES_FACT_PHOTO_BATCH` and `UNCHARLES_FACT_ARCHIVE_VERIFIED`; `reset-session` consumes `UNCHARLES_FACT_SESSION_OBSOLETE` or explicit `--session-id <ID>` for manual recovery.

- [ ] **Step 1: Implement `notify_verified` and the `notify` CLI.** Revalidate source/session identity, manifest digest, exact photo set, and hashes under the action lock before submitting the exact notification text. Pass message text as argv to a fixed AppleScript adapter with a 10-second timeout; mock mode writes only to its temporary notification sink.
- [ ] **Step 2: Persist submission receipts and failure state.** Write a separate receipt only after successful submission, leaving the completed manifest/digest unchanged. Repeated calls with an existing matching receipt do not resubmit. Failed/stale proof or failed notification submission cannot create a success receipt.
- [ ] **Step 3: Implement token-bound `reset_session` and its CLI.** Clear only the matching active record; stale session IDs leave current bookkeeping untouched. Never delete complete/partial archives or touch the card.
- [ ] **Step 4: Perform a manual isolated verification/notification/reset smoke check.** With synthetic photos and the mock notification sink, obtain a sensor-produced verification proof and submit one notification. Repeat and inspect the sink/receipt for no duplicate. Use a fresh unnotified temporary session to change destination bytes without changing file count; verification must fail and no success receipt may appear. Reset its active record and confirm archives remain. Do not create automated script tests.
- [ ] **Step 5: Review and commit the notification slice.** Suggested subject: `feat(uncharles): notify after verified photo imports`.

### Task 4: YAML automaton and compiled-binary config/CLI tests — about 15 min

**Files:** create example `config.yaml` and `uncharles/tests/photo_archiver.rs`.

**Interfaces:**
- Consumes the exact sensor/action names, env payloads, preconditions, and staged effects defined in this document.
- The config uses argv lists starting with `uv`, `run`, `--project`, `uncharles/examples/photo-archiver-uncharles`, `--python`, `3.14`, `--no-sync`, `--frozen`, `--offline`, `--no-python-downloads`, `python`, `-B`, then the corresponding `scripts/{sensors,actions}.py` path and its subcommand; goal requires `[idle]`.
- Rust tests invoke `env!("CARGO_BIN_EXE_uncharles")` with workspace-root `current_dir`, using only `run --dry-run` and `inspect`. They validate the compiled config/CLI interface without executing/testing Python scripts and therefore do not require a provisioned Python environment.

- [ ] **Step 1: Write failing compiled-binary config/CLI tests.** Add `photo_archiver_dry_run_has_expected_schema` and `photo_archiver_inspection_runs_without_commands` in `uncharles/tests/photo_archiver.rs`. Check valid parsing, the expected sensor/action names and idle goal, and successful CLI output without creating state/output/notification artifacts. Inspect the config structure to pin notification's required `archive_verified` fact and the absence of that fact from every action's success effects. Do not invoke Python scripts or add a Python-suite Cargo wrapper.
- [ ] **Step 2: Run RED.** `cargo test -p uncharles --test photo_archiver -- --nocapture`. Confirm the expected missing-config assertions. If an existing uncharles defect or missing capability prevents the check, retain the evidence and mark it blocked; do not repair the runtime.
- [ ] **Step 3: Write `config.yaml` with the tables above.** Invoke scripts through the specified `uv run` argv, declare both effects of `photo_batch` together, use the specified recoverable failure effects, and gate notification on the sensor-derived verification fact. Obsolete-session reset precedes copying through `forbids`. Do not add runtime features or turn verification into an action to make the graph pass.
- [ ] **Step 4: Verify config/CLI behavior.** Repeat the Task 4 test command, then run `cargo run -p uncharles -- run --config uncharles/examples/photo-archiver-uncharles/config.yaml --dry-run` and `cargo run -p uncharles -- inspect --config uncharles/examples/photo-archiver-uncharles/config.yaml --format json`. Report actual passes/failures and any staged-model inspection findings; a runtime limitation is a cycle-end finding, not permission for a runtime fix.
- [ ] **Step 5: Review and commit the automaton slice if its checks pass.** Suggested subject: `feat(uncharles): wire the photo archiver automaton`. If blocked, retain the draft and report the blocker instead of claiming completion.

### Task 5: Usage documentation, final acceptance, and findings — about 10 min

**Files:** create example `README.md` and `.gitignore`; add the example link to `uncharles/README.md`.

**Interfaces:** documentation must match the implemented config, environment variables, failure policy, and staged daemon semantics.

- [ ] **Step 1: Document setup and safe previews.** Require Python **3.14+**, uv, and a macOS user session with removable-volume/Pictures access and Notification Center permissions. Document the local `pyproject.toml`, explicit uv setup command, committed lockfile, `uv run ... python --version` check, and uv-based sensor/action argv. Environment provisioning occurs before the daemon, with no-sync/frozen/offline/no-download flags during polling. Explain automatic SD classification versus explicit USB-reader selection, supported formats/DCIM restriction, one-card selection, preserved bytes/paths, dated naming, reinsertion/restart policy, source preservation, and no post-import corruption watchdog. `--dry-run` and `inspect` execute neither sensors nor actions; a plain plan executes read-only sensors. Do not advertise `--once` as a complete import.
- [ ] **Step 2: Document run, mock demo, and failure recovery.** From the workspace root:
  ```sh
  cargo run --release -p uncharles -- run \
    --config uncharles/examples/photo-archiver-uncharles/config.yaml \
    --execute --interval-ms 2000 --max-iterations 100000 --pretty
  ```
  The action cap is still finite; the CLI default is only 100. Include Ctrl+C/SIGTERM behavior, a test-root demo with synthetic JPG/RAW files, direct sensor troubleshooting (runtime sensor stderr is suppressed), and token-bound manual reset after stopping the daemon. Warn that resetting a failed session retains partial archives and starts a fresh import; ordinary interrupted `copying` sessions resume. No launchd installation or tool auto-install action.
- [ ] **Step 3: Run workspace gates and a manual isolated workflow demo.** Run `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --workspace`; request active LSP diagnostics for changed Rust/Python paths when supported. No Python-script test command or suite is planned. Separately, run the daemon against a temporary `PHOTO_ARCHIVER_TEST_ROOT`, insert synthetic JPG/RAW fixtures after startup, inspect the copied archive/verification/notification artifacts, wait several polls for no repeats, then observe eject/reinsert and a distinct second import. Stop with Ctrl+C/SIGTERM and retain logs. Use receipts/artifacts and actual sensor readings, not optimistic `idle` events, as evidence of completion. If uncharles fails or lacks a feature, stop that affected check, preserve evidence, and include it in the cycle-end report; do not fix the runtime.
- [ ] **Step 4: Perform a separately approved macOS hardware check.** Use a dedicated test SD card, isolated Pictures/state overrides, one JPEG and one Canon RAW fixture. Confirm real diskutil classification, insertion detection, extraction-date folder, byte-identical source/destination, notification submission/banner, no repeat while mounted, and reimport after observed eject/reinsert. Verify an unrelated USB disk is ignored. Do not touch a card containing irreplaceable photos for the acceptance experiment. If hardware is unavailable, record this as unverified, not passing.
- [ ] **Step 5: Review scope and report cycle-end findings.** Confirm only the example/necessary uv metadata, compiled config/CLI integration test, and README link changed; no Python-script tests or runtime/library modifications. Run `git diff --check`. Report implemented example behavior, actual gate/manual-check results, unverified hardware checks, and every observed uncharles failure/missing feature with command, expected/actual behavior, evidence, and impact. Do not open issues, add ADRs, or implement fixes/features unless separately requested. Commit documentation only with truthful status; suggested subject: `docs(uncharles): document the camera photo archiver example`.

## Repository evidence and scope boundaries

- `uncharles/README.md`, `src/actors/runtime.rs`, and `src/actors/goal_supervisor.rs`: execute mode is already perpetual; no-plan and goal-satisfied are nonterminal in daemon mode. The initial sensor sweep is ordered, but continuous sensing is parallel.
- `src/config.rs`, `src/run.rs`, and `src/actors/executor.rs`: capture is sensor stdout; only required captured facts become action env variables; action effects are optimistic; default action failure is terminal without `on_failure`.
- Existing `configs/pendrive_audit.yaml` illustrates progress/idle/session-reset facts, but its inline shell and wrapper are not templates to copy here.
- Backlog checked on **2026-10-13**: no photo-related issue found. Compose with [#19](https://github.com/leopepe/automata-atelier/issues/19) (sensor purity/ordering), [#20](https://github.com/leopepe/automata-atelier/issues/20) (idle-goal limitations), [#17](https://github.com/leopepe/automata-atelier/issues/17) and [#58](https://github.com/leopepe/automata-atelier/issues/58) (existing watcher/runtime work), rather than implementing new runtime features in this example.
- Read `docs/adrs.md` before implementation. This example uses existing ADR 0003/0005 mechanisms, not a new integration schema or concurrency architecture. If the example exposes a failure or needs a missing uncharles feature/interface, mark the affected work blocked and report it at cycle end. Do not change runtime/goal semantics, open an issue/ADR, or implement a fix/feature in this cycle.
- macOS references: [diskutil plist interface](https://www.manpagez.com/man/8/diskutil/osx-10.13.1.php), [Apple notification API](https://developer.apple.com/library/archive/documentation/LanguagesUtilities/Conceptual/MacAutomationScriptingGuide/DisplayNotifications.html). Specific SD-reader classification remains subject to the real-device acceptance check.

## Conductor execution authorization

The operator authorizes implementation through the **Conductor agent at `~/Workspace/conductor/`**, with metrics and mistake ledger enabled. Convert this existing plan into canonical proposal/design/spec/tasks artifacts; skip Explore and Propose by explicit operator overrides, not by forging a handover or claiming the unimplemented Prepared Change route exists. Retain applicable coherence, review, verification, and archive gates.

Binding adjustments: **Python 3.14+**, **an example-local `pyproject.toml` with uncharles invoking scripts through `uv run`**, **no Python-script tests**, and **report uncharles failures/missing features at cycle end without runtime fixes/features**. Preserve strict sensor/action separation and the rest of the described workflow. Conductor controls implementation dispatches and records actual evidence; no push, PR, merge, public publication, or hardware/photo access without separate approval.
