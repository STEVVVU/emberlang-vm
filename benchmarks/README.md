# Benchmarks

There are two harnesses, and both are kept on purpose because they answer
different questions. The short version: **the bin produces the committed
artifact, criterion answers "did my change make it slower?"** Neither subsumes
the other, so deleting either would lose something.

| | `benchmark` bin | `cargo bench` (criterion) |
| --- | --- | --- |
| Question | what did this build do, on the record | is this change faster than the last |
| Output | one JSON doc, committed as `results-*.json` | statistical reports under `target/`, not committed |
| Scope | whole pipeline + optimizer counters + VM | per stage, and per stage only |
| Deps | none beyond the crate itself | criterion (a dev-dependency) |
| Run in CI | yes, as a smoke test | compiled and `--test`-run, never timed |

## 1. `benchmark` bin — committed evidence

```sh
cargo run --release --bin benchmark                          # print to stdout
cargo run --release --bin benchmark > results-windows-x86_64.json
```

The harness writes JSON to stdout, so regenerating the artifact means redirecting
it. **On Windows, redirect from `cmd` or git-bash, not PowerShell 5.1**: its `>`
writes UTF-16LE, and `Out-File`/`Tee-Object` add a BOM. Either produces an
artifact that no longer parses as UTF-8 JSON while looking perfectly correct in an
editor. From PowerShell 7+, use
`| Out-File -Encoding utf8NoBOM results-windows-x86_64.json`.
`scripts/check_fixture_bytes.py` fails on both encodings, so a slip here is caught
rather than committed.

Generates a deterministic 8,441-byte program containing 80 helper functions and
one loop-heavy `main`, then measures:

1. full source-to-bytecode compilation over 200 independent iterations;
2. optimization counters from that compilation; and
3. VM dispatch throughput over 50 fresh executions.

It prints one JSON document and needs no dependencies, so it is what CI
smoke-runs and what `results-*.json` is generated from. It reports **min**,
median, and p95 compile latency. The minimum matters on a shared machine: noise
can only ever make a sample slower, so the fastest observed compile is the
closest available estimate of the work itself, and it stays far more stable
across invocations than the median does — measured below at 1.3x spread versus
12.2x for p95.

It also emits the three figures that are *not* timings — constants folded, dead
instructions removed, resulting bytecode size. Those are the numbers worth
committing, because they are properties of the compiler rather than of the clock,
and criterion does not report them at all. That alone is why this bin was not
replaced when criterion was added.

`schema` is the artifact's version field; bump it when the field set changes so a
consumer cannot misread new output as old.

## 2. `cargo bench` — criterion, for comparison

```sh
cargo bench                      # both suites
cargo bench --bench compiler     # per-stage: lex/parse/resolve/typecheck/...
cargo bench --bench vm           # whole-pipeline compile + VM throughput
```

`benches/compiler.rs` times each stage on the previous stage's output, so a
number is attributable to one phase rather than to the whole pipeline.
`benches/vm.rs` times whole compiles (optimized vs unoptimized) and VM dispatch,
reporting the interpreter's instructions/second directly via criterion's
throughput support.

Criterion's value here is **relative**, via its baseline support:

```sh
cargo bench -- --save-baseline before
# ... make a change ...
cargo bench -- --baseline before
```

That comparison is worth having even on a noisy machine, because both sides are
measured under the same conditions minutes apart. Its reports live under `target/`
and are machine-local; they are deliberately not committed as artifacts.

CI compiles both suites and runs them under `cargo bench -- --test`, which
executes each benchmark once to prove it still works but records no timings.
Timing on a shared runner would produce numbers nobody should act on.

## Honesty about these numbers

Results are single-machine baselines, not cross-machine claims. Turbo state,
background load, compiler version, and OS all move them.

The measurement box is a **power-managed laptop with unpinned clocks**, and the
recorded run was taken while ordinary interactive applications were running —
sampled CPU utilization was ~100%. So these are reported as **envelopes, not
point values.** Across 32 consecutive invocations of the same release binary
(pooled from batches of 12 and 20):

| Statistic | min | median | max | spread |
| --- | ---: | ---: | ---: | ---: |
| Compile min (µs) | 771 | 884 | 984 | 1.3x |
| Compile median (µs) | 1,056 | 1,258 | 1,473 | 1.4x |
| Compile p95 (µs) | 1,656 | 2,524 | 20,267 | 12.2x |
| VM throughput (M instr/s) | 19.8 | 128.8 | 153.0 | 7.7x |

Two things worth being explicit about:

- **The two batches disagreed with each other by more than 5x on the lowest VM
  sample.** That is why 32 runs are pooled and no single batch is quoted: had only
  the second batch been reported, the VM floor would have looked like 104 M
  instr/s instead of the 19.8 M actually observed.
- **The run recorded in `results-windows-x86_64.json` is the one closest to the
  pooled median, not the fastest.** Picking the best of 32 would have been a ~1.2x
  free improvement on compile and ~1.06x on VM throughput.

Republishing on a quiesced machine would tighten all of this considerably; until
then, treat the envelope width as part of the result. The same noise makes
criterion's per-stage confidence intervals unreliable here, so **no per-stage
figure is quoted as a baseline anywhere in this repo.**

What the noise does *not* affect, and the reason any of this is still useful:
the optimizer counters (402 constants folded, 722 dead instructions removed), the
resulting bytecode size (336 instructions), and the retired instruction count
(5,000,400) were **byte-identical in all 32 runs.** Those are compiler properties,
not clock properties, so they are the figures to hold the compiler to.

The harness uses `std::hint::black_box` and excludes release build time.
Regenerate `results-*.json` rather than editing measurements to fit a target.
