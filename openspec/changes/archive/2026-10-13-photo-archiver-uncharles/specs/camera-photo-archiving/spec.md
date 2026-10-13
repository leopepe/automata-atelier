## Purpose

Provide a runnable camera-photo import example that uses uncharles sensors to observe mounted media and archive integrity, and actions to preserve photos in dated directories and notify users after verification.

## ADDED Requirements

### Requirement: Local Python project and uv execution

The example SHALL live at `uncharles/examples/photo-archiver-uncharles/`, contain a local `pyproject.toml` requiring Python 3.14 or newer, and have uncharles launch Python-only helper scripts through uv. It MUST NOT create a root uv workspace, mix shell/Python helper implementations, or place inline programs in YAML. Environment preparation SHALL occur before sensor polling; runtime uv invocations SHALL disable syncing, lockfile updates, network access, and interpreter downloads and Python bytecode writes.

#### Scenario: Prepared runtime execution
- **WHEN** the user has prepared the example's uv environment and starts uncharles
- **THEN** helper commands run through the local uv project with Python 3.14 or newer without provisioning the environment from sensors

#### Scenario: Missing setup
- **WHEN** the prepared environment/interpreter is unavailable
- **THEN** the example does not provision it from a sensor or claim successful photo archiving

### Requirement: Read-only card and photograph sensing

Sensors SHALL independently observe an identifiable selected mounted camera SD card, photo inventory/count/bytes, copy completion, verification, and session state without writing application files. Automatic selection SHALL exclude the startup volume, images, unrelated and unmounted devices and refuse ambiguous/unknown media. An explicit known-volume UUID SHALL permit selection of a qualifying USB-reader card. The supported inventory SHALL contain regular non-symlink files recursively under DCIM with case-insensitive `.jpg`, `.jpeg`, `.cr2`, `.cr3`, or `.crw` extensions.

#### Scenario: Supported card insertion
- **WHEN** an eligible selected mounted card has supported DCIM photographs
- **THEN** sensing exposes identity-bound photo facts and a compact inventory containing the source photo count and bytes

#### Scenario: Empty or untrusted media
- **WHEN** no qualifying selected card exists, the card has no supported photos, or media selection is ambiguous/unknown
- **THEN** the example does not guess a source or start a photo-copy action

### Requirement: Source-preserving dated copy

A copy action SHALL store every recognized photo in a newly allocated `~/Pictures/YYYY-MM-DD_HH-MM-SS-<session-id>/` archive dated by local extraction time and preserve source-relative paths and bytes. It SHALL retain its destination across matching interrupted-session resume. It MUST NOT modify/eject source data, overwrite unrelated Pictures contents, or silently overwrite mismatching completed archive files. Only actions SHALL create directories, locks, manifests, receipts, and active bookkeeping.

#### Scenario: JPEG and RAW with duplicate basenames
- **WHEN** the selected inventory contains JPEG/RAW photos with matching basenames in different source directories
- **THEN** the archive retains both at their source-relative paths with byte-identical contents and the source remains unchanged

#### Scenario: Interrupted matching copy
- **WHEN** a persisted matching copy session is resumed
- **THEN** it retains the same destination, skips only hash-matching completed files, and preserves incomplete evidence rather than silently overwriting different completed bytes

### Requirement: Sensor-confirmed complete integrity

A read-only verification sensor SHALL compare the exact supported photo path set, count, byte sizes, and SHA-256 hashes among the still-mounted source, saved copy manifest, and destination. A success proof SHALL identify session, inventory, destination, immutable manifest digest, and verified count. No action SHALL add the `archive_verified` fact optimistically or implement verification as a standalone action. A notification action SHALL revalidate current identities and integrity before mutation.

#### Scenario: Complete archive
- **WHEN** every expected photo matches the source and manifest by path, count, size, and SHA-256
- **THEN** the sensor emits a matching verification proof permitting the guarded notification action

#### Scenario: Same count with corrupted bytes
- **WHEN** destination photo bytes differ despite an unchanged file count
- **THEN** verification does not succeed and no success-notification receipt is created

### Requirement: Completion notification and immutable copy evidence

