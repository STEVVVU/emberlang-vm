//! Whole-pipeline compile latency and VM dispatch throughput.
//!
//! The two numbers a compiler project is judged on: how long a compile takes,
//! and how many instructions per second the interpreter retires. Run with
//! `cargo bench --bench vm`.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use emberlang_vm::vm::{execute, VmLimits};
use emberlang_vm::{compile, CompileOptions};
use std::hint::black_box;

fn benchmark_source(helper_count: usize) -> String {
    use std::fmt::Write;
    let mut source = String::new();
    for index in 0..helper_count {
        let _ = writeln!(
            source,
            "fn helper_{index}(x: int) -> int {{ let folded: int = 2 + 3 * 4; \
             let dead: int = 99 * 2; return x + folded; }}"
        );
    }
    source.push_str(
        "fn main() -> int { let total: int = 0; let i: int = 0; \
         while i < 10000 { total = total + i; i = i + 1; } \
         print(total); return 0; }\n",
    );
    source
}

fn compile_pipeline(criterion: &mut Criterion) {
    let source = benchmark_source(80);
    let mut group = criterion.benchmark_group("compile");
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("optimized", |bencher| {
        bencher.iter(|| compile(black_box(&source), CompileOptions::default()).unwrap());
    });
    // Paired with the above, this isolates what the optimization passes cost on
    // a full compile rather than in isolation.
    group.bench_function("unoptimized", |bencher| {
        bencher.iter(|| compile(black_box(&source), CompileOptions { optimize: false }).unwrap());
    });
    group.finish();
}

fn vm_dispatch(criterion: &mut Criterion) {
    let source = benchmark_source(80);
    let compilation = compile(&source, CompileOptions::default()).expect("benchmark must compile");
    // The benchmark program prints once and loops 10,000 times. Raise the step
    // ceiling so the measured run is not cut short, but keep every other bound
    // (call depth, frame registers, function count, output bytes) at its
    // default: this measures the sandboxed interpreter, not an unbounded one.
    let limits = VmLimits {
        max_steps: 10_000_000,
        ..VmLimits::default()
    };
    let steps = execute(&compilation.bytecode, limits)
        .expect("benchmark must execute")
        .steps;

    let mut group = criterion.benchmark_group("vm");
    // Elements = retired instructions, so criterion reports the interpreter's
    // instructions/second directly.
    group.throughput(Throughput::Elements(steps));
    group.bench_function("dispatch", |bencher| {
        bencher.iter(|| execute(black_box(&compilation.bytecode), limits).unwrap());
    });
    group.finish();
}

criterion_group!(benches, compile_pipeline, vm_dispatch);
criterion_main!(benches);
