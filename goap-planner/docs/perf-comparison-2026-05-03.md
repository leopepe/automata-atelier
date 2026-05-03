# Performance comparison — 2026-05-03 (issue #18 / ADR 0003)

This run validates that ADR 0003's value-carrying-fact mechanism, implemented
in PR closing #18, has **no measurable impact** on `goap-planner` — the
crate's source under `src/` is byte-identical between the baseline and head
runs. The entire feature lives in `uncharles`; the planner's `State`,
`Action`, `Goal`, and `Planner::plan` are unchanged.

## Why capture it

1. **Honour the workspace rule.** `docs/performance-tests.md` requires a
   numerical comparison for any change that *could* affect runtime
   performance. Even when the expectation is "no movement", the comparison
   is the evidence.
2. **Establish a same-machine noise floor for this branch.** The 2026-05-02
   validation rerun (also against unchanged source) measured ±5 % drift on
   the same hardware. Re-confirming that floor on this branch makes future
   regression triage easier.
3. **Pin the ADR's confirmation gate.** ADR 0003 commits to "the planner
   stays Boolean; benchmarks should report no movement on `goap-planner`'s
   headline numbers" as one of three confirmation criteria. This file is
   the artifact for that gate.

## Setup

- **Date:** 2026-05-03
- **Profile:** release
  - Baseline: `cargo bench -p goap-planner --bench performance -- --save-baseline pre-issue-18`
  - Head:     `cargo bench -p goap-planner --bench performance -- --baseline pre-issue-18`
- **Criterion settings:** defaults — 3 s warm-up, 5 s measurement, 100 samples
- **Platform:** macOS Darwin 25.3.0
- **Raw log:** [`bench-2026-05-03-issue-18.txt`](./bench-2026-05-03-issue-18.txt)
- **Source delta in `goap-planner/src/`:** none (zero lines changed)
- **Source delta in `uncharles/src/`:** `SensorSpec.capture` field, `Values`
  type alias, env-var injection in `execute_action`, `Outcome.values` and
  `LoopEvent::Sensed.values` for observability. None of those run during
  `goap-planner` benches.

## Results

All 31 individual benches. Δ is `(head − baseline) / baseline × 100` from
criterion's reported change point estimate. Negative is faster, positive is
slower. Markers: `* ` ≥ 5 % drift, `!!` ≥ 10 % drift. The CI regression gate
(`detect-bench-regression.py`, `REGRESSION_THRESHOLD_PCT=10`) fires only on
robust regressions ≥ 10 %; nothing here approaches that.

| Bench | Head (median) | Δ | Verdict |
|---|---:|---:|---|
| planning/chain/steps/5 | 3.23 µs | -1.0 % | improved (noise) |
| planning/chain/steps/10 | 5.94 µs | +0.5 % | flat (p = 0.07) |
| planning/chain/steps/20 | 11.65 µs | +1.7 % | regressed (within noise) |
| planning/chain/steps/50 | 32.52 µs | +1.2 % | regressed (within noise) |
| planning/wide/branches/8 | 10.27 µs | +0.4 % | flat (p = 0.14) |
| planning/wide/branches/32 | 51.07 µs | +3.9 % | regressed (within noise) |
| planning/wide/branches/128 | 358.62 µs | +3.4 % | regressed (within noise) |
| planning/wide/branches/512 | 5.43 ms | +1.6 % | regressed (within noise) |
| planning/redundant_paths/paths/2 | 3.24 µs | +4.1 % | regressed (within noise) |
| planning/redundant_paths/paths/4 | 5.88 µs | +2.5 % | regressed (within noise) |
| planning/redundant_paths/paths/8 | 11.38 µs | +2.1 % | regressed (within noise) |
| planning/redundant_paths/paths/16 | 23.58 µs | +1.2 % | regressed (within noise) |
| planning/redundant_paths/paths/32 | 51.52 µs | +0.8 % | regressed (within noise) |
| planning/boundaries/already_satisfied_x32 | 205 ns | +1.5 % | regressed (within noise) |
| planning/boundaries/unreachable | 3.54 µs | +1.3 % | regressed (within noise) |
| ops/state/contains_hit_x32 | 175 ns | +2.4 % | regressed (within noise) |
| ops/state/contains_miss_x32 | 87.5 ns | -0.1 % | flat (p = 0.21) |
| ops/state/insert | 42.5 ns | -8.9 % | improved (noise outlier) |
| ops/state/from_facts/1 | 33.7 ns | +1.5 % | regressed (within noise) |
| ops/state/from_facts/10 | 225 ns | +3.8 % | regressed (within noise) |
| ops/state/from_facts/100 | 2.23 µs | +1.3 % | regressed (within noise) |
| ops/action/applicable_met_x32 | 536 ns | +1.1 % | regressed (within noise) |
| ops/action/applicable_unmet_x32 | 459 ns | +0.2 % | flat (p = 0.55) |
| ops/action/apply | 128 ns | +1.9 % | regressed (within noise) |
| ops/goal/trivial_one_required_x32 | 203 ns | +1.6 % | regressed (within noise) |
| ops/goal/compound_10_req_10_forbid_x32 | 2.45 µs | -0.6 % | flat (p = 0.85) |
| ops/goal/unmet_required_x32 | 116 ns | +2.8 % | regressed (within noise) |
| concurrent_plans/sequential_64 | 770 µs | `* ` +5.0 % | regressed (within noise) |
| concurrent_plans/parallel_64_rayon | 151 µs | +2.0 % | regressed (within noise) |
| concurrent_plans/parallel_rayon/8 | 54.6 µs | `* ` +5.0 % | regressed (within noise) |
| concurrent_plans/parallel_rayon/32 | 98.6 µs | +1.2 % | regressed (within noise) |
| concurrent_plans/parallel_rayon/128 | 235 µs | +4.0 % | regressed (within noise) |

## Interpretation

- **No bench crosses the 10 % CI gate.** The largest reported regressions are
  +5.0 % (`concurrent_plans/sequential_64`, `concurrent_plans/parallel_rayon/8`).
  The CI's robust regression detector requires the **lower bound** of the
  confidence interval to clear 10 %; both of these have lower bounds at
  +3.5 % and +3.6 % respectively, well below the gate.
- **The `+ ~2-5 %` skew is consistent with same-machine, same-source drift.**
  The 2026-05-02 validation rerun (which also benched unchanged source against
  itself) reported a similar magnitude of drift, biased the *other* direction
  (mostly negative). Build-to-build drift on this hardware sits in the ±5 %
  band; nothing here is signal.
- **The single outlier — `ops/state/insert: -8.9 %` — is noise, not a real
  improvement.** No code path involved in `State::insert` was touched. The
  bench is a sub-50 ns single-call canary subject to the documented absolute
  jitter floor described in `docs/performance-tests.md`'s "Sub-50ns benches"
  section. Treating it as evidence either way would be over-reading the
  data.

## Conclusion

ADR 0003's confirmation gate (1) — "`pre-` and `post-` numbers must agree
within criterion's noise band" — is satisfied. The planner is unchanged,
the bench numbers reflect that, and the CI gate would pass this PR cleanly.
No flamegraph captured; no headline numbers moved, so
`goap-planner/docs/performance.md` is left untouched.
