# Performance comparison — 2026-05-02 (validation re-run)

This is a validation re-run of the goap-planner bench suite against an
**unchanged library**. The only commits since `bench-baseline.txt` was
captured (`fb4181b`) are CI-workflow changes — PR #5 (reduced criterion
sampling for CI), the active PR #6 (CI subset mode), and the bench-gate
fragility follow-up. Neither affects local benchmark performance: the
source under `goap-planner/src/` is byte-identical to the baseline
commit.

## Why capture it

1. **Refresh** the canonical [`performance.md`](./performance.md)
   numbers so the latest "current state" cited from the README stays
   recent.
2. **Document run-to-run variance** on this machine so future regression
   triage has a reference for what same-source drift looks like — the
   bench-gate fragility follow-up in `docs/todo.md` will need exactly
   this kind of empirical noise floor.

## Setup

- **Date:** 2026-05-02
- **Profile:** release (`cargo bench -p goap-planner --bench performance -- --save-baseline 2026-05-02`)
- **Criterion settings:** defaults — 3 s warm-up, 5 s measurement, 100 samples
- **Platform:** macOS Darwin 25.3.0, Rust stable (1.95.0)
- **Raw log:** [`bench-2026-05-02.txt`](./bench-2026-05-02.txt)
- **Compared against:** [`bench-baseline.txt`](./bench-baseline.txt) (initial sweep, same date)
- **Flamegraph:** not captured this round, consistent with the baseline (which has the same gap; tracked as a follow-up in PR #4's commit message).

## Results

All 32 individual benches. Δ is `(new − baseline) / baseline × 100`.
Negative is faster, positive is slower. Markers: `* ` ≥ 5 % drift,
`!!` ≥ 10 % drift.

| Bench | Baseline (median) | 2026-05-02 (median) | Δ |
|---|---:|---:|---:|
| planning/chain/steps/5 | 3.43 µs | 3.26 µs | -4.8 % |
|  `* ` planning/chain/steps/10 | 6.19 µs | 5.88 µs | -5.1 % |
| planning/chain/steps/20 | 11.9 µs | 11.4 µs | -4.1 % |
| planning/chain/steps/50 | 33.3 µs | 31.9 µs | -4.0 % |
|  `* ` planning/wide/branches/8 | 10.8 µs | 10.1 µs | -6.1 % |
|  `* ` planning/wide/branches/32 | 49.7 µs | 47.0 µs | -5.4 % |
|  `* ` planning/wide/branches/128 | 362 µs | 337 µs | -7.0 % |
|  `* ` planning/wide/branches/512 | 5.22 ms | 4.78 ms | -8.4 % |
|  `* ` planning/redundant_paths/paths/2 | 3.29 µs | 3.11 µs | -5.6 % |
|  `* ` planning/redundant_paths/paths/4 | 6.03 µs | 5.64 µs | -6.5 % |
|  `* ` planning/redundant_paths/paths/8 | 11.7 µs | 11.0 µs | -5.5 % |
| planning/redundant_paths/paths/16 | 24.0 µs | 22.9 µs | -4.9 % |
| planning/redundant_paths/paths/32 | 51.9 µs | 50.2 µs | -3.2 % |
| planning/boundaries/already_satisfied | 6.87 ns | 7.17 ns | +4.3 % |
| planning/boundaries/unreachable | 3.53 µs | 3.43 µs | -2.8 % |
|  `* ` ops/state/contains_hit | 5.35 ns | 5.08 ns | -5.1 % |
| ops/state/contains_miss | 2.71 ns | 2.58 ns | -4.7 % |
| ops/state/insert | 905 ns | 879 ns | -2.9 % |
|  `* ` ops/state/from_facts/1 | 33.4 ns | 31.7 ns | -5.0 % |
| ops/state/from_facts/10 | 228 ns | 219 ns | -4.0 % |
| ops/state/from_facts/100 | 2.21 µs | 2.21 µs | -0.1 % |
| ops/action/applicable_met | 17.3 ns | 16.4 ns | -4.8 % |
| ops/action/applicable_unmet | 13.8 ns | 13.3 ns | -3.5 % |
| ops/action/apply | 126 ns | 124 ns | -1.8 % |
|  `* ` ops/goal/trivial_one_required | 6.21 ns | 6.54 ns | +5.2 % |
| ops/goal/compound_10_req_10_forbid | 77.1 ns | 79.3 ns | +2.8 % |
| ops/goal/unmet_required | 3.48 ns | 3.48 ns | -0.0 % |
| concurrent_plans/sequential_64 | 781 µs | 764 µs | -2.2 % |
| concurrent_plans/parallel_64_rayon | 145 µs | 148 µs | +1.9 % |
| concurrent_plans/parallel_rayon/8 | 54.0 µs | 53.3 µs | -1.2 % |
| concurrent_plans/parallel_rayon/32 | 95.8 µs | 98.6 µs | +2.9 % |
| concurrent_plans/parallel_rayon/128 | 235 µs | 234 µs | -0.5 % |

## Interpretation

- **Range of drift:** -8.4 % to +5.2 % across 32 benches. No bench
  crossed ±10 %.
- **Direction:** 24 of 32 benches measured faster on the 2026-05-02 run,
  6 slower, 2 unchanged. Skew is mild but consistent — likely better
  CPU thermal headroom and/or quieter background load on this run.
- **No bench crossed the CI gate's 10 % threshold.** Confirms the
  threshold is set wide enough to absorb single-machine run-to-run
  drift on this hardware. Cross-machine drift (the cache-hit path on
  shared GitHub runners) is wider — that's the separate fragility
  noted in `docs/todo.md`.
- **Largest swings cluster on the planning suite.** The
  `planning/wide/branches/*` cases shift the most (-5.4 % to -8.4 %),
  consistent with these being the longest-running benches and therefore
  most exposed to runtime drift accumulated across the warmup +
  measurement window.

## Decisions

- **Refresh `performance.md` to the new numbers.** The doc tracks
  "latest measured" per workspace policy.
- **No code change, no trade-off.** Source is identical to baseline.
- **No flamegraph.** Consistent with the baseline's known follow-up.
- **Add a History row** in `performance.md` so the prior baseline stays
  visible alongside this run.

## Next benchmark trigger

Per `goap-planner/CLAUDE.md`, the next required bench session is on
any change to `Planner::plan`, `Action::applicable` / `Action::apply`,
`State::signature` / `State::contains` / `State::insert`,
`Goal::satisfied_by`, the BFS bound (`max_states`), or the `grafo`
dependency. None pending.
