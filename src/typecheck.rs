use crate::ast::{BinaryOp, Block, Expr, ExprKind, Function, Program, Stmt, TypeName, UnaryOp};
use crate::collections::HashMap;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[must_use]
pub fn check(program: &Program) -> Vec<Diagnostic> {
    Checker::new(program).run(program)
}

#[derive(Clone)]
struct Signature {
    params: Vec<TypeName>,
    return_type: TypeName,
    span: Span,
}

struct Checker {
    signatures: HashMap<String, Signature>,
    scopes: Vec<HashMap<String, TypeName>>,
    diagnostics: Vec<Diagnostic>,
    current_return: TypeName,
}

impl Checker {
    fn new(program: &Program) -> Self {
        let signatures = program
            .functions
            .iter()
            .map(|function| {
                (
                    function.name.clone(),
                    Signature {
                        params: function
                            .params
                            .iter()
                            .map(|parameter| parameter.ty)
                            .collect(),
                        return_type: function.return_type,
                        span: function.name_span,
                    },
                )
            })
            .collect();
        Self {
            signatures,
            scopes: Vec::new(),
            diagnostics: Vec::new(),
            current_return: TypeName::Unit,
        }
    }

    fn run(mut self, program: &Program) -> Vec<Diagnostic> {
        if let Some(main) = self.signatures.get("main") {
            if !main.params.is_empty() || main.return_type != TypeName::Int {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E4001",
                        "`main` must have signature `fn main() -> int`",
                        main.span,
                    )
                    .with_help("remove parameters and return an integer exit code"),
                );
            }
        }
        for function in &program.functions {
            self.function(function);
        }
        self.diagnostics
    }

    fn function(&mut self, function: &Function) {
        self.current_return = function.return_type;
        self.scopes.push(
            function
                .params
                .iter()
                .map(|parameter| (parameter.name.clone(), parameter.ty))
                .collect(),
        );
        self.block(&function.body, false);
        self.scopes.pop();
        if function.return_type != TypeName::Unit && !guarantees_return(&function.body) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E4002",
                    format!(
                        "function `{}` can finish without returning `{}`",
                        function.name, function.return_type
                    ),
                    function.name_span,
                )
                .with_help("add a return on every control-flow path"),
            );
        }
    }

    fn block(&mut self, block: &Block, nested: bool) {
        if nested {
            self.scopes.push(HashMap::default());
        }
        for statement in &block.statements {
            self.statement(statement);
        }
        if nested {
            self.scopes.pop();
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let {
                name,
                ty,
                value,
                span,
                ..
            } => {
                let actual = self.expression(value);
                self.require(
                    *ty,
                    actual,
                    *span,
                    "initializer type does not match annotation",
                );
                self.scopes
                    .last_mut()
                    .expect("type scope exists")
                    .insert(name.clone(), *ty);
            }
            Stmt::Assign {
                name, value, span, ..
            } => {
                let expected = self.lookup(name);
                let actual = self.expression(value);
                if let Some(expected) = expected {
                    self.require(expected, actual, *span, "assignment changes variable type");
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let actual = self.expression(condition);
                self.require(
                    TypeName::Bool,
                    actual,
                    condition.span,
                    "`if` condition must be boolean",
                );
                self.block(then_block, true);
                if let Some(block) = else_block {
                    self.block(block, true);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                let actual = self.expression(condition);
                self.require(
                    TypeName::Bool,
                    actual,
                    condition.span,
                    "`while` condition must be boolean",
                );
                self.block(body, true);
            }
            Stmt::Return { value, span } => {
                let actual = value
                    .as_ref()
                    .map_or(Some(TypeName::Unit), |value| self.expression(value));
                self.require(
                    self.current_return,
                    actual,
                    *span,
                    "return type does not match function signature",
                );
            }
            Stmt::Print { value, .. } => {
                self.expression(value);
            }
            Stmt::Expr { value, span } => {
                if self
                    .expression(value)
                    .is_some_and(|ty| ty != TypeName::Unit)
                {
                    self.diagnostics.push(
                        Diagnostic::error("E4003", "unused expression value", *span)
                            .with_help("assign the value, print it, or return it"),
                    );
                }
            }
        }
    }

    fn expression(&mut self, expression: &Expr) -> Option<TypeName> {
        match &expression.kind {
            ExprKind::Integer(_) => Some(TypeName::Int),
            ExprKind::Boolean(_) => Some(TypeName::Bool),
            ExprKind::Variable(name) => self.lookup(name),
            ExprKind::Unary { op, operand } => {
                let actual = self.expression(operand);
                let expected = match op {
                    UnaryOp::Negate => TypeName::Int,
                    UnaryOp::Not => TypeName::Bool,
                };
                self.require(expected, actual, expression.span, "invalid unary operand");
                Some(expected)
            }
            ExprKind::Binary { left, op, right } => {
                let left_type = self.expression(left);
                let right_type = self.expression(right);
                Some(self.binary_type(*op, left_type, right_type, expression.span))
            }
            ExprKind::Call { callee, args, .. } => {
                let argument_types: Vec<Option<TypeName>> = args
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect();
                let signature = self.signatures.get(callee).cloned()?;
                if signature.params.len() != args.len() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E4004",
                            format!(
                                "function `{callee}` expects {} but received {}",
                                plural_arguments(signature.params.len()),
                                args.len()
                            ),
                            expression.span,
                        )
                        .with_label(format!("call site passes {}", plural_arguments(args.len())))
                        .with_secondary(
                            signature.span,
                            format!(
                                "`{callee}` is declared here with {}",
                                plural_parameters(signature.params.len())
                            ),
                        ),
                    );
                }
                for (index, (expected, actual)) in
                    signature.params.iter().zip(argument_types).enumerate()
                {
                    self.require(
                        *expected,
                        actual,
                        args[index].span,
                        format!("argument {} has the wrong type", index + 1),
                    );
                }
                Some(signature.return_type)
            }
        }
    }

    fn binary_type(
        &mut self,
        op: BinaryOp,
        left: Option<TypeName>,
        right: Option<TypeName>,
        span: Span,
    ) -> TypeName {
        let (expected, result) = match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Remainder => (TypeName::Int, TypeName::Int),
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                (TypeName::Int, TypeName::Bool)
            }
            BinaryOp::And | BinaryOp::Or => (TypeName::Bool, TypeName::Bool),
            BinaryOp::Equal | BinaryOp::NotEqual => {
                if let (Some(left), Some(right)) = (left, right) {
                    if left != right || left == TypeName::Unit {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E4005",
                                format!("cannot compare `{left}` with `{right}`"),
                                span,
                            )
                            .with_help("both operands must have the same non-unit type"),
                        );
                    }
                }
                return TypeName::Bool;
            }
        };
        self.require(expected, left, span, "left operand has the wrong type");
        self.require(expected, right, span, "right operand has the wrong type");
        result
    }

    fn require(
        &mut self,
        expected: TypeName,
        actual: Option<TypeName>,
        span: Span,
        context: impl Into<String>,
    ) {
        if let Some(actual) = actual {
            if expected != actual {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E4006",
                        format!(
                            "{}: expected `{expected}`, found `{actual}`",
                            context.into()
                        ),
                        span,
                    )
                    .with_label(format!("this has type `{actual}`"))
                    .with_help(format!("use an expression of type `{expected}`")),
                );
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<TypeName> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

fn plural_arguments(count: usize) -> String {
    if count == 1 {
        "1 argument".to_owned()
    } else {
        format!("{count} arguments")
    }
}

fn plural_parameters(count: usize) -> String {
    if count == 1 {
        "1 parameter".to_owned()
    } else {
        format!("{count} parameters")
    }
}

fn guarantees_return(block: &Block) -> bool {
    block.statements.iter().any(|statement| match statement {
        Stmt::Return { .. } => true,
        Stmt::If {
            then_block,
            else_block: Some(else_block),
            ..
        } => guarantees_return(then_block) && guarantees_return(else_block),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn diagnostics(source: &str) -> Vec<Diagnostic> {
        let program = parser::parse(&lexer::lex(source).tokens).program;
        check(&program)
    }

    #[test]
    fn rejects_non_boolean_condition() {
        let result = diagnostics("fn main() -> int { if 1 { return 0; } return 1; }");
        assert!(result
            .iter()
            .any(|diagnostic| diagnostic.message.contains("condition must be boolean")));
    }

    #[test]
    fn checks_call_arity() {
        let result = diagnostics(
            "fn add(x: int, y: int) -> int { return x + y; } fn main() -> int { return add(1); }",
        );
        assert!(result
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expects 2 arguments")));
    }
}
