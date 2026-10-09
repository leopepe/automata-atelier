//! Reproducible cost-query comparisons: cargo bench -p grafo-dag --bench workspace.
//! Save a run: -- --save-baseline workspace-v1; compare later: -- --baseline workspace-v1.
//! Fresh means a new workspace per iteration, not cold CPU/allocator caches.
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use grafo::{Graph, SearchWorkspace};
use rayon::prelude::*;

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

/// Match the investigation's 100k-vertex reachable subgraph and append isolated
/// vertices. The search frontier is fixed while total graph size changes.
fn padded_sparse(total: usize) -> Graph {
    const REACHABLE: usize = 100_000;
    let labels: Vec<String> = (0..total).map(|i| i.to_string()).collect();
    let nodes: Vec<_> = labels.iter().map(String::as_str).collect();
    let mut raw = Vec::with_capacity(REACHABLE * 5);
    let mut rng = 0xdead_beef_cafe;
    for i in 0..REACHABLE - 1 {
        raw.push((i, i + 1, 100.0_f64));
        for _ in 0..4 {
            let target = i + 1 + (lcg(&mut rng) % (REACHABLE - i - 1) as u64) as usize;
            raw.push((i, target, (lcg(&mut rng) % 100 + 1) as f64));
        }
    }
    raw.sort_unstable_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)).then(a.2.total_cmp(&b.2)));
    raw.dedup_by_key(|edge| (edge.0, edge.1));
    let edges: Vec<_> = raw
        .iter()
        .map(|&(a, b, cost)| (nodes[a], nodes[b], cost))
        .collect();
    Graph::new(&nodes, &edges).unwrap()
}

/// Every vertex is visited; generation checks cannot remove frontier work.
fn chain(n: usize) -> Graph {
    let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let nodes: Vec<_> = labels.iter().map(String::as_str).collect();
    let edges: Vec<_> = (0..n - 1).map(|i| (nodes[i], nodes[i + 1], 1.0)).collect();
    Graph::new(&nodes, &edges).unwrap()
}

/// Wide equal-cost layers stress heap ties and a broad frontier.
fn layered() -> Graph {
    let labels: Vec<String> = (0..3_000).map(|i| i.to_string()).collect();
    let nodes: Vec<_> = labels.iter().map(String::as_str).collect();
    let mut edges = Vec::new();
    for layer in 0..99 {
        for src in 0..30 {
            for dst in 0..30 {
                edges.push((nodes[layer * 30 + src], nodes[(layer + 1) * 30 + dst], 1.0));
            }
        }
    }
    Graph::new(&nodes, &edges).unwrap()
}

/// Rejected middle vertices force a longer but permitted route; endpoints pass.
fn filtered() -> Graph {
    let labels: Vec<String> = (0..100_000).map(|i| i.to_string()).collect();
    let nodes: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            (
                label.as_str(),
                if i == 1 {
                    &["blocked"][..]
                } else {
                    &["allowed"][..]
                },
            )
        })
        .collect();
    Graph::new_with_attrs(
        &nodes,
        &[
            ("0", "1", 1.0),
            ("1", "3", 1.0),
            ("0", "2", 2.0),
            ("2", "3", 3.0),
        ],
    )
    .unwrap()
}

/// Pair the unchanged one-shot API with a workspace warmed outside timing.
fn pair(c: &mut Criterion, name: &str, graph: &Graph, from: &str, to: &str) {
    let expected = graph.shortest_path_cost(from, to).unwrap();
    let mut workspace = SearchWorkspace::new();
    assert_eq!(
        graph
            .shortest_path_cost_with_workspace(from, to, &mut workspace)
            .unwrap(),
        expected
    );
    let mut group = c.benchmark_group(format!("workspace/{name}"));
    group.bench_function("one_shot", |b| {
        b.iter(|| {
            graph
                .shortest_path_cost(black_box(from), black_box(to))
                .unwrap()
        })
    });
    group.bench_function("warm", |b| {
        b.iter(|| {
            graph
                .shortest_path_cost_with_workspace(
                    black_box(from),
                    black_box(to),
                    black_box(&mut workspace),
                )
                .unwrap()
        })
    });
    group.finish();
}

fn bench_padding(c: &mut Criterion) {
    for total in [100_000, 250_000, 1_000_000] {
        let graph = padded_sparse(total);
        pair(c, &format!("padding/{total}"), &graph, "0", "99999");
        let mut group = c.benchmark_group("workspace/cold");
        group.bench_with_input(BenchmarkId::new("padding", total), &total, |b, _| {
            b.iter(|| {
                let mut workspace = SearchWorkspace::new();
                graph
                    .shortest_path_cost_with_workspace(
                        black_box("0"),
                        black_box("99999"),
                        &mut workspace,
                    )
                    .unwrap()
            })
        });
        group.finish();
    }
}

