# Grafo audit repairs action plan

Use the canonical [OpenSpec tasks](../../../openspec/changes/archive/2026-10-20-fix-grafo-audit-contracts/tasks.md) for the action plan; this pointer avoids duplicating its checklist.

## Planning artifacts

- [Proposal](../../../openspec/changes/archive/2026-10-20-fix-grafo-audit-contracts/proposal.md): approved F1/F2/F3/F4/F5/F9 scope.
- [Design](../../../openspec/changes/archive/2026-10-20-fix-grafo-audit-contracts/design.md): exact signatures, errors, algorithm and consumer migration.
- [Specification](../../../openspec/changes/archive/2026-10-20-fix-grafo-audit-contracts/specs/grafo-error-contracts/spec.md): observable behavior and acceptance scenarios.

## Execution boundary

All eight tasks are complete. The six approved findings have functional fixes, per-issue tests and independent source review. On 2026-10-20 the operator explicitly waived performance-test failures for this change; task 3.3 closes under that waiver, not a passing benchmark claim. Raw failures, rejected experiments and profiling limitations remain in the [performance comparison](../../../grafo/docs/perf-comparison-2026-10-20.md). Local archival/spec sync may proceed. Preserve user-owned investigation artifacts; no commits or public operations are part of this plan.
