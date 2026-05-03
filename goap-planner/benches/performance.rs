/// goap-planner performance benchmarks.
///
/// Run with:
///   cargo bench -p goap-planner
///   cargo bench -p goap-planner -- <filter>          # subset, e.g. "planning"
///   cargo bench -p goap-planner -- --save-baseline main   # save baseline
///   cargo bench -p goap-planner -- --baseline main        # compare
///
/// HTML reports are written to target/criterion/.
///
/// Mirrors `grafo/benches/performance.rs` in style: deterministic LCG for
/// shape generation, factory functions per scenario, one bench function per
/// group separated by section headers, criterion_group at the bottom.
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use goap_planner::{Action, Goal, Planner, State};
use rayon::prelude::*;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Deterministic pseudo-random number generator (LCG — no external dep needed)
// ---------------------------------------------------------------------------

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

// ---------------------------------------------------------------------------
// CI sweep gate
// ---------------------------------------------------------------------------

/// Reduce a multi-size sweep to its largest entry when `CI_BENCH_SUBSET`
/// is set in the environment. The largest input dominates the regression
/// signal, so dropping the smaller sizes in CI saves wall time without
/// changing the gate's verdict. Local `cargo bench` (env unset) runs the
/// full sweep so the per-size scaling curve stays visible during
/// development.
fn ci_sample_sizes<T: Copy>(sizes: &[T]) -> Vec<T> {
    if std::env::var_os("CI_BENCH_SUBSET").is_some() {
        sizes.last().copied().into_iter().collect()
    } else {
        sizes.to_vec()
    }
}

// ---------------------------------------------------------------------------
// Nano-bench batching
// ---------------------------------------------------------------------------

/// Inner-loop batch size for benches whose single-call cost is below ~50 ns.
///
/// Criterion's per-sample variance on shared CI runners has an absolute
/// floor of 2-5 ns (cache warm-up state, scheduling slack, neighbour
/// activity in the Azure VM), independent of the bench's per-iter cost.
/// On a 7-30 ns op that's 7-50 % relative noise — large enough that the
/// regression gate fires on identical code between runs.
///
/// Wrapping each call in `for _ in 0..NANO_BATCH` multiplies the
/// observed time by 32, pushing the bench to 200-1000 ns per iter
/// where the same 2-5 ns of absolute jitter shrinks to 0.5-2.5 %
/// relative — comfortably below the 10 % gate. Per-call ns can be
/// derived locally by dividing the reported per-iter time by 32.
///
/// Bench IDs carry an `_x32` suffix so future readers (and criterion's
/// baseline matching) treat them as distinct from any past unbatched
/// runs.
const NANO_BATCH: usize = 32;

// ---------------------------------------------------------------------------
// Scenario factories
//
// Each factory returns (actions, initial_state, goal). The topology is
// deliberately predictable so that BFS state-space size is bounded and the
// numbers are reproducible across runs.
// ---------------------------------------------------------------------------

/// Linear chain plan: `n` actions, each consuming the previous step's marker
/// and adding the next one. Plan length = n. State space explored ≈ n.
///
/// Stresses: per-step `Action::applicable` scan, repeated `State::clone` in
/// `Action::apply`, `State::signature` cost as fact set grows by one each
/// step, and the final Dijkstra over the resulting linear graph.
fn chain_plan(n: usize) -> (Vec<Action>, State, Goal) {
    let mut actions = Vec::with_capacity(n);
    for i in 0..n {
        let pre = if i == 0 {
            "start".to_string()
        } else {
            format!("done_{}", i - 1)
        };
        let post = format!("done_{i}");
        actions.push(
            Action::new(format!("step_{i}"), 1.0)
                .requires(pre.clone())
                .removes(pre)
                .adds(post),
        );
    }
    let initial = State::from_facts(["start"]);
    let goal = Goal::new().requires(format!("done_{}", n - 1));
    (actions, initial, goal)
}

