# Camera photo archiver

A foreground macOS **sense → plan → act** example: observe a mounted camera
SD card, copy supported photographs without changing the card, verify the
archive, then submit a completion notification. No runtime/library changes,
Photos.app integration, conversion, auto-ejection, launchd installer, or
cross-import deduplication.

## Prepare first

Run every command below from the **workspace root**. YAML script/project paths
are relative to that working directory, not to this config. You need `uv`,
Python **3.14+**, and a built `uncharles`. Real operation also needs a macOS
user session, removable-volume/Pictures access, and Notification Center
permissions. Install tools yourself; no action installs anything.

This directory is a dependency-free local Python project (`package = false`),
with a committed `uv.lock`; there is no root uv workspace or test dependency.
Prepare its environment **before** starting the daemon:

```sh
uv sync --project uncharles/examples/photo-archiver-uncharles --python 3.14
uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 \
  --no-sync --frozen --offline --no-python-downloads python --version
```

The version check must show 3.14 or newer. Every helper in YAML uses:

```sh
uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 \
  --no-sync --frozen --offline --no-python-downloads python -B \
  uncharles/examples/photo-archiver-uncharles/scripts/sensors.py photo_batch
```

`--no-sync`, `--frozen`, `--offline`, and `--no-python-downloads` disable runtime
syncing, lock updates, network use, and interpreter downloads. `-B` disables
bytecode writes. Missing preparation is an error, not permission to provision
from a sensor. Sensors create no application files, directories, locks, or
markers. These flags alone do **not** prove that a particular uv version never
writes launcher/environment artifacts: inspect the prepared project and `.venv`
before/after polling. The isolated manual check with uv 0.13.0 / Python 3.14.7
observed no artifact changes; this is not a guarantee for other installations.

## What is selected and copied

- Automatic discovery uses read-only `diskutil list -plist physical` and
  `diskutil info -plist` observations. It requires identifiable **Secure
  Digital** removable/ejectable physical media, and rejects the startup volume,
  images, unmounted/unrelated volumes, missing classification, and ambiguity.
- A USB SD reader may report only `USB`. Set `PHOTO_ARCHIVER_VOLUME_UUID` to the
  known card's volume UUID to select it explicitly; it must still be mounted,
  removable/ejectable physical media. One card is selected at a time; selection
  is never guessed from `/Volumes` or `DCIM` alone. Reader plist fields vary:
  real hardware classification has **not** been accepted by the synthetic demo.
- Regular non-symlink files recursively under `DCIM/` with case-insensitive
  `.jpg`, `.jpeg`, `.cr2`, `.cr3`, and `.crw` extensions are recognized. Videos,
  other files, and symlinks are excluded. Directory/extension recognition is
  **not** proof of Canon manufacture, a valid image, or decoded EXIF content.
- Archives use local **extraction time**, not EXIF capture time:
  `~/Pictures/YYYY-MM-DD_HH-MM-SS-<session-id>/`. Bytes and source-relative paths
  such as `DCIM/100CANON/IMG_0001.JPG` are preserved, including duplicate
  basenames in different directories and spaces/Unicode/quotes/newlines.
- Source identity is volume UUID + normalized mount path + deterministic
  path/size/mtime inventory ID. Inventory ID is not an integrity hash. Full
  verification compares exact photo paths, count, sizes, and SHA-256 against
  the still-mounted source and immutable completed copy manifest, in bounded
  1 MiB chunks. Leave source and archive files unchanged during an import.
- An empty card produces no copy. A card already mounted at startup is eligible.
  There is one import per **observed insertion**; no repeated import while the
  same unchanged batch stays mounted. Restart resumes/skips the persisted
  unchanged session. Changed inventory retires active bookkeeping and imports
  a new complete batch, retaining old archives.
- Polling cannot detect removal/reinsertion wholly between polls or while
  stopped. There is no global deduplication. Once notified, verification can
  use the saved import-time proof: **no ongoing corruption watchdog** is claimed.

Defaults/overrides:

