## Implementation

Imported from `docs/superpowers/plans/2026-10-13-photo-archiver-uncharles.md`; read its typed interfaces and the canonical proposal/design/spec. The operator authorizes Conductor implementation and skips Explore/Propose. No Python-script tests or runtime/library fixes/features. Side-quest budget: zero for work outside these tasks; declared validation/manual smoke is assigned work. Local exact-scope commits are permitted; no push/PR/merge/public publication or real SD/photo access. Shared helper/session/action contracts make this a single coherent implementation seam; annotations express the actual serialization, not a worker swarm.

- [x] 1. Prepare the dependency-free local pyproject/uv lock with Python >=3.14 and implement typed settings, card discovery/inventory, read-only session loading, verification, and sensor CLI; validate with manual isolated observations through the specified uv run invocation, including sensor/project artifact nonmutation.
      depends-on: none
      touches: uncharles/examples/photo-archiver-uncharles/pyproject.toml, uncharles/examples/photo-archiver-uncharles/uv.lock, uncharles/examples/photo-archiver-uncharles/.gitignore, uncharles/examples/photo-archiver-uncharles/scripts/common.py, uncharles/examples/photo-archiver-uncharles/scripts/session.py, uncharles/examples/photo-archiver-uncharles/scripts/sensors.py
- [x] 2. Implement action-only locking/atomic persistence, identity-bound resumable copy, dated allocation, per-file SHA-256/temporary completion, source-preserving failure/idempotence handling, and copy CLI; manually inspect paths/bytes/manifest and repeat-call destination stability in mock mode.
      depends-on: 1
      touches: uncharles/examples/photo-archiver-uncharles/scripts/session.py, uncharles/examples/photo-archiver-uncharles/scripts/actions.py
- [x] 3. Implement guarded notification, separate proof/receipt persistence, mock notification sink, failed/stale-proof handling, and token-bound reset that retains archives; manually check successful no-repeat submission and equal-count corruption/no receipt with isolated fixtures.
      depends-on: 2
      touches: uncharles/examples/photo-archiver-uncharles/scripts/actions.py, uncharles/examples/photo-archiver-uncharles/scripts/sensors.py
- [x] 4. Add the YAML automaton invoking helpers through uv run with no-sync/frozen/offline/no-download flags, exact identity/fact/failure/staged-idle contracts, and compiled-binary config-only Rust dry-run/inspection tests; run cargo test -p uncharles --test photo_archiver plus real CLI dry-run/inspect. Do not execute or add tests for Python scripts from the Rust test.
      depends-on: 3
      touches: uncharles/examples/photo-archiver-uncharles/config.yaml, uncharles/tests/photo_archiver.rs
- [x] 5. Document uv setup/run/preview/mock/recovery and all limitations in the example README, link it from uncharles README, run workspace format/clippy/test gates and the manual isolated daemon insertion/copy/verification/notification/no-repeat/eject-reinsert demo, and retain exact check/failure evidence for Conductor's final report. Report uncharles defects/missing features without fixes. Real SD-card acceptance is separately approved or explicitly unverified; it is not a hidden prerequisite or invented pass.
      depends-on: 4
      touches: uncharles/examples/photo-archiver-uncharles/README.md, uncharles/README.md

## Gate evidence and report boundary

Validation commands from workspace root:
- `cargo test -p uncharles --test photo_archiver -- --nocapture`
- `cargo run -p uncharles -- run --config uncharles/examples/photo-archiver-uncharles/config.yaml --dry-run`
- `cargo run -p uncharles -- inspect --config uncharles/examples/photo-archiver-uncharles/config.yaml --format json`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `git diff --check`

Manual helper invocation prefix: `uv run --project uncharles/examples/photo-archiver-uncharles --python 3.14 --no-sync --frozen --offline --no-python-downloads python -B` followed by the workspace-relative helper and subcommand. Prepare the environment before polling, and use only synthetic data under a temporary `PHOTO_ARCHIVER_TEST_ROOT` for this cycle. Preserve logs rather than creating automated script tests. Active LSP diagnostics are an additional check when available, never a substitute for commands.

Mark a task complete only when its implementation and assigned check evidence support completion. If an existing uncharles defect/missing feature blocks a task/check, record the blocked status and exact evidence without editing runtime/library paths or disguising it as a pass. Retain `.conductor/photo-archiver-uncharles/metrics.json`, `ledger.md`, routing, and final findings even if the cycle stops; private cycle state is not part of a publication commit.