/// Wide branching plan: `n_branches` first-step actions all applicable from
/// the initial state, each producing a distinct branch marker. Only one
/// branch leads to the goal; the rest are dead-ends (an extra trailing
/// action that adds an irrelevant fact).
///
/// Total actions = 2 × n_branches. State space explored ≈ 2 × n_branches.
/// Stresses: action library scan (`O(actions)` per state), distinguishing
/// between productive and dead-end branches via reachability search.
fn wide_branching(n_branches: usize) -> (Vec<Action>, State, Goal) {
    let mut actions = Vec::with_capacity(2 * n_branches);
    for i in 0..n_branches {
        actions.push(
            Action::new(format!("branch_{i}"), 1.0)
                .requires("start")
                .removes("start")
                .adds(format!("branch_{i}")),
        );
        if i == 0 {
            actions.push(
                Action::new("finish".to_string(), 1.0)
                    .requires(format!("branch_{i}"))
                    .adds("goal_done"),
            );
        } else {
            actions.push(
                Action::new(format!("dead_{i}"), 1.0)
                    .requires(format!("branch_{i}"))
                    .adds(format!("dead_{i}")),
            );
        }
    }
    let initial = State::from_facts(["start"]);
    let goal = Goal::new().requires("goal_done");
    (actions, initial, goal)
}

/// Redundant paths plan: `n_paths` parallel two-step routes from start to
/// goal, costs uniformly distributed in [1, n_paths]. The planner must
/// discover all of them and pick the cheapest via the underlying Dijkstra.
///
/// Total actions = 2 × n_paths. State space explored ≈ 2 × n_paths.
/// Stresses: edge-cost selection during state-space construction, the
/// `edge_map.entry().and_modify()` cheap-path tracking, and Dijkstra over
/// a parallel-edge graph.
fn redundant_paths(n_paths: usize) -> (Vec<Action>, State, Goal) {
    let mut actions = Vec::with_capacity(2 * n_paths);
    let mut rng: u64 = 0x9e37_79b9_7f4a_7c15;
    for i in 0..n_paths {
        let cost = (lcg(&mut rng) % n_paths as u64 + 1) as f64;
        actions.push(
            Action::new(format!("step1_{i}"), cost)
                .requires("start")
                .removes("start")
                .adds(format!("mid_{i}")),
        );
        actions.push(
            Action::new(format!("step2_{i}"), 1.0)
                .requires(format!("mid_{i}"))
                .adds("goal_done"),
        );
    }
    let initial = State::from_facts(["start"]);
    let goal = Goal::new().requires("goal_done");
    (actions, initial, goal)
}

// ---------------------------------------------------------------------------
// 1. Planning — chain plans (linear plan length scaling)
// ---------------------------------------------------------------------------

