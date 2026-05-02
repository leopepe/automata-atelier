/// Grafo performance benchmarks.
///
/// Run with:
///   cargo bench
///   cargo bench -- <filter>          # run a subset, e.g. "search"
///   cargo bench -- --save-baseline main   # save a named baseline
///   cargo bench -- --baseline main        # compare against saved baseline
///
/// HTML reports are written to target/criterion/.
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use grafo::{Graph, NodeAttrs};
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
// Graph factories
// ---------------------------------------------------------------------------

/// Linear chain: 0 → 1 → 2 → … → n-1, weight 1.0 each.
///
/// Stresses: path reconstruction (path length == n), minimal heap churn
/// (only one neighbour per node), no filter overhead.
fn chain(n: usize) -> Graph {
    let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let nodes: Vec<&str> = labels.iter().map(String::as_str).collect();
    let raw_edges: Vec<(usize, usize, f64)> = (0..n - 1).map(|i| (i, i + 1, 1.0)).collect();
    let edges: Vec<(&str, &str, f64)> = raw_edges
        .iter()
        .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
        .collect();
    Graph::new(&nodes, &edges).unwrap()
}

/// Sparse DAG: each node connects to `fan_out` forward neighbours chosen
/// deterministically. Weights are random in [1, 100].
///
/// Stresses: realistic search frontier expansion, heap pressure proportional
/// to fan-out, Dijkstra correctness under competing paths.
fn sparse_dag(n: usize, fan_out: usize) -> Graph {
    let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let nodes: Vec<&str> = labels.iter().map(String::as_str).collect();

    let mut rng: u64 = 0xdead_beef_cafe_u64;
    let mut seen = std::collections::HashSet::new();
    let mut raw_edges: Vec<(usize, usize, f64)> = Vec::new();

    for i in 0..n {
        let remaining = n - i - 1;
        if remaining == 0 {
            break;
        }
        for _ in 0..fan_out.min(remaining) {
            // Pick a forward neighbour in [i+1, n-1].
            let j = i + 1 + (lcg(&mut rng) as usize % remaining);
            if seen.insert((i, j)) {
                let w = (lcg(&mut rng) % 100 + 1) as f64;
                raw_edges.push((i, j, w));
            }
        }
    }

    let edges: Vec<(&str, &str, f64)> = raw_edges
        .iter()
        .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
        .collect();
    Graph::new(&nodes, &edges).unwrap()
}

/// Layered (wide) DAG: `layers` layers of `width` nodes each.
/// Every node connects to every node in the next layer.
///
/// Edge count = layers × width². Stresses: heap pollution — many nodes share
/// equal or near-equal distances, so the heap grows very large.
fn layered_dag(layers: usize, width: usize) -> (Graph, String, String) {
    let n = layers * width;
    let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let nodes: Vec<&str> = labels.iter().map(String::as_str).collect();

    let mut rng: u64 = 0x1234_5678_u64;
    let mut raw_edges: Vec<(usize, usize, f64)> = Vec::new();

    for layer in 0..layers - 1 {
        for src in 0..width {
            for dst in 0..width {
                let i = layer * width + src;
                let j = (layer + 1) * width + dst;
                let w = (lcg(&mut rng) % 10 + 1) as f64;
                raw_edges.push((i, j, w));
            }
        }
    }

    let edges: Vec<(&str, &str, f64)> = raw_edges
        .iter()
        .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
        .collect();

    let src_label = labels[0].clone();
    let dst_label = labels[n - 1].clone();
    (Graph::new(&nodes, &edges).unwrap(), src_label, dst_label)
}

