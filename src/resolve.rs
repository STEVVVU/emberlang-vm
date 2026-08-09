use crate::ast::{Block, Expr, ExprKind, Program, Stmt};
use crate::collections::HashMap;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[must_use]
pub fn resolve(program: &Program) -> Vec<Diagnostic> {
    Resolver::new(program).run(program)
}

struct Resolver {
    functions: HashMap<String, Span>,
    scopes: Vec<HashMap<String, Span>>,
    diagnostics: Vec<Diagnostic>,
}

impl Resolver {
    fn new(program: &Program) -> Self {
        let mut functions = HashMap::default();
        let mut diagnostics = Vec::new();
        for function in &program.functions {
            if let Some(previous) = functions.insert(function.name.clone(), function.name_span) {
                diagnostics.push(
                    Diagnostic::error(
                        "E3001",
                        format!("function `{}` is defined more than once", function.name),
                        function.name_span,
                    )
                    .with_label("redefined here")
                    .with_secondary(previous, "first defined here")
                    .with_help("rename or remove one of the functions"),
                );
            }
        }
        Self {
            functions,
            scopes: Vec::new(),
            diagnostics,
        }
    }

    fn run(mut self, program: &Program) -> Vec<Diagnostic> {
        if !self.functions.contains_key("main") {
            self.diagnostics.push(
                Diagnostic::error("E3002", "program has no `main` function", Span::new(0, 0))
                    .with_help("add `fn main() -> int { return 0; }`"),
            );
        }
        for function in &program.functions {
            self.push_scope();
            for parameter in &function.params {
                self.define(&parameter.name, parameter.name_span, "parameter");
            }
            self.block(&function.body, false);
            self.pop_scope();
        }
        self.diagnostics
    }

    fn block(&mut self, block: &Block, nested: bool) {
        if nested {
            self.push_scope();
        }
        for statement in &block.statements {
            self.statement(statement);
        }
        if nested {
            self.pop_scope();
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let {
                name,
                name_span,
                value,
                ..
            } => {
                self.expression(value);
                self.define(name, *name_span, "variable");
            }
            Stmt::Assign {
                name,
                name_span,
                value,
                ..
            } => {
                if self.lookup(name).is_none() {
                    self.unknown_name(name, *name_span, "variable");
                }
                self.expression(value);
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.expression(condition);
                self.block(then_block, true);
                if let Some(block) = else_block {
                    self.block(block, true);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expression(condition);
                self.block(body, true);
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Stmt::Print { value, .. } | Stmt::Expr { value, .. } => self.expression(value),
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match &expression.kind {
            ExprKind::Variable(name) => {
                if self.lookup(name).is_none() {
                    self.unknown_name(name, expression.span, "variable");
                }
            }
            ExprKind::Unary { operand, .. } => self.expression(operand),
            ExprKind::Binary { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            ExprKind::Call {
                callee,
                callee_span,
                args,
            } => {
                if !self.functions.contains_key(callee) {
                    self.unknown_name(callee, *callee_span, "function");
                }
                for argument in args {
                    self.expression(argument);
                }
            }
            ExprKind::Integer(_) | ExprKind::Boolean(_) => {}
        }
    }

    fn define(&mut self, name: &str, span: Span, kind: &str) {
        let scope = self.scopes.last_mut().expect("resolver scope exists");
        if let Some(previous) = scope.insert(name.to_owned(), span) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E3003",
                    format!("{kind} `{name}` is already defined in this scope"),
                    span,
                )
                .with_label(format!("this {kind} shadows the one above"))
                .with_secondary(previous, format!("`{name}` first defined here"))
                .with_help(format!("choose a different {kind} name")),
            );
        }
    }

    fn unknown_name(&mut self, name: &str, span: Span, kind: &str) {
        let mut candidates: Vec<&str> = if kind == "function" {
            self.functions.keys().map(String::as_str).collect()
        } else {
            self.scopes
                .iter()
                .flat_map(|scope| scope.keys().map(String::as_str))
                .collect()
        };
        // Hash-map iteration order is not a stable contract, so sort before
        // picking a suggestion. Otherwise two names at the same edit distance
        // would tie-break differently between runs and the suggested spelling
        // in the diagnostic would not be reproducible.
        candidates.sort_unstable();
        let mut diagnostic = Diagnostic::error("E3004", format!("unknown {kind} `{name}`"), span)
            .with_label(format!("no {kind} named `{name}` is in scope"));
        if let Some(candidate) = closest(name, &candidates) {
            diagnostic = diagnostic.with_help(format!("did you mean `{candidate}`?"));
        } else {
            diagnostic = diagnostic.with_help(format!("define `{name}` before using it"));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lookup(&self, name: &str) -> Option<Span> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn closest<'a>(needle: &str, candidates: &'a [&str]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|candidate| (*candidate, edit_distance(needle, candidate)))
        .filter(|(_, distance)| *distance <= 2)
        .min_by_key(|(_, distance)| *distance)
        .map(|(candidate, _)| candidate)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (left_index, left_byte) in left.bytes().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_byte) in right.bytes().enumerate() {
            let substitution = previous[right_index] + usize::from(left_byte != right_byte);
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            current.push(substitution.min(insertion).min(deletion));
        }
        previous = current;
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn parsed(source: &str) -> Program {
        parser::parse(&lexer::lex(source).tokens).program
    }

    #[test]
    fn suggests_similar_local_name() {
        let diagnostics = resolve(&parsed(
            "fn main() -> int { let count: int = 1; print(cout); return 0; }",
        ));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.help.as_deref() == Some("did you mean `count`?")));
    }

    #[test]
    fn nested_scope_does_not_escape() {
        let diagnostics = resolve(&parsed(
            "fn main() -> int { if true { let x: int = 1; } print(x); return 0; }",
        ));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown variable `x`")));
    }
}