/// How does planning time scale with plan length on a single-path scenario?
/// Expected complexity: O(n) state expansions × per-state action scan.
fn bench_planning_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("planning/chain");

    for &n in &ci_sample_sizes(&[5usize, 10, 20, 50]) {
        let (actions, initial, goal) = chain_plan(n);
        let planner = Planner::new(actions);
        group.bench_with_input(BenchmarkId::new("steps", n), &n, |b, _| {
            b.iter(|| {
                planner
                    .plan(black_box(&initial), black_box(&goal))
                    .unwrap()
                    .unwrap()
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Planning — wide branching (action library size scaling)
// ---------------------------------------------------------------------------

/// How does planning time scale with action library size when most actions
/// are dead-ends? Reveals per-state action-scan cost.
fn bench_planning_wide(c: &mut Criterion) {
    let mut group = c.benchmark_group("planning/wide");

    for &n in &ci_sample_sizes(&[8usize, 32, 128, 512]) {
        let (actions, initial, goal) = wide_branching(n);
        let planner = Planner::new(actions);
        group.bench_with_input(BenchmarkId::new("branches", n), &n, |b, _| {
            b.iter(|| {
                planner
                    .plan(black_box(&initial), black_box(&goal))
                    .unwrap()
                    .unwrap()
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Planning — redundant paths (cheapest-path selection)
// ---------------------------------------------------------------------------

/// Multiple two-step routes from start to goal. Validates that the planner
/// picks the cheapest under increasing parallel-edge density.
fn bench_planning_redundant(c: &mut Criterion) {
    let mut group = c.benchmark_group("planning/redundant_paths");

    for &n in &ci_sample_sizes(&[2usize, 4, 8, 16, 32]) {
        let (actions, initial, goal) = redundant_paths(n);
        let planner = Planner::new(actions);
        group.bench_with_input(BenchmarkId::new("paths", n), &n, |b, _| {
            b.iter(|| {
                planner
                    .plan(black_box(&initial), black_box(&goal))
                    .unwrap()
                    .unwrap()
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Planning — fast paths (already satisfied / unreachable)
// ---------------------------------------------------------------------------

/// Two boundary cases that should be cheap:
///
/// - `already_satisfied`: initial state satisfies the goal — planner returns
///   an empty plan immediately, before any BFS work. Should be ~ns.
/// - `unreachable`: goal can never be reached from initial; BFS exhausts
///   the (small) discovered state space and returns `Ok(None)`. We bound
///   `max_states` so the bench terminates quickly.
fn bench_planning_boundaries(c: &mut Criterion) {
    let mut group = c.benchmark_group("planning/boundaries");

    // Already satisfied — fast path. Batched (see `NANO_BATCH`): the
    // pre-BFS check returns in ~7 ns, well below the runner's noise
    // floor.
    let (actions, _, _) = chain_plan(10);
    let initial = State::from_facts(["already_done"]);
    let goal = Goal::new().requires("already_done");
    let planner = Planner::new(actions);
    group.bench_function("already_satisfied_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(
                    planner
                        .plan(black_box(&initial), black_box(&goal))
                        .unwrap()
                        .unwrap(),
                );
            }
        })
    });

    // Unreachable goal — BFS exhausts the bounded state space.
    let (actions, initial, _) = chain_plan(10);
    let unreachable_goal = Goal::new().requires("never_set_anywhere");
    let planner = Planner::new(actions).with_max_states(64);
    group.bench_function("unreachable", |b| {
        b.iter(|| planner.plan(black_box(&initial), black_box(&unreachable_goal)))
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 5. State micro-ops
// ---------------------------------------------------------------------------

/// Cost of `State::contains`, `State::insert`, `State::from_facts`. These
/// are the per-fact primitives invoked in `Action::applicable` /
/// `Action::apply` and dominate when action effects are small but
/// frequently-invoked.
fn bench_state_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("ops/state");

    // contains — hit and miss paths against a 100-fact state. Batched
    // (see `NANO_BATCH`) because a single hash lookup at ~8 ns is below
    // the runner's noise floor; per-batch reporting keeps the gate
    // signal clean.
    let large_state = State::from_facts((0..100).map(|i| format!("fact_{i}")));
    group.bench_function("contains_hit_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(black_box(&large_state).contains(black_box("fact_42")));
            }
        })
    });
    group.bench_function("contains_miss_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(black_box(&large_state).contains(black_box("nope")));
            }
        })
    });

    // insert — single-fact mutation cost.
    //
    // Uses `iter_batched_ref` (not `iter_batched`) so the State is borrowed
    // across iterations and its drop falls outside the timed window. The
    // earlier `iter_batched` form moved the State into the closure each
    // iteration, which dropped a 100-fact `FxHashSet` *inside* the timed
    // window — `State::insert` is ~50 ns but observed time was ~1 µs, of
    // which ~99 % was drop. See issue #24.
    //
    // Each call inserts a uniquely-named fact, so the state grows by one
    // per batch iteration; FxHashSet inserts are O(1) regardless of size,
    // so this still measures the per-insert cost. Drops happen at batch
    // boundaries, amortised over the whole batch.
    group.bench_function("insert", |b| {
        let template = State::from_facts((0..100).map(|i| format!("fact_{i}")));
        let mut counter: u64 = 0;
        b.iter_batched_ref(
            || template.clone(),
            |s| {
                counter = counter.wrapping_add(1);
                s.insert(black_box(format!("new_fact_{counter}")));
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // from_facts — bulk construction at varying sizes.
    for &n in &ci_sample_sizes(&[1usize, 10, 100]) {
        let inputs: Vec<String> = (0..n).map(|i| format!("fact_{i}")).collect();
        group.bench_with_input(BenchmarkId::new("from_facts", n), &n, |b, _| {
            b.iter(|| State::from_facts(black_box(inputs.iter().cloned())))
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 6. Action micro-ops
// ---------------------------------------------------------------------------

/// `Action::applicable` (precondition scan) and `Action::apply` (state clone
/// + effect application). These run `O(|library|)` and `O(1)` times per
/// expansion respectively, so even small wins compound.
// Temporary: tighten doc indentation on the continuation line when finalising bench docs.
#[allow(clippy::doc_lazy_continuation)]
fn bench_action_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("ops/action");

    let action = Action::new("act", 1.0)
        .requires("a")
        .requires("b")
        .requires("c")
        .removes("a")
        .adds("d")
        .adds("e");
    let met_state = State::from_facts(["a", "b", "c", "noise_1", "noise_2"]);
    let unmet_state = State::from_facts(["a", "b", "noise_1", "noise_2"]);

    // applicable_* are batched (see `NANO_BATCH`): the precondition scan
    // runs at ~25 ns per call, low enough that single-call noise on
    // shared CI runners exceeds the regression gate's threshold by
    // itself. `apply` clones the State and is two orders of magnitude
    // slower, so it is not batched.
    group.bench_function("applicable_met_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(black_box(&action).applicable(black_box(&met_state)));
            }
        })
    });

    group.bench_function("applicable_unmet_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(black_box(&action).applicable(black_box(&unmet_state)));
            }
        })
    });

    group.bench_function("apply", |b| {
        b.iter(|| black_box(&action).apply(black_box(&met_state)))
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 7. Goal check
// ---------------------------------------------------------------------------

/// `Goal::satisfied_by` runs once per state expanded. With required +
/// forbidden facts it is `O(|required|+|forbidden|)` membership lookups.
fn bench_goal_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("ops/goal");

    let state = State::from_facts((0..50).map(|i| format!("fact_{i}")));

    // All three goal benches are batched (see `NANO_BATCH`): even the
    // compound case at ~140 ns is cheap enough that runner-to-runner
    // jitter shows up as multi-percent drift; batching gets every goal
    // bench above 200 ns where the gate is precise.
    let trivial = Goal::new().requires("fact_0");
    group.bench_function("trivial_one_required_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(black_box(&trivial).satisfied_by(black_box(&state)));
            }
        })
    });

    let mut compound = Goal::new();
    for i in 0..10 {
        compound = compound.requires(format!("fact_{i}"));
    }
    for i in 50..60 {
        compound = compound.forbids(format!("fact_{i}"));
    }
    group.bench_function("compound_10_req_10_forbid_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(black_box(&compound).satisfied_by(black_box(&state)));
            }
        })
    });

    let unmet = Goal::new().requires("fact_999");
    group.bench_function("unmet_required_x32", |b| {
        b.iter(|| {
            for _ in 0..NANO_BATCH {
                black_box(black_box(&unmet).satisfied_by(black_box(&state)));
            }
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 8. Concurrent plans — Arc<Planner> scalability
// ---------------------------------------------------------------------------

/// Run N independent plan calls sequentially vs. in parallel via Rayon over
/// a shared `Arc<Planner>`. Validates that `Planner` is `Send + Sync` in
/// practice and shows wall-clock scaling.
fn bench_concurrent_plans(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_plans");

    // Pre-warm the global Rayon thread pool. Without this, the first
    // parallel bench in this group pays for thread-pool spin-up inside
    // its timed window, and per-runner startup variance shows up as
    // false positives on the regression gate. See issue #24.
    (0..256u32)
        .into_par_iter()
        .for_each(|_| std::hint::spin_loop());

    // Use a non-trivial chain plan as the per-call workload.
    let (actions, initial, goal) = chain_plan(20);
    let planner = Arc::new(Planner::new(actions));

    let inputs: Vec<(State, Goal)> = (0..64).map(|_| (initial.clone(), goal.clone())).collect();

    group.bench_function("sequential_64", |b| {
        b.iter(|| {
            inputs.iter().for_each(|(s, g)| {
                planner.plan(black_box(s), black_box(g)).unwrap().unwrap();
            });
        })
    });

    group.bench_function("parallel_64_rayon", |b| {
        b.iter(|| {
            inputs.par_iter().for_each(|(s, g)| {
                planner.plan(black_box(s), black_box(g)).unwrap().unwrap();
            });
        })
    });

    for &n_calls in &ci_sample_sizes(&[8usize, 32, 128]) {
        let local: Vec<(State, Goal)> = (0..n_calls)
            .map(|_| (initial.clone(), goal.clone()))
            .collect();
        group.bench_with_input(
            BenchmarkId::new("parallel_rayon", n_calls),
            &n_calls,
            |b, _| {
                b.iter(|| {
                    local.par_iter().for_each(|(s, g)| {
                        planner.plan(black_box(s), black_box(g)).unwrap().unwrap();
                    });
                })
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion entry point
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_planning_chain,
    bench_planning_wide,
    bench_planning_redundant,
    bench_planning_boundaries,
    bench_state_ops,
    bench_action_ops,
    bench_goal_check,
    bench_concurrent_plans,
);
criterion_main!(benches);
