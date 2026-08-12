//! End-to-end benchmark harness that emits the committed JSON baseline.
//!
//! This is deliberately kept alongside the criterion suites in `benches/`; the
//! two answer different questions and neither replaces the other:
//!
//! * `cargo bench` (criterion) is for *comparison* — per-stage latency with
//!   confidence intervals and `--save-baseline`/`--baseline` regression checks.
//!   Its reports are machine-local HTML/JSON under `target/`, not artifacts.
//! * this binary is for *evidence* — one deterministic, dependency-free run that
//!   prints a single JSON document, including the optimizer counters and total
//!   retired instruction count that back the README's claims. It is what
//!   `benchmarks/results-*.json` is generated from, and what CI smoke-runs,
//!   because it needs no criterion and finishes in seconds.

use emberlang_vm::vm::{execute, VmLimits};
use emberlang_vm::{compile, CompileOptions};
use std::fmt::Write;
use std::hint::black_box;
use std::time::{Duration, Instant};

const COMPILE_ITERATIONS: usize = 200;
const VM_ITERATIONS: usize = 50;

fn main() {
    let source = benchmark_source(80);
    let mut compile_samples = Vec::with_capacity(COMPILE_ITERATIONS);
    let mut last_instruction_count = 0;
    let mut folded = 0;
    let mut removed = 0;

    for _ in 0..COMPILE_ITERATIONS {
        let start = Instant::now();
        let compilation = compile(black_box(&source), CompileOptions::default())
            .expect("benchmark source must compile");
        compile_samples.push(start.elapsed());
        last_instruction_count = compilation
            .bytecode
            .functions
            .iter()
            .map(|function| function.code.len())
            .sum();
        folded = compilation.ir.optimization_stats.constants_folded;
        removed = compilation.ir.optimization_stats.dead_instructions_removed;
        black_box(compilation);
    }

    compile_samples.sort_unstable();
    let compilation =
        compile(&source, CompileOptions::default()).expect("benchmark source must compile");
    let limits = VmLimits {
        max_steps: 10_000_000,
        max_output_bytes: 1_000,
        ..VmLimits::default()
    };
    let start = Instant::now();
    let mut vm_steps = 0_u64;
    for _ in 0..VM_ITERATIONS {
        let execution = execute(black_box(&compilation.bytecode), limits)
            .expect("benchmark program must execute");
        vm_steps += execution.steps;
        black_box(execution);
    }
    let vm_elapsed = start.elapsed();
    let steps_per_second = u128::from(vm_steps) * 1_000_000 / vm_elapsed.as_micros().max(1);

    println!("{{");
    println!("  \"schema\": 1,");
    println!("  \"profile\": \"release\",");
    println!("  \"source_bytes\": {},", source.len());
    println!("  \"source_functions\": 81,");
    println!("  \"compile_iterations\": {COMPILE_ITERATIONS},");
    println!(
        "  \"compile_median_us\": {},",
        micros(compile_samples[COMPILE_ITERATIONS / 2])
    );
    println!(
        "  \"compile_p95_us\": {},",
        micros(compile_samples[COMPILE_ITERATIONS * 95 / 100])
    );
    println!("  \"bytecode_instructions\": {last_instruction_count},");
    println!("  \"constants_folded\": {folded},");
    println!("  \"dead_instructions_removed\": {removed},");
    println!("  \"vm_iterations\": {VM_ITERATIONS},");
    println!("  \"vm_total_steps\": {vm_steps},");
    println!("  \"vm_elapsed_us\": {},", micros(vm_elapsed));
    println!("  \"vm_steps_per_second\": {steps_per_second}");
    println!("}}");
}

fn micros(duration: Duration) -> u128 {
    duration.as_micros()
}

fn benchmark_source(helper_count: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_benchmark_is_valid() {
        let source = benchmark_source(3);
        let compilation = compile(&source, CompileOptions::default()).unwrap();
        assert_eq!(compilation.bytecode.functions.len(), 4);
    }
}
