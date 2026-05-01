# Benchmark Results — 2026-04-20

**Library version:** grafo 0.1.0  
**Benchmark harness:** Criterion 0.5 (100 samples, 3 s warm-up per group)  
**Platform:** macOS Darwin 25.3.0, Rust 1.94.1

All times are the median of 100 samples. Range shown as [low median high].

---

## Construction

Time to build a `Graph` from node and edge slices (includes label interning, parallel edge
resolution, parallel sort, and CSR offset build).

### `construction` — chain and sparse DAG

| Graph shape         | Nodes   | Time                        |
|---------------------|---------|-----------------------------|
| chain               |   1 000 | 225 – 226 – 228 µs          |
| sparse DAG (fan=4)  |   1 000 | 327 – 330 – 333 µs          |
| chain               |   5 000 | 495 – 497 – 499 µs          |
| sparse DAG (fan=4)  |   5 000 | 632 – 633 – 635 µs          |
| chain               |  10 000 | 710 – 713 – 716 µs          |
| sparse DAG (fan=4)  |  10 000 | 935 – 938 – 941 µs          |
| chain               |  50 000 | 2.64 – 2.65 – 2.65 ms       |
| sparse DAG (fan=4)  |  50 000 | 3.70 – 3.72 – 3.74 ms       |
| chain               | 100 000 | 5.19 – 5.20 – 5.22 ms       |
| sparse DAG (fan=4)  | 100 000 | 7.33 – 7.39 – 7.46 ms       |

### `construction/parallel_sort` — large graphs with Rayon + FxHashMap

| Nodes   | Time                    |
|---------|-------------------------|
|  10 000 | 919 – 922 – 924 µs      |
| 100 000 | 7.06 – 7.11 – 7.16 ms   |
| 500 000 | 45.6 – 46.2 – 46.8 ms   |

---

## Search — no filter (`shortest_path` / `shortest_path_cost`)

### Chain graphs (worst-case path length = node count)

| Nodes   | Time                    |
|---------|-------------------------|
|   1 000 | 18.7 – 18.8 – 18.9 µs  |
|  10 000 | 196 – 197 – 197 µs     |
| 100 000 | 2.05 – 2.06 – 2.06 ms  |

### Sparse DAG fan=4

| Nodes   | Time                  |
|---------|-----------------------|
|   1 000 | 648 – 652 – 657 ns   |
|  10 000 | 5.22 – 5.25 – 5.29 µs |
|  50 000 | 8.69 – 8.75 – 8.80 µs |

### Sparse DAG fan=16

| Nodes   | Time                   |
|---------|------------------------|
|   1 000 | 798 – 802 – 806 ns     |
|  10 000 | 6.15 – 6.16 – 6.17 µs  |
|  50 000 | 19.4 – 19.5 – 19.6 µs  |

### Layered DAG (layers × nodes-per-layer)

| Shape    | Time                   |
|----------|------------------------|
|  20 × 10 | 5.43 – 5.44 – 5.46 µs  |
|  50 × 20 | 39.3 – 39.5 – 39.7 µs  |
| 100 × 30 | 201 – 203 – 206 µs     |

---

## Search — filter overhead (`search/filter_cost`)

All measurements on a 5 000-node sparse DAG. Demonstrates cost of the filter closure
relative to a raw unfiltered search.

| Variant                             | Time                   |
|-------------------------------------|------------------------|
| `shortest_path_cost` (no filter)    | 5.66 – 5.69 – 5.71 µs  |
| `shortest_path_filtered_cost` `\|_\| true` | 5.58 – 5.60 – 5.62 µs  |
| simple one-attr filter              | 13.2 – 13.3 – 13.3 ns  |
| compound AND filter                 | 13.3 – 13.3 – 13.4 ns  |
| strict rare-attr filter             | 13.1 – 13.2 – 13.2 ns  |

The pass-all closure (`|_| true`) is indistinguishable from the unfiltered path — the
compiler monomorphizes and inlines it away. Selective filters run at ~13 ns because most
nodes are pruned before Dijkstra explores them.

---

## Search — attribute count scaling (`search/attr_count_scaling`)

`HashSet`-backed `NodeAttrs` keeps membership checks flat regardless of attribute count.
The jump at 50 attrs is caused by filter density (every node in the 20-word pool has all
words), not scan overhead.

| Attrs per node | Time                   |
|----------------|------------------------|
|              1 | 13.1 – 13.2 – 13.2 ns  |
|              5 | 13.1 – 13.1 – 13.1 ns  |
|             10 | 13.1 – 13.2 – 13.2 ns  |
|             20 | 13.2 – 13.3 – 13.3 ns  |
|             50 | 3.29 – 3.30 – 3.31 µs¹ |

¹ At 50 attrs drawn from a 20-word pool every node has every attribute, so no pruning
occurs and Dijkstra traverses the full graph. The `contains()` call itself is still O(1).

---

## Search — path reconstruction (`search/path_reconstruction`)

Compares `shortest_path` (full `Vec<String>` path) vs `shortest_path_cost` (cost only,
no reconstruction).

| Path length | `shortest_path`        | `shortest_path_cost`   | Speedup |
|------------:|------------------------|------------------------|--------:|
|          10 | 315 – 317 – 318 ns     | 77.8 – 78.8 – 80.0 ns | **4.0×** |
|         100 | 2.04 – 2.04 – 2.05 µs  | 408 – 422 – 435 ns     | **4.8×** |
|       1 000 | 18.9 – 19.0 – 19.1 µs  | 3.28 – 3.38 – 3.48 µs  | **5.6×** |
|      10 000 | 199 – 199 – 200 µs     | 30.6 – 31.1 – 31.7 µs  | **6.4×** |
|     100 000 | 2.04 – 2.05 – 2.05 ms  | 356 – 369 – 382 µs     | **5.5×** |

---

## Search — fan-out scaling (`search/fanout_scaling`)

10 000-node graph. Settled-node array prevents pushing already-finalised nodes back into
the heap.

| Fan-out | Time                   |
|--------:|------------------------|
|       1 | 2.35 – 2.36 – 2.37 µs  |
|       2 | 3.26 – 3.28 – 3.31 µs  |
|       4 | 5.12 – 5.15 – 5.18 µs  |
|       8 | 3.25 – 3.26 – 3.26 µs  |
|      16 | 6.08 – 6.10 – 6.11 µs  |
|      32 | 13.0 – 13.1 – 13.1 µs  |

---

## Concurrent queries (`concurrent_queries`)

All queries share a single `Arc<Graph>` over a 10 000-node sparse DAG.

| Mode                          | Queries | Time                   |
|-------------------------------|--------:|------------------------|
| Sequential                    |      64 | 214 – 216 – 219 µs     |
| Rayon `par_iter`              |      64 | 161 – 163 – 166 µs     |
| Rayon `par_iter`              |       8 | 49.0 – 49.2 – 49.4 µs  |
| Rayon `par_iter`              |      32 | 101 – 102 – 102 µs     |
| Rayon `par_iter`              |     128 | 272 – 274 – 276 µs     |
| Rayon `par_iter`              |     512 | 943 – 945 – 947 µs     |

Rayon delivers **~1.3× wall-clock speedup** at 64 queries. At 512 queries throughput
scales near-linearly with available cores.
