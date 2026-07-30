use emberlang_vm::diagnostic::render_all;
use emberlang_vm::vm::{execute, VmLimits};
use emberlang_vm::{compile, CompileOptions};

#[test]
fn fibonacci_runs_end_to_end() {
    let source = include_str!("../examples/fibonacci.em");
    let compilation = compile(source, CompileOptions::default()).unwrap();
    let execution = execute(&compilation.bytecode, VmLimits::default()).unwrap();
    assert_eq!(execution.output, "55\n");
    assert_eq!(execution.exit_code, 0);
}

#[test]
fn optimized_and_unoptimized_programs_match() {
    let source = include_str!("../examples/optimizer.em");
    let optimized = compile(source, CompileOptions { optimize: true }).unwrap();
    let baseline = compile(source, CompileOptions { optimize: false }).unwrap();
    let optimized_result = execute(&optimized.bytecode, VmLimits::default()).unwrap();
    let baseline_result = execute(&baseline.bytecode, VmLimits::default()).unwrap();
    assert_eq!(optimized_result.output, baseline_result.output);
    let optimized_size: usize = optimized
        .bytecode
        .functions
        .iter()
        .map(|function| function.code.len())
        .sum();
    let baseline_size: usize = baseline
        .bytecode
        .functions
        .iter()
        .map(|function| function.code.len())
        .sum();
    assert!(optimized_size < baseline_size);
}

#[test]
fn golden_type_error_is_stable() {
    let source = include_str!("golden/type_error.em");
    let diagnostics = compile(source, CompileOptions::default()).unwrap_err();
    let actual = render_all("tests/golden/type_error.em", source, &diagnostics);
    assert_eq!(actual, include_str!("golden/type_error.stderr"));
}

#[test]
fn disassembly_is_deterministic() {
    let source = include_str!("../examples/control_flow.em");
    let first = compile(source, CompileOptions::default())
        .unwrap()
        .bytecode
        .disassemble();
    let second = compile(source, CompileOptions::default())
        .unwrap()
        .bytecode
        .disassemble();
    assert_eq!(first, second);
    assert!(first.contains("JUMP_IF_FALSE"));
}
