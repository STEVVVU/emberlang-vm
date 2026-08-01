# Architecture

## Goals

Ember is deliberately a compiler/VM project rather than a language-design
showcase. The syntax is small so the implementation can focus on source
tracking, recovery, semantic passes, an inspectable IR, optimization, bytecode,
determinism, and bounded execution.

The implementation has no third-party Rust dependencies. This keeps the build
and trust surface small and makes each compiler stage visible in the repository.

## Language

Ember has three types: `int` (signed 64-bit), `bool`, and `unit`. Programs
contain typed functions, lexical blocks, typed mutable locals, assignment,
`if`/`else`, `while`, `return`, calls, and `print`.

An abbreviated grammar:

```text
program     := function* EOF
function    := "fn" IDENT "(" parameters? ")" "->" type block
parameters  := IDENT ":" type ("," IDENT ":" type)*
block       := "{" statement* "}"
statement   := "let" IDENT ":" type "=" expression ";"
             | IDENT "=" expression ";"
             | "if" expression block ("else" block)?
             | "while" expression block
             | "return" expression? ";"
             | "print" "(" expression ")" ";"
             | expression ";"
expression  := unary | expression binary-op expression | call
type        := "int" | "bool" | "unit"
```

Integers use checked arithmetic at runtime. Boolean `&&` and `||` are eager;
there are no implicit conversions. `main` must be `fn main() -> int`.

## Pipeline

1. **Lexer** (`src/lexer.rs`) produces spanned tokens. Unknown characters and
   unterminated comments become diagnostics without discarding later tokens.
2. **Parser** (`src/parser.rs`) uses precedence climbing for expressions and
   recursive descent elsewhere. On failure it synchronizes at statement
   boundaries or the next top-level `fn`, allowing multiple errors per run.
3. **Resolver** (`src/resolve.rs`) builds lexical scopes, checks duplicate and
   unknown names, resolves calls, and offers edit-distance suggestions.
4. **Type checker** (`src/typecheck.rs`) validates annotations, operators,
   calls, conditions, returns, `main`, and definite return for non-unit
   functions.
5. **IR lowering** (`src/ir.rs`) turns structured control flow into labels,
   explicit branches, typed constants, virtual registers, and calls.
6. **Optimization** (`src/optimize.rs`) performs local constant
   propagation/folding and iterative dead-result elimination.
7. **Bytecode generation** (`src/bytecode.rs`) resolves labels and function
   names into numeric instruction offsets and indexes.
8. **VM** (`src/vm.rs`) executes register frames with checked operations and
   explicit resource limits.

All phases preserve byte-offset spans. Diagnostics convert offsets to line and
column only when rendered, avoiding repeated source scans during compilation.

## Intermediate Representation

The IR is a linearized control-flow graph. Labels identify basic-block entries;
instructions refer to virtual registers:

```text
bb0:
  %0 = const 2
  %1 = const 3
  %2 = Add %0, %1
  branch %condition, bb1, bb2
bb1:
  print %2
  jump bb3
bb2:
  jump bb3
bb3:
  return %2
```

IR instructions are:

- `Label`, `Jump`, `Branch`, and `Return` for control flow;
- `Const`, `Copy`, `Unary`, and `Binary` for pure computation;
- `Call` and `Print` for observable effects.

Source locals use stable registers, which makes mutation across loop back-edges
correct without SSA phi nodes. Expression temporaries receive fresh registers.
This is not SSA: multiple assignments may define a local's register.

### Constant folding

The pass tracks constants within a basic block, folds checked unary/binary
operations, propagates constant copies, and replaces constant branches with
jumps. State is cleared at labels and terminators, so it makes no
cross-block assumptions. Invalid operations such as division by zero are left
for the VM to trap with the original source span.

### Dead-result elimination

The pass counts register uses and removes pure definitions with zero uses. It
repeats to a fixed point so removing one result can expose an entire dead
expression tree. Calls and prints are retained because they may have effects.
Multiple definitions of a live mutable register are conservatively retained.

## Bytecode and VM

Bytecode is register-based, not stack-based. Each function declares a fixed
register count and parameter prefix. Labels become absolute instruction
offsets; calls become function-table indexes. Disassembly is deterministic
because source order and numeric allocation are deterministic and hash-map
iteration never controls emitted order.

The VM has no filesystem, network, environment, FFI, dynamic code loading, or
heap/object instructions. Its only observable operation is appending scalar
values to captured output. Default limits are:

| Resource | Default |
| --- | ---: |
| Source bytes | 1,000,000 |
| Tokens | 250,000 |
| VM instructions | 1,000,000 |
| Call depth | 256 |
| Output bytes | 1,000,000 |
| Registers per frame | 65,536 |
| Functions | 10,000 |

The CLI exposes the three operational VM limits most useful to callers. Integer
overflow, division by zero, exhausted limits, and malformed bytecode become
source-mapped traps with a language-level call stack.

## Tradeoffs and Extensions

- Stable mutable registers make loops straightforward but prevent aggressive
  SSA data-flow optimization. A production next step would construct basic
  blocks, dominators, and phi nodes.
- Constant propagation is intrablock. Sparse conditional constant propagation
  would improve cross-block results.
- Dead code covers unused pure values, not unreachable blocks.
- There is no module system, strings, heap, closures, generics, or user-defined
  data. These omissions keep resource accounting and semantics auditable.
- Diagnostics carry one primary span rather than arbitrary multi-span labels.
- Bytecode is in-memory only; a versioned serializer and verifier would be
  required before accepting untrusted bytecode files.