/// Attributed sparse DAG: like `sparse_dag` but each node carries
/// `attrs_per_node` string attributes chosen from a fixed pool.
///
/// Stresses: filter predicate evaluation cost as attribute count grows.
fn attributed_dag(n: usize, fan_out: usize, attrs_per_node: usize) -> Graph {
    // Fixed pool of realistic attribute names.
    const POOL: &[&str] = &[
        "taxi",
        "bus",
        "train",
        "ferry",
        "metro",
        "airport",
        "parking",
        "hotel",
        "hospital",
        "university",
        "mall",
        "park",
        "beach",
        "museum",
        "stadium",
        "port",
        "border",
        "toll",
        "rest_area",
        "fuel",
    ];

    let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let mut rng: u64 = 0xbeef_cafe_u64;

    let nodes_with_attrs: Vec<(&str, Vec<&str>)> = labels
        .iter()
        .map(|label| {
            let attrs: Vec<&str> = (0..attrs_per_node)
                .map(|_| POOL[lcg(&mut rng) as usize % POOL.len()])
                .collect();
            (label.as_str(), attrs)
        })
        .collect();

    // Build nodes slice for new_with_attrs.
    let nodes_ref: Vec<(&str, &[&str])> = nodes_with_attrs
        .iter()
        .map(|(id, attrs)| (*id, attrs.as_slice()))
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut raw_edges: Vec<(usize, usize, f64)> = Vec::new();

    for i in 0..n {
        let remaining = n - i - 1;
        if remaining == 0 {
            break;
        }
        for _ in 0..fan_out.min(remaining) {
            let j = i + 1 + (lcg(&mut rng) as usize % remaining);
            if seen.insert((i, j)) {
                let w = (lcg(&mut rng) % 100 + 1) as f64;
                raw_edges.push((i, j, w));
            }
        }
    }

    let edges: Vec<(&str, &str, f64)> = raw_edges
        .iter()
        .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
        .collect();

    Graph::new_with_attrs(&nodes_ref, &edges).unwrap()
}

// ---------------------------------------------------------------------------
// 1. Construction benchmarks
// ---------------------------------------------------------------------------

