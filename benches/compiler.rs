//! Per-stage compiler microbenchmarks.
//!
//! Each stage is measured on the *output of the previous stage*, so the numbers
//! attribute time to one phase rather than to the whole pipeline. The whole-file
//! and VM figures live in `benches/vm.rs`.
//!
//! Run with `cargo bench --bench compiler`. The end-to-end harness that emits
//! the committed JSON baseline is still `cargo run --release --bin benchmark`.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use emberlang_vm::{bytecode, ir, lexer, optimize, parser, resolve, typecheck};
use std::hint::black_box;

/// Generates the same deterministic input shape as `src/bin/benchmark.rs`, so
/// the micro and end-to-end numbers describe the same program.
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

fn frontend(criterion: &mut Criterion) {
    let source = benchmark_source(80);
    let tokens = lexer::lex(&source).tokens;
    let program = parser::parse(&tokens).program;

    let mut group = criterion.benchmark_group("frontend");
    // Byte throughput is the comparable figure for lexing; the later stages are
    // sized by AST nodes, not bytes, so they are reported as plain latency.
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("lex", |bencher| {
        bencher.iter(|| lexer::lex(black_box(&source)));
    });
    group.finish();

    let mut group = criterion.benchmark_group("analysis");
    group.bench_function("parse", |bencher| {
        bencher.iter(|| parser::parse(black_box(&tokens)));
    });
    group.bench_function("resolve", |bencher| {
        bencher.iter(|| resolve::resolve(black_box(&program)));
    });
    group.bench_function("typecheck", |bencher| {
        bencher.iter(|| typecheck::check(black_box(&program)));
    });
    group.finish();
}

fn backend(criterion: &mut Criterion) {
    let source = benchmark_source(80);
    let tokens = lexer::lex(&source).tokens;
    let program = parser::parse(&tokens).program;
    let lowered = ir::lower(&program);

    let mut group = criterion.benchmark_group("backend");
    group.bench_function("lower", |bencher| {
        bencher.iter(|| ir::lower(black_box(&program)));
    });
    // `optimize` mutates in place, so each iteration needs a fresh clone. With
    // `BatchSize::SmallInput` criterion excludes that setup from the timing.
    group.bench_function("optimize", |bencher| {
        bencher.iter_batched_ref(
            || lowered.clone(),
            |program| optimize::optimize(black_box(program)),
            BatchSize::SmallInput,
        );
    });

    let mut optimized = lowered.clone();
    optimize::optimize(&mut optimized);
    group.bench_function("codegen", |bencher| {
        bencher.iter(|| bytecode::generate(black_box(&optimized)));
    });
    group.bench_function("disassemble", |bencher| {
        let program = bytecode::generate(&optimized);
        bencher.iter(|| black_box(&program).disassemble());
    });
    group.finish();
}

criterion_group!(benches, frontend, backend);
criterion_main!(benches);