| Variable | Meaning |
| --- | --- |
| `PHOTO_ARCHIVER_PICTURES_DIR` | Output root; default `~/Pictures` |
| `PHOTO_ARCHIVER_STATE_DIR` | Active state/lock root; default `~/Library/Application Support/uncharles-photo-archiver/` |
| `PHOTO_ARCHIVER_VOLUME_UUID` | Optional explicit known-card selector |
| `PHOTO_ARCHIVER_TEST_ROOT` | Confined synthetic mode, described below |

State, output, and source trees must not overlap. Saved paths and symlink
components beneath configured roots are validated; unsupported/malformed
records are errors, never permission to overwrite or delete data. The example
assumes the operator controls these directories; do not run other writers
against them concurrently.

## Preview and run

These two commands run **neither sensors nor actions**, and require no helper
execution:

```sh
cargo run -p uncharles -- run \
  --config uncharles/examples/photo-archiver-uncharles/config.yaml --dry-run
cargo run -p uncharles -- inspect \
  --config uncharles/examples/photo-archiver-uncharles/config.yaml --format json
```

Inspection simulates every sensor's success effects, including `idle`. Its
clean graph does not prove a complete import or model asynchronous future
sensor readings. A plain plan **does execute read-only sensors**, but not
mutating actions:

```sh
cargo run -p uncharles -- run \
  --config uncharles/examples/photo-archiver-uncharles/config.yaml --pretty
```

Run as a daemon, stop with Ctrl+C/SIGTERM:

```sh
cargo run --release -p uncharles -- run \
  --config uncharles/examples/photo-archiver-uncharles/config.yaml \
  --execute --interval-ms 2000 --max-iterations 100000 --pretty
```

The action cap remains finite; the CLI default is only 100. Signals request a
runtime drain between actions, not cancellation halfway through a copy.

**Do not use `--once` for a complete import.** The goal is staged `idle`:
copying adds optimistic `idle` to make the copy stage reachable, then the idle
sensor removes it until a real receipt exists. Notification requires the
sensor-derived `archive_verified`; no action adds verification optimistically.
Pending verification may mean a nonterminal “no plan” in daemon mode. A preview
may show only the current copy stage, and `--once` can stop after it. Empty-plan
or optimistic idle events are not evidence of verified/notified completion.
Inspect actual manifest, sensor proof, and receipt.

## Safe hardware-free demo

Synthetic mode works without macOS hardware and **never uses real Pictures,
state, or desktop notifications**. It ignores production output/UUID overrides.
Only `<root>/card` is selected; output is `<root>/Pictures`, state is
`<root>/state`, and notification submission appends to
`<root>/notifications.jsonl`. Sensors do not create these paths. Use synthetic
bytes only, not real photos.

In terminal 1, from the workspace root:

```sh
export PHOTO_ARCHIVER_TEST_ROOT="$(mktemp -d)"
printf 'Test root: %s\n' "$PHOTO_ARCHIVER_TEST_ROOT"
cargo run --release -p uncharles -- run \
  --config uncharles/examples/photo-archiver-uncharles/config.yaml \
  --execute --interval-ms 2000 --max-iterations 100000 --pretty
```

In terminal 2, set the **same printed root**, then insert fixtures:

```sh
export PHOTO_ARCHIVER_TEST_ROOT='<the printed root>'
mkdir -p "$PHOTO_ARCHIVER_TEST_ROOT/card/DCIM/100CANON"
printf 'synthetic JPEG' > "$PHOTO_ARCHIVER_TEST_ROOT/card/DCIM/100CANON/a.JPG"
printf 'synthetic RAW' > "$PHOTO_ARCHIVER_TEST_ROOT/card/DCIM/100CANON/b.CR3"
```

Wait for a completed `.uncharles-photo-archive.json` in a dated Pictures
folder and a separate `.uncharles-photo-notification.json`. Observe one sink
line, unchanged source bytes, and no new archive/line over several polls.
Direct sensor troubleshooting uses the uv command above, changing the final
sensor name to `archive_verified`, `user_notified`, `import_failed`, or
`session_obsolete`. Sensor exit 0 is true, 1 false, 2 observation/configuration
error. Captured stdout is machine data; diagnostics go to stderr. The runtime
suppresses sensor stderr, so invoke the sensor directly to investigate.