/// How long does it take to build graphs of increasing size?
/// Expected complexity: O(E log E) due to the CSR sort.
fn bench_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("construction");

    for &n in &ci_sample_sizes(&[1_000usize, 5_000, 10_000, 50_000, 100_000]) {
        group.bench_with_input(BenchmarkId::new("chain", n), &n, |b, &n| {
            let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
            let nodes: Vec<&str> = labels.iter().map(String::as_str).collect();
            let raw: Vec<(usize, usize, f64)> = (0..n - 1).map(|i| (i, i + 1, 1.0)).collect();
            let edges: Vec<(&str, &str, f64)> = raw
                .iter()
                .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
                .collect();
            b.iter_with_large_drop(|| Graph::new(black_box(&nodes), black_box(&edges)).unwrap())
        });

        group.bench_with_input(BenchmarkId::new("sparse_dag_fan4", n), &n, |b, &n| {
            let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
            let nodes: Vec<&str> = labels.iter().map(String::as_str).collect();
            let mut rng: u64 = 42;
            let mut seen = std::collections::HashSet::new();
            let mut raw: Vec<(usize, usize, f64)> = Vec::new();
            for i in 0..n {
                let rem = n - i - 1;
                if rem == 0 {
                    break;
                }
                for _ in 0..4usize.min(rem) {
                    let j = i + 1 + (lcg(&mut rng) as usize % rem);
                    if seen.insert((i, j)) {
                        raw.push((i, j, (lcg(&mut rng) % 100 + 1) as f64));
                    }
                }
            }
            let edges: Vec<(&str, &str, f64)> = raw
                .iter()
                .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
                .collect();
            b.iter_with_large_drop(|| Graph::new(black_box(&nodes), black_box(&edges)).unwrap())
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Search benchmarks — no filter (baseline Dijkstra)
// ---------------------------------------------------------------------------

/// Shortest-path search across graph sizes.
/// Reveals how search time scales with V and E.
fn bench_search_no_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("search/no_filter");

    // Chain: O(V) edges, path visits every node — stresses path reconstruction.
    for &n in &ci_sample_sizes(&[1_000usize, 10_000, 100_000]) {
        let g = chain(n);
        let src = "0";
        let dst = (n - 1).to_string();
        group.bench_with_input(BenchmarkId::new("chain", n), &n, |b, _| {
            b.iter(|| g.shortest_path(black_box(src), black_box(&dst)).unwrap())
        });
    }

    // Sparse DAG fan-out 4: realistic workload.
    for &n in &ci_sample_sizes(&[1_000usize, 10_000, 50_000]) {
        let g = sparse_dag(n, 4);
        let dst = (n - 1).to_string();
        group.bench_with_input(BenchmarkId::new("sparse_dag_fan4", n), &n, |b, _| {
            b.iter(|| g.shortest_path(black_box("0"), black_box(&dst)).unwrap())
        });
    }

    // Sparse DAG fan-out 16: higher edge density → more heap churn.
    for &n in &ci_sample_sizes(&[1_000usize, 10_000, 50_000]) {
        let g = sparse_dag(n, 16);
        let dst = (n - 1).to_string();
        group.bench_with_input(BenchmarkId::new("sparse_dag_fan16", n), &n, |b, _| {
            b.iter(|| g.shortest_path(black_box("0"), black_box(&dst)).unwrap())
        });
    }

    // Layered DAG: maximum heap pressure (E = layers × width²).
    for &(layers, width) in &ci_sample_sizes(&[(20usize, 10usize), (50, 20), (100, 30)]) {
        let (g, src, dst) = layered_dag(layers, width);
        let label = format!("{}x{}", layers, width);
        group.bench_with_input(BenchmarkId::new("layered", &label), &label, |b, _| {
            b.iter(|| g.shortest_path(black_box(&src), black_box(&dst)).unwrap())
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Search benchmarks — filter cost isolation
// ---------------------------------------------------------------------------

/// Compare search with no filter vs. simple filter vs. complex filter on the
/// same graph. Isolates the overhead introduced by predicate evaluation.
fn bench_search_filter_cost(c: &mut Criterion) {
    let mut group = c.benchmark_group("search/filter_cost");

    // Pre-build a single attributed graph used for all sub-benchmarks.
    const N: usize = 10_000;
    const FAN: usize = 4;
    let g = attributed_dag(N, FAN, 5);
    let dst = (N - 1).to_string();

    // Baseline: no filter at all.
    group.bench_function("no_filter", |b| {
        b.iter(|| g.shortest_path(black_box("0"), black_box(&dst)).unwrap())
    });

    // Pass-all filter: same result as no filter but with closure overhead.
    group.bench_function("pass_all_closure", |b| {
        b.iter(|| {
            g.shortest_path_filtered(black_box("0"), black_box(&dst), |_| true)
                .unwrap()
        })
    });

    // Simple filter: single string scan (realistic taxi/bus use case).
    group.bench_function("simple_one_attr", |b| {
        b.iter(|| {
            g.shortest_path_filtered(black_box("0"), black_box(&dst), |attrs: &NodeAttrs| {
                attrs.contains("taxi")
            })
            .unwrap()
        })
    });

    // Compound filter: AND of two conditions (two scans per node).
    group.bench_function("compound_and", |b| {
        b.iter(|| {
            g.shortest_path_filtered(black_box("0"), black_box(&dst), |attrs: &NodeAttrs| {
                attrs.contains("taxi") && attrs.contains("bus")
            })
            .unwrap()
        })
    });

    // Strict filter: attribute that almost no node has → most nodes pruned,
    // so Dijkstra explores very few nodes but has to check many candidates.
    group.bench_function("strict_rare_attr", |b| {
        b.iter(|| {
            g.shortest_path_filtered(black_box("0"), black_box(&dst), |attrs: &NodeAttrs| {
                attrs.contains("ferry")
            })
            .unwrap()
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Attribute count scaling
// ---------------------------------------------------------------------------

/// How does filter evaluation cost scale with attribute count?
///
/// With the old `Vec<String>` storage, cost was O(n) per node visit and grew
/// linearly. With `HashSet<String>` + `NodeAttrs::contains`, it is O(1) and
/// should be flat across all attribute counts.
fn bench_attr_count_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("search/attr_count_scaling");

    const N: usize = 5_000;
    const FAN: usize = 4;

    for &attrs in &ci_sample_sizes(&[1usize, 5, 10, 20, 50]) {
        let g = attributed_dag(N, FAN, attrs);
        let dst = (N - 1).to_string();
        group.bench_with_input(BenchmarkId::new("attrs_per_node", attrs), &attrs, |b, _| {
            b.iter(|| {
                // O(1) lookup regardless of how many attributes each node has.
                g.shortest_path_filtered(black_box("0"), black_box(&dst), |a: &NodeAttrs| {
                    a.contains("taxi")
                })
                .unwrap()
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 5. Path length (reconstruction cost) — full path vs. cost-only
// ---------------------------------------------------------------------------

/// Directly compares `shortest_path` (with reconstruction) against
/// `shortest_path_cost` (without reconstruction) on chains of increasing
/// length. The delta isolates the pure String-clone overhead.
fn bench_path_reconstruction(c: &mut Criterion) {
    let mut group = c.benchmark_group("search/path_reconstruction");

    for &n in &ci_sample_sizes(&[10usize, 100, 1_000, 10_000, 100_000]) {
        let g = chain(n);
        let dst = (n - 1).to_string();

        // Full path: Dijkstra + reconstruction (String::clone per hop).
        group.bench_with_input(BenchmarkId::new("full_path", n), &n, |b, _| {
            b.iter(|| g.shortest_path(black_box("0"), black_box(&dst)).unwrap())
        });

        // Cost only: Dijkstra only, no predecessor array, no reconstruction.
        group.bench_with_input(BenchmarkId::new("cost_only", n), &n, |b, _| {
            b.iter(|| {
                g.shortest_path_cost(black_box("0"), black_box(&dst))
                    .unwrap()
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 6. Fan-out scaling (heap pressure)
// ---------------------------------------------------------------------------

/// Fix graph size and increase fan-out to observe heap churn vs. edge count.
/// At high fan-out, lazy deletion means many stale heap entries accumulate.
fn bench_fanout_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("search/fanout_scaling");

    const N: usize = 10_000;

    for &fan in &ci_sample_sizes(&[1usize, 2, 4, 8, 16, 32]) {
        let g = sparse_dag(N, fan);
        let dst = (N - 1).to_string();
        group.bench_with_input(BenchmarkId::new("fan_out", fan), &fan, |b, _| {
            b.iter(|| g.shortest_path(black_box("0"), black_box(&dst)).unwrap())
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 7. Construction — parallel sort scaling
// ---------------------------------------------------------------------------

/// Shows how construction time scales with graph size now that edge resolution
/// and sorting use Rayon. Pair with `--save-baseline` / `--baseline` to diff
/// against a sequential-only build.
fn bench_construction_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("construction/parallel_sort");

    // Large sparse DAGs where the sort step dominates.
    for &n in &ci_sample_sizes(&[10_000usize, 100_000, 500_000]) {
        // Pre-build labels and raw edge list outside the timed section.
        let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
        let nodes: Vec<&str> = labels.iter().map(String::as_str).collect();

        let mut rng: u64 = 0xdead_beef_u64;
        let mut seen = std::collections::HashSet::new();
        let mut raw: Vec<(usize, usize, f64)> = Vec::new();
        for i in 0..n {
            let rem = n - i - 1;
            if rem == 0 {
                break;
            }
            for _ in 0..4usize.min(rem) {
                let j = i + 1 + (lcg(&mut rng) as usize % rem);
                if seen.insert((i, j)) {
                    raw.push((i, j, (lcg(&mut rng) % 100 + 1) as f64));
                }
            }
        }
        let edges: Vec<(&str, &str, f64)> = raw
            .iter()
            .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
            .collect();

        group.bench_with_input(BenchmarkId::new("sparse_dag_fan4", n), &n, |b, _| {
            b.iter_with_large_drop(|| Graph::new(black_box(&nodes), black_box(&edges)).unwrap())
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 8. Concurrent queries — Arc<Graph> scalability
// ---------------------------------------------------------------------------

/// Measures the cost of running N shortest-path queries sequentially vs. in
/// parallel via Rayon. The graph is shared via Arc so there is zero clone cost.
fn bench_concurrent_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_queries");

    // Build once; queries run against this shared immutable graph.
    const N: usize = 10_000;
    let graph = Arc::new(sparse_dag(N, 4));
    let dst = (N - 1).to_string();

    // Representative set of (from, to) pairs covering different path lengths.
    let query_pairs: Vec<(String, String)> = (0..64)
        .map(|i| {
            let from = (i * (N / 64)).to_string();
            (from, dst.clone())
        })
        .collect();

    // Sequential: run each query one after another on a single thread.
    group.bench_function("sequential_64", |b| {
        b.iter(|| {
            query_pairs.iter().for_each(|(from, to)| {
                graph
                    .shortest_path_cost(black_box(from), black_box(to))
                    .unwrap();
            });
        })
    });

    // Parallel: Rayon distributes queries across all available cores.
    group.bench_function("parallel_64_rayon", |b| {
        b.iter(|| {
            query_pairs.par_iter().for_each(|(from, to)| {
                graph
                    .shortest_path_cost(black_box(from), black_box(to))
                    .unwrap();
            });
        })
    });

    // Scale: how does parallel throughput grow with query count?
    for &n_queries in &ci_sample_sizes(&[8usize, 32, 128, 512]) {
        let pairs: Vec<(String, String)> = (0..n_queries)
            .map(|i| ((i * (N / n_queries)).to_string(), dst.clone()))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("parallel_rayon", n_queries),
            &n_queries,
            |b, _| {
                b.iter(|| {
                    pairs.par_iter().for_each(|(from, to)| {
                        graph
                            .shortest_path_cost(black_box(from), black_box(to))
                            .unwrap();
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
    bench_construction,
    bench_search_no_filter,
    bench_search_filter_cost,
    bench_attr_count_scaling,
    bench_path_reconstruction,
    bench_fanout_scaling,
    bench_construction_parallel,
    bench_concurrent_queries,
);
criterion_main!(benches);
