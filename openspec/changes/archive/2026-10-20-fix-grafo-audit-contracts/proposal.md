## Why

The parent-validated grafo audit at commit `70d543f` identifies recoverable-input panics, ambiguous numerical failure, unchecked representation limits, missing consumer coverage, and misleading benchmark fixtures. The approved handoff `.conductor/fix-grafo-audit-contracts/handoff.md` authorizes repairs F1/F2/F3/F4/F5/F9.

## What Changes

- Reject invalid weights and unsupported graph sizes with domain errors.
- Distinguish reachable-but-overflowing routes from unreachable destinations across six queries; prefer finite alternatives.
- **BREAKING**: make `PathResult::resolve_labels` return `Result<Vec<String>, GraphError>` and migrate consumers atomically.
- Add external-consumer regressions and controlled attribute-scaling fixtures.
- Record the numerical/error contract in a new ADR and update directly affected documentation.

## Capabilities

### New Capabilities

- `grafo-error-contracts`: fallible construction, resolution, numerical search outcomes, reusable-workspace recovery, and representative benchmark controls.

### Modified Capabilities

None. `find openspec/specs -type f` found the unrelated `camera-photo-archiving` capability.

## Impact

`grafo/src/{graph,workspace,lib}.rs`, integration tests, examples, README, scoped guidance, benchmarks/performance evidence, `goap-planner/src/planner.rs`, and ADR/index companions. No new dependencies planned. Exclude F6/F7/F8 cleanup except documentation required by changed contracts. Preserve user-owned investigation paths. Planning ends before implementation; existing config context describing the photo-archiver change is unrelated historical context, superseded for scope by this explicit request.
