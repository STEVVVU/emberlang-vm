# Benchmarks

The benchmark binary generates a deterministic 8.4 KB program containing 80
helper functions and one loop-heavy `main`. It measures:

1. full source-to-bytecode compilation over 200 independent iterations;
2. optimization counters from that compilation; and
3. VM dispatch throughput over 50 fresh executions.

Run it with:

```sh
cargo run --release --bin benchmark
```

Results are local baselines, not cross-machine claims. Turbo state, background
load, compiler version, and operating system affect them. The harness uses
`std::hint::black_box`, reports median and p95 compilation latency, and does not
include release build time. Raw observed output is committed under
`benchmarks/results-*.json`; regenerate it rather than editing measurements to
fit a target.

