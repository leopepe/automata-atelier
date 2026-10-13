## Why

Importing camera photos by hand can leave an incomplete archive or no clear completion signal. Add a runnable uncharles example that observes a mounted camera SD card, safely copies supported photos into a dated Pictures directory, verifies the archive, and then notifies its owner.

## What Changes

- Add `uncharles/examples/photo-archiver-uncharles/` with YAML config, Python-only `scripts/`, and usage documentation.
- Add an example-local dependency-free `pyproject.toml` requiring Python 3.14+; uncharles invokes helpers through `uv run --project` rather than directly invoking a virtualenv interpreter.
- Observe card identity, JPEG/Canon RAW inventory/count/bytes, copy completion, integrity, notification receipt, obsolete session, and idle/failed state through read-only sensors.
- Copy supported DCIM photographs into a local-extraction-date Pictures directory, preserve source-relative paths, and resume only a matching session without silently overwriting mismatching files.
- Verify exact photo paths, counts, sizes, and SHA-256 bytes before submitting a macOS completion notification; retain a separate receipt without modifying the completed copy manifest.
- Provide an isolated hardware-free mock mode and manual smoke checks. Add only compiled-binary YAML/config/CLI tests; do not create tests for Python scripts.
- Retain cycle evidence, metrics, and mistake ledger privately; report uncharles runtime failures/missing features without implementing fixes or new runtime features.

## Capabilities

### New Capabilities

- `camera-photo-archiving`: Event-driven camera-card import example with read-only sensing, safe copying, sensor-confirmed verification, and notification.

### Modified Capabilities

None. Existing uncharles runtime and core-library requirements are unchanged.

## Impact

Implementation is limited to the example, its compiled config/CLI integration test, and the uncharles README link. Canonical OpenSpec artifacts, the existing build-plan/index update, and scratch-state ignore rules support the requested Conductor cycle. No root uv workspace, new Python/Rust dependency, runtime/library edit, Python-script test suite, automatic ejection, Photos.app import, or global deduplication is included.

This is an import of the existing build plan, not a newly dispatched Propose phase. The operator explicitly authorizes implementation through the trusted Conductor agent and skips Explore/Propose. Remaining applicable gates still run. No public GitHub actions or real SD-card/photo access are authorized.
