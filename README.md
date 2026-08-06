# Ember Compiler and VM

Ember is a compact, statically typed compiler toolchain written in Rust. It
implements the work expected in a real compiler pipeline: source spans and
parser recovery, lexical name resolution, type checking, an inspectable
register IR, optimization passes, bytecode generation, diagnostics, and a
resource-limited VM.

The language is intentionally small. The project is about compiler engineering,
not unusual syntax or novelty features.

## Quick Start

Requires stable Rust (tested with Rust 1.96.0).

```sh
cargo build --release --bin ember
cargo run --bin ember -- check examples/fibonacci.em
cargo run --bin ember -- run examples/fibonacci.em
cargo run --bin ember -- disassemble examples/optimizer.em
```

Expected Fibonacci output:

```text
55
```

Install the CLI locally with `cargo install --path . --bin ember`.

## CLI

```text
ember check <file> [--no-opt]
ember run <file> [--no-opt] [--max-steps N] [--max-call-depth N] [--max-output-bytes N]
ember disassemble <file> [--no-opt]
```

- `check` runs lexing through type checking and reports optimization counters.
- `run` compiles and executes `main`, using its integer result as the exit code.
- `disassemble` prints deterministic register bytecode with resolved jumps.

Diagnostics use stable error codes, source locations, underlines, notes, and
actionable help:

```text
error[E4006]: initializer type does not match annotation: expected `int`, found `bool`
 --> tests/golden/type_error.em:2:5
  |
2 |     let attempts: int = true;
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
 help: use an expression of type `int`
```

## Example

```text
fn sum(n: int) -> int {
    let total: int = 0;
    let index: int = 0;
    while index < n {
        total = total + index;
        index = index + 1;
    }
    return total;
}

fn main() -> int {
    print(sum(10));
    return 0;
}
```

See `examples/` for Fibonacci, nested control flow, and optimization-focused
programs.

## Compiler Pipeline

```text
source
  -> spanned tokens
  -> recovered AST
  -> name resolution
  -> static type checking
  -> register IR
  -> constant folding
  -> dead-result elimination
  -> bytecode
  -> sandboxed VM
```

The IR has explicit labels, branches, virtual registers, calls, and
source-bearing instructions. Constant folding is conservative at control-flow
boundaries. Dead-result elimination iterates to a fixed point and never removes
calls or output. See [`docs/architecture.md`](docs/architecture.md) for the
grammar, IR contract, pass invariants, bytecode model, and design tradeoffs.

## Safety and Determinism

The VM exposes no host filesystem, network, environment, or FFI operations.
Checked `i64` arithmetic traps on overflow and division by zero. Defaults bound
execution to 1,000,000 instructions, 256 call frames, 1 MB output, 65,536
registers per frame, and 10,000 functions. Compilation also caps source size and
token count.

Function/register allocation, optimization order, bytecode, disassembly, and
output are deterministic for identical source and options. A test compiles and
disassembles the same source twice and compares exact output.

## Verification

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release --bins
```

The 21-test suite includes:

- lexer/parser recovery and precedence;
- resolver scope errors and typo suggestions;
- operator, call, condition, and return typing;
- constant folding and dead-result removal;
- optimized/unoptimized behavior equivalence;
- function calls, loops, arithmetic traps, and VM limits;
- deterministic disassembly;
- CLI integration and an exact diagnostic golden file.

CI runs formatting, strict Clippy, all tests, and release builds on Linux and
Windows, plus a release-mode benchmark smoke test.

## Measured Evidence

`cargo run --release --bin benchmark` generates a deterministic 8,441-byte,
81-function input. On the committed Windows x86_64 baseline:

| Metric | Observed |
| --- | ---: |
| Full compile median (200 iterations) | 2.197 ms |
| Full compile p95 | 6.716 ms |
| Constants folded per compile | 402 |
| Dead instructions removed per compile | 722 |
| Resulting bytecode instructions | 336 |
| VM throughput (5,000,400 instructions) | 98.2 M instructions/s |

These are single-machine engineering baselines, not universal claims. The raw
artifact and methodology are in [`benchmarks/`](benchmarks/).

## Limitations

Ember has no modules, strings, heap objects, closures, generics, or native
interop. The IR is not SSA; constant propagation is intrablock, and dead-code
elimination removes unused pure results but not unreachable blocks. Bytecode is
generated and executed in memory rather than loaded from untrusted files.

Those choices keep semantics and resource accounting reviewable. The natural
next compiler work would be CFG construction, dominators/SSA, cross-block data
flow, a bytecode verifier, and property/fuzz testing.

## Resume Bullets

- Built a dependency-free Rust compiler pipeline with recovering parser, lexical
  resolver, static type checker, documented register IR, deterministic bytecode,
  and source-mapped diagnostics.
- Implemented constant folding and fixed-point dead-result elimination; on an
  81-function benchmark they folded 402 constants and removed 722 instructions,
  producing 336 bytecode instructions.
- Designed a sandboxed register VM with checked arithmetic and configurable
  instruction/call/output limits; measured 98.2M instructions/s over 5.0M
  release-mode dispatches on the recorded Windows baseline.

## License

MIT
