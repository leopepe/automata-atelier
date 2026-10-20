use grafo::Graph;

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

/// Attributed sparse DAG: like `sparse_dag` but each node carries
/// `attrs_per_node` string attributes chosen from a fixed pool.
///
/// Stresses: filter predicate evaluation cost as attribute count grows.
pub fn attributed_dag(n: usize, fan_out: usize, attrs_per_node: usize) -> Graph {
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

/// Actual scaling input retained outside timing so tests can inspect topology.
pub struct AttributeScalingFixture {
    pub graph: Graph,
    pub edges: Vec<(usize, usize, f64)>,
}

/// Exact distinct tags with a guaranteed taxi chain. Shortcuts cost more than
/// the chain segment they bypass, so the unique optimum is 0..n at cost n-1.
/// Attribute generation does not consume the topology RNG.
pub fn attribute_scaling_dag(
    n: usize,
    fan_out: usize,
    attrs_per_node: usize,
) -> AttributeScalingFixture {
    assert!(n >= 2 && attrs_per_node >= 1);
    let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let mut attrs = vec!["taxi".to_string()];
    attrs.extend((1..attrs_per_node).map(|i| format!("tag-{i}")));
    let attrs_ref: Vec<&str> = attrs.iter().map(String::as_str).collect();
    let nodes: Vec<(&str, &[&str])> = labels
        .iter()
        .map(|label| (label.as_str(), attrs_ref.as_slice()))
        .collect();

    let mut edges: Vec<_> = (0..n - 1).map(|i| (i, i + 1, 1.0)).collect();
    let mut seen: std::collections::HashSet<_> = edges.iter().map(|&(a, b, _)| (a, b)).collect();
    let mut rng = 0xbeef_cafe_u64;
    for i in 0..n - 1 {
        let remaining = n - i - 1;
        for _ in 0..fan_out.min(remaining) {
            let j = i + 1 + (lcg(&mut rng) as usize % remaining);
            if seen.insert((i, j)) {
                edges.push((i, j, (j - i + 1) as f64));
            }
        }
    }
    let edge_refs: Vec<_> = edges
        .iter()
        .map(|&(a, b, w)| (labels[a].as_str(), labels[b].as_str(), w))
        .collect();
    let graph = Graph::new_with_attrs(&nodes, &edge_refs).unwrap();
    AttributeScalingFixture { graph, edges }
}

/// Validate the actual benchmark fixture before entering any timed closure.
pub fn validate_attribute_scaling(fixture: &AttributeScalingFixture, n: usize, count: usize) {
    assert!(fixture.edges.len() >= n - 1);
    for i in 0..n {
        let node = i.to_string();
        let result = fixture
            .graph
            .shortest_path_filtered(&node, &node, |attrs| {
                assert_eq!(attrs.len(), count);
                attrs.contains("taxi")
            })
            .unwrap()
            .unwrap();
        assert_eq!(result.cost, 0.0);
    }
    let route = fixture
        .graph
        .shortest_path_filtered("0", &(n - 1).to_string(), |attrs| attrs.contains("taxi"))
        .unwrap()
        .unwrap();
    assert_eq!(route.cost, (n - 1) as f64);
    assert_eq!(route.indices, (0..n as u32).collect::<Vec<_>>());
}
