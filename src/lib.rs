pub mod ast;
pub mod bytecode;
pub mod diagnostic;
pub mod ir;
pub mod lexer;
pub mod optimize;
pub mod parser;
pub mod resolve;
pub mod span;
pub mod token;
pub mod typecheck;
pub mod vm;

use ast::Program;
use bytecode::BytecodeProgram;
use diagnostic::Diagnostic;
use ir::IrProgram;

pub const MAX_SOURCE_BYTES: usize = 1_000_000;
pub const MAX_TOKENS: usize = 250_000;

#[derive(Clone, Copy, Debug)]
pub struct CompileOptions {
    pub optimize: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self { optimize: true }
    }
}

#[derive(Debug)]
pub struct Compilation {
    pub ast: Program,
    pub ir: IrProgram,
    pub bytecode: BytecodeProgram,
}

/// Compiles source through bytecode generation.
///
/// # Errors
///
/// Returns every diagnostic produced by the first failing compiler stages.
pub fn compile(source: &str, options: CompileOptions) -> Result<Compilation, Vec<Diagnostic>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic::error(
            "E0001",
            format!(
                "source is {} bytes; the compiler limit is {MAX_SOURCE_BYTES} bytes",
                source.len()
            ),
            span::Span::new(0, source.len().min(1)),
        )
        .with_help("split the program into a smaller input")]);
    }

    let lexed = lexer::lex(source);
    if lexed.tokens.len() > MAX_TOKENS {
        return Err(vec![Diagnostic::error(
            "E0002",
            format!("token count exceeds the compiler limit of {MAX_TOKENS}"),
            span::Span::new(0, source.len().min(1)),
        )]);
    }

    let parsed = parser::parse(&lexed.tokens);
    let mut diagnostics = lexed.diagnostics;
    diagnostics.extend(parsed.diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let ast = parsed.program;
    diagnostics.extend(resolve::resolve(&ast));
    diagnostics.extend(typecheck::check(&ast));
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let mut ir = ir::lower(&ast);
    if options.optimize {
        optimize::optimize(&mut ir);
    }
    let bytecode = bytecode::generate(&ir);
    Ok(Compilation { ast, ir, bytecode })
}