fn bench_shapes(c: &mut Criterion) {
    pair(c, "chain", &chain(100_000), "0", "99999");
    pair(c, "layered", &layered(), "0", "2999");
    pair(c, "unreachable", &padded_sparse(100_000), "99999", "0");
}

fn bench_batches(c: &mut Criterion) {
    let graph = padded_sparse(100_000);
    let queries = [
        ("0", "99999"),
        ("0", "50000"),
        ("1", "99999"),
        ("100", "99999"),
        ("0", "1"),
        ("99998", "99999"),
        ("99999", "0"),
        ("0", "0"),
    ];
    let mut workspace = SearchWorkspace::new();
    for &(from, to) in &queries {
        assert_eq!(
            graph
                .shortest_path_cost_with_workspace(from, to, &mut workspace)
                .unwrap(),
            graph.shortest_path_cost(from, to).unwrap()
        );
    }
    let mut group = c.benchmark_group("workspace/batches");
    group.bench_function("mixed_one_shot_x8", |b| {
        b.iter(|| {
            for &(from, to) in &queries {
                black_box(
                    graph
                        .shortest_path_cost(black_box(from), black_box(to))
                        .unwrap(),
                );
            }
        })
    });
    group.bench_function("mixed_warm_x8", |b| {
        b.iter(|| {
            for &(from, to) in &queries {
                black_box(
                    graph
                        .shortest_path_cost_with_workspace(
                            black_box(from),
                            black_box(to),
                            &mut workspace,
                        )
                        .unwrap(),
                );
            }
        })
    });
    group.bench_function("amortized_one_shot_x4", |b| {
        b.iter(|| {
            for _ in 0..4 {
                black_box(
                    graph
                        .shortest_path_cost(black_box("0"), black_box("99999"))
                        .unwrap(),
                );
            }
        })
    });
    group.bench_function("amortized_fresh_workspace_x4", |b| {
        b.iter(|| {
            let mut workspace = SearchWorkspace::new();
            for _ in 0..4 {
                black_box(
                    graph
                        .shortest_path_cost_with_workspace(
                            black_box("0"),
                            black_box("99999"),
                            &mut workspace,
                        )
                        .unwrap(),
                );
            }
        })
    });
    group.finish();
}

fn bench_filter(c: &mut Criterion) {
    let graph = filtered();
    let mut workspace = SearchWorkspace::new();
    assert_eq!(
        graph
            .shortest_path_filtered_cost_with_workspace(
                "0",
                "3",
                |attrs| attrs.contains("allowed"),
                &mut workspace
            )
            .unwrap(),
        Some(5.0)
    );
    assert_eq!(
        graph
            .shortest_path_filtered_cost("0", "3", |attrs| attrs.contains("allowed"))
            .unwrap(),
        Some(5.0)
    );
    let mut group = c.benchmark_group("workspace/filtered");
    group.bench_function("one_shot", |b| {
        b.iter(|| {
            graph
                .shortest_path_filtered_cost(black_box("0"), black_box("3"), |attrs| {
                    attrs.contains("allowed")
                })
                .unwrap()
        })
    });
    group.bench_function("warm", |b| {
        b.iter(|| {
            graph
                .shortest_path_filtered_cost_with_workspace(
                    black_box("0"),
                    black_box("3"),
                    |attrs| attrs.contains("allowed"),
                    &mut workspace,
                )
                .unwrap()
        })
    });
    group.finish();
}

/// Scheduling is included equally; each of four tasks owns its retained workspace.
fn bench_concurrent(c: &mut Criterion) {
    let graph = padded_sparse(100_000);
    let mut workspaces: Vec<_> = (0..4).map(|_| SearchWorkspace::new()).collect();
    let expected = graph.shortest_path_cost("0", "99999").unwrap();
    for workspace in &mut workspaces {
        assert_eq!(
            graph
                .shortest_path_cost_with_workspace("0", "99999", workspace)
                .unwrap(),
            expected
        );
    }
    let mut group = c.benchmark_group("workspace/concurrent");
    group.bench_function("one_shot_4_workers_x16", |b| {
        b.iter(|| {
            (0..4).into_par_iter().for_each(|_| {
                for _ in 0..16 {
                    black_box(
                        graph
                            .shortest_path_cost(black_box("0"), black_box("99999"))
                            .unwrap(),
                    );
                }
            });
        })
    });
    group.bench_function("warm_4_workers_x16", |b| {
        b.iter(|| {
            workspaces.par_iter_mut().for_each(|workspace| {
                for _ in 0..16 {
                    black_box(
                        graph
                            .shortest_path_cost_with_workspace(
                                black_box("0"),
                                black_box("99999"),
                                workspace,
                            )
                            .unwrap(),
                    );
                }
            });
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_padding,
    bench_shapes,
    bench_batches,
    bench_filter,
    bench_concurrent
);
criterion_main!(benches);
