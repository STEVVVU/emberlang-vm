//! Snapshot coverage for every reviewable artifact the compiler emits:
//! rendered diagnostics, register IR (unoptimized and optimized), and bytecode
//! disassembly.
//!
//! Snapshots are committed under `tests/snapshots/`, so `cargo test` verifies
//! them with no `cargo insta` step in CI. Regenerate deliberately with
//! `INSTA_UPDATE=always cargo test --test snapshots` (or `cargo insta review`)
//! and read the diff before committing it.
//!
//! Everything asserted here is rendered through [`RenderStyle::Plain`], which is
//! ASCII-only and escape-free, so a snapshot recorded on Windows matches one
//! verified on Linux byte for byte.

use emberlang_vm::diagnostic::{render_all, RenderStyle};
use emberlang_vm::vm::{execute, VmLimits};
use emberlang_vm::{compile, CompileOptions};

/// Compiles a source that is expected to fail and renders its diagnostics.
fn render_errors(filename: &str, source: &str) -> String {
    let diagnostics = compile(source, CompileOptions::default())
        .err()
        .unwrap_or_else(|| panic!("`{filename}` was expected to fail compilation"));
    render_all(filename, source, &diagnostics)
}

#[test]
fn lex_error_diagnostics() {
    let source = include_str!("golden/lex_error.em");
    insta::assert_snapshot!(render_errors("tests/golden/lex_error.em", source));
}

#[test]
fn parse_error_diagnostics() {
    let source = include_str!("golden/parse_error.em");
    insta::assert_snapshot!(render_errors("tests/golden/parse_error.em", source));
}

/// Covers duplicate definitions, an unknown name with a typo suggestion, and a
/// call-arity error — the three diagnostics that carry a secondary span.
#[test]
fn resolve_and_arity_error_diagnostics() {
    let source = include_str!("golden/resolve_error.em");
    insta::assert_snapshot!(render_errors("tests/golden/resolve_error.em", source));
}

/// Replaces the former `tests/golden/type_error.stderr` comparison. The two
/// assertions that file backed — an exact byte-for-byte render, and the presence
/// of the `E4006` code, location, underline, and help — are both still enforced,
/// the first by the committed snapshot and the second explicitly below.
#[test]
fn type_error_diagnostics() {
    let source = include_str!("golden/type_error.em");
    let rendered = render_errors("tests/golden/type_error.em", source);
    assert!(rendered.contains("E4006"), "lost the stable error code");
    assert!(
        rendered.contains("tests/golden/type_error.em:2:5"),
        "lost the source location"
    );
    assert!(rendered.contains('^'), "lost the span underline");
    assert!(
        rendered.contains("Help: use an expression of type `int`"),
        "lost the actionable help"
    );
    insta::assert_snapshot!(rendered);
}

/// The rendered diagnostic must stay free of ANSI escapes and non-ASCII bytes so
/// snapshots are identical on Windows and Linux runners.
#[test]
fn rendered_diagnostics_are_platform_independent() {
    let source = include_str!("golden/resolve_error.em");
    let rendered = render_errors("tests/golden/resolve_error.em", source);
    assert!(!rendered.contains('\u{1b}'), "ANSI escape in plain render");
    assert!(rendered.is_ascii(), "non-ASCII byte in plain render");
    assert!(!rendered.contains('\r'), "carriage return in plain render");
    assert!(
        !rendered.contains(" \n"),
        "trailing padding in plain render"
    );

    // A CRLF copy of the same source must render byte-identically, so a checkout
    // with `core.autocrlf=true` does not change the snapshot.
    let crlf = source.replace('\n', "\r\n");
    assert_eq!(
        render_errors("tests/golden/resolve_error.em", &crlf),
        rendered,
        "CRLF source changed the rendered diagnostic"
    );
}

/// A runtime trap is source-mapped through the same renderer as a compile error.
#[test]
fn vm_trap_diagnostics() {
    let source = "fn main() -> int {\n    let divisor: int = 0;\n    return 10 / divisor;\n}\n";
    let compilation = compile(source, CompileOptions::default()).unwrap();
    let error = execute(&compilation.bytecode, VmLimits::default()).unwrap_err();
    insta::assert_snapshot!(error.diagnostic().render("trap.em", source));
}

/// A resource-limit trap must name the limit it hit, so the sandbox stays
/// observable rather than silently truncating.
#[test]
fn vm_step_limit_diagnostics() {
    let source =
        "fn main() -> int {\n    while true {\n        print(1);\n    }\n    return 0;\n}\n";
    let compilation = compile(source, CompileOptions::default()).unwrap();
    let limits = VmLimits {
        max_steps: 12,
        ..VmLimits::default()
    };
    let error = execute(&compilation.bytecode, limits).unwrap_err();
    assert_eq!(error.code, "E7104", "lost the step-limit error code");
    insta::assert_snapshot!(error.diagnostic().render("limit.em", source));
}

#[test]
fn unoptimized_ir_dump() {
    let source = include_str!("../examples/optimizer.em");
    let compilation = compile(source, CompileOptions { optimize: false }).unwrap();
    insta::assert_snapshot!(compilation.ir.to_string());
}

/// Pairs with the dump above: the diff between the two snapshots *is* the
/// reviewable record of what constant folding and dead-result elimination did.
#[test]
fn optimized_ir_dump() {
    let source = include_str!("../examples/optimizer.em");
    let compilation = compile(source, CompileOptions::default()).unwrap();
    assert!(compilation.ir.optimization_stats.constants_folded > 0);
    assert!(compilation.ir.optimization_stats.dead_instructions_removed > 0);
    insta::assert_snapshot!(compilation.ir.to_string());
}

#[test]
fn control_flow_disassembly() {
    let source = include_str!("../examples/control_flow.em");
    let compilation = compile(source, CompileOptions::default()).unwrap();
    insta::assert_snapshot!(compilation.bytecode.disassemble());
}

#[test]
fn fibonacci_disassembly() {
    let source = include_str!("../examples/fibonacci.em");
    let compilation = compile(source, CompileOptions::default()).unwrap();
    insta::assert_snapshot!(compilation.bytecode.disassemble());
}

/// The colored style is exercised so a change to it cannot slip through
/// unnoticed, but only its *shape* is asserted: the escape sequences themselves
/// are terminal-dependent and deliberately never snapshotted.
#[test]
fn colored_style_wraps_the_same_content() {
    let source = include_str!("golden/type_error.em");
    let diagnostics = compile(source, CompileOptions::default()).unwrap_err();
    let colored = emberlang_vm::diagnostic::render_all_with(
        "tests/golden/type_error.em",
        source,
        &diagnostics,
        RenderStyle::Colored,
    );
    assert!(colored.contains('\u{1b}'), "expected ANSI escapes");
    assert!(colored.contains("E4006"));
    assert!(colored.contains("initializer type does not match annotation"));
}