Simulate **observed** removal and reinsertion, preserving synthetic source:

```sh
mv "$PHOTO_ARCHIVER_TEST_ROOT/card" "$PHOTO_ARCHIVER_TEST_ROOT/card-away"
# Wait until active-session.json is removed by reset_session (several polls).
mv "$PHOTO_ARCHIVER_TEST_ROOT/card-away" "$PHOTO_ARCHIVER_TEST_ROOT/card"
# Wait for a distinct second archive and second notification sink line.
```

Stop terminal 1 with Ctrl+C. Never point mock mode at irreplaceable data.

## Persistence and recovery

Only actions mutate. Writers serialize through `fcntl.flock`, re-read active
state, and publish durable JSON atomically. `active-session.json` and completed
`.uncharles-photo-archive.json` use schema version 1 and contain identity,
extraction timestamp, chosen destination, per-file size/SHA-256, and status
(`copying`, `copied`, `failed`) with failure detail. Completed copy manifests
are immutable. Notification proof/submission receipt is separate, referenced
by active state, so recording it cannot invalidate the manifest digest.

Copy streams into a session-owned `.part` file, persists write-ahead size/hash
progress, then publishes the completed file atomically without clobbering an
existing path. A crash on either side of publication can be reconciled: missing
finals are retried, existing finals are skipped only after the recorded hash
and current source match. Ordinary interrupted `copying` sessions retain the
same session/destination. Different completed bytes are **never overwritten**.
Source files are never removed, renamed, moved, formatted, or ejected.

Copy/notification failures preserve evidence and persist `failed` for the
matching owned session, preventing automatic retry loops. A sensor-detected
integrity mismatch writes no failure marker: it remains unverified/unnotified
and needs operator investigation. The notification action independently
rechecks current source/session/manifest/destination integrity, so an old
capture cannot authorize success after corruption or replacement.

After fixing the cause, **stop the daemon first**, read the session ID from
`active-session.json`, and explicitly reset matching bookkeeping:

```sh
uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 \
  --no-sync --frozen --offline --no-python-downloads python -B \
  uncharles/examples/photo-archiver-uncharles/scripts/actions.py \
  reset-session --session-id '<ID from active-session.json>'
```

Reset preserves **all complete and partial archives** and the card. It does
not repair a corrupted archive. The next daemon run creates a fresh dated
import; review retained partial directories yourself. A stale reset token
cannot clear a newer session. Corrupt/unsupported bookkeeping must be inspected
manually: reset fails rather than hiding it.

Notification title is `Photo import complete`; body is
`Copied and verified <count> photos to <destination>`. `osascript` receives
these as argv data to a fixed program, not interpolated code. A receipt is saved
only after successful submission. It means **submitted to Notification
Center**, not that a banner was visible (Focus/permissions can suppress it).
Normal repeated calls do not resubmit. A crash between submission and receipt
persistence can cause a repeat; exactly-once desktop delivery is not promised.

## Validation boundaries

The retained Rust integration tests exercise only compiled dry-run/inspection
and YAML wiring; they do not run helpers or need Python provisioning:

```sh
cargo test -p uncharles --test photo_archiver -- --nocapture
```

Python behavior is checked through isolated manual invocations, not a script
test suite/framework/Cargo wrapper. Synthetic checks cover source preservation,
resume, corruption/stale proof rejection, notification idempotence, and stale
reset tokens. Real SD discovery/classification, unrelated USB rejection on
hardware, actual Notification Center submission/banner visibility, and real
insertion/ejection acceptance remain **unverified**, requiring separate
approval with a dedicated test card and isolated output/state paths.

If uncharles fails or lacks a feature, retain the command/exit/log/artifacts and
report the limitation. Do not weaken verification, turn it into an action, or
modify the runtime/core libraries to conceal it.