After guarded successful verification, a notification action SHALL submit title `Photo import complete` and body `Copied and verified <count> photos to <destination>`. It SHALL pass text as data, persist a separate receipt only after successful submission, and not change the completed manifest whose digest is in its proof. Submission SHALL NOT be represented as proof the user saw the banner. Normal repeated invocations for a receipted matching session SHALL not resubmit; the crash window between submission and receipt persistence remains a documented limitation.

#### Scenario: Verified completion
- **WHEN** current integrity succeeds and notification submission exits successfully
- **THEN** a separate matching receipt is recorded without changing the completed copy manifest

#### Scenario: Failed submission or stale proof
- **WHEN** notification submission fails or the captured proof no longer matches the current source/session/archive
- **THEN** no success receipt is recorded and the action does not claim completed notification

### Requirement: Per-session daemon behavior and recovery

The example SHALL run under existing perpetual daemon semantics with staged idle-goal effects and a 2000 ms default documented cadence. It SHALL not repeat a completed unchanged import while mounted, SHALL create a fresh import after observed ejection/reinsertion, and SHALL resume/skip the persisted unchanged session after restart. A changed inventory SHALL retire obsolete bookkeeping before a fresh batch. Token-bound reset SHALL clear only matching active bookkeeping and preserve archives. Copy/notification action failures SHALL preserve evidence and avoid repeated automatic action retries. The example SHALL document that `--once` is not a full-import mode and fast/stop-time reinsertion can go unobserved.

#### Scenario: Completed card remains mounted
- **WHEN** a matching completed/notified card is sensed repeatedly
- **THEN** repeated polls do not create additional archives or notification submissions

#### Scenario: Observed removal and reinsertion
- **WHEN** the daemon observes removal, resets matching obsolete bookkeeping, and observes reinsertion
- **THEN** it creates a distinct extraction-dated import while retaining prior archives

#### Scenario: Delayed obsolete-session reading
- **WHEN** reset receives an old session token that differs from the current active session
- **THEN** current bookkeeping and archived/source files remain untouched

### Requirement: Isolated validation without Python-script tests

The change MUST NOT create Python-script test files, a script test framework/suite, or a Cargo wrapper for one. Rust integration tests SHALL be limited to compiled uncharles config/CLI parsing, dry-run, and inspection without executing helpers. Manual smoke checks SHALL use `PHOTO_ARCHIVER_TEST_ROOT` with synthetic source, Pictures/state, and notification sink confined to that root; mock mode SHALL not use real Pictures or send desktop notifications. Real hardware/photo acceptance SHALL require separate operator approval and unperformed checks SHALL remain unverified.

#### Scenario: Mock workflow
- **WHEN** the user runs a manual workflow with an explicit isolated test root
- **THEN** synthetic archives/state/notification evidence remain inside it and real photos/notifications are not accessed

#### Scenario: Config-only CLI validation
- **WHEN** the compiled binary dry-runs or inspects this example in its Rust integration tests
- **THEN** the expected config surface is checked without running Python scripts or requiring a provisioned Python environment

### Requirement: Conductor control and report-only runtime limitations

The Conductor agent from the trusted `~/Workspace/conductor/` checkout SHALL control implementation and remaining applicable gates using imported canonical OpenSpec artifacts, with operator-requested Explore/Propose skips recorded explicitly. It SHALL persist metrics and a mistake ledger throughout and retain them if blocked. Existing uncharles failures or missing features SHALL be reported at cycle end with command/evidence/impact and MUST NOT be repaired or implemented in this change. No push, PR, merge, public publication, or real photo/hardware access SHALL occur without separate approval.

#### Scenario: External plan admission
- **WHEN** the existing build plan is admitted for the explicitly authorized implementation cycle
- **THEN** Explore/Propose dispatches are skipped without claiming a nonexistent admission feature or fabricating prior lifecycle/review success

#### Scenario: Runtime blocker
- **WHEN** an observed uncharles failure or missing feature blocks an example check
- **THEN** evidence and metrics/ledger are retained, the affected check is marked blocked, and no runtime/library fix is made
