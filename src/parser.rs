use crate::ast::{
    BinaryOp, Block, Expr, ExprKind, Function, Param, Program, Stmt, TypeName, UnaryOp,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;
use crate::token::{Token, TokenKind};
use std::mem::discriminant;

#[derive(Debug)]
pub struct ParseResult {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
}

#[must_use]
pub fn parse(tokens: &[Token]) -> ParseResult {
    Parser::new(tokens).parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            cursor: 0,
            diagnostics: Vec::new(),
        }
    }

    fn parse_program(mut self) -> ParseResult {
        let mut functions = Vec::new();
        while !self.at(&TokenKind::Eof) {
            match self.function() {
                Some(function) => functions.push(function),
                None => self.synchronize_top_level(),
            }
        }
        ParseResult {
            program: Program { functions },
            diagnostics: self.diagnostics,
        }
    }

    fn function(&mut self) -> Option<Function> {
        let start = self.expect(&TokenKind::Fn, "E2001", "expected `fn` to start a function")?;
        let (name, name_span) = self.identifier("function name")?;
        self.expect(
            &TokenKind::LeftParen,
            "E2002",
            "expected `(` after function name",
        )?;
        let mut params = Vec::new();
        if !self.at(&TokenKind::RightParen) {
            loop {
                let (param_name, param_span) = self.identifier("parameter name")?;
                self.expect(
                    &TokenKind::Colon,
                    "E2003",
                    "expected `:` after parameter name",
                )?;
                let ty = self.type_name()?;
                params.push(Param {
                    name: param_name,
                    name_span: param_span,
                    ty,
                });
                if !self.consume(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(
            &TokenKind::RightParen,
            "E2004",
            "expected `)` after parameters",
        )?;
        self.expect(
            &TokenKind::Arrow,
            "E2005",
            "expected `->` before return type",
        )?;
        let return_type = self.type_name()?;
        let body = self.block()?;
        Some(Function {
            name,
            name_span,
            params,
            return_type,
            span: start.merge(body.span),
            body,
        })
    }

    fn type_name(&mut self) -> Option<TypeName> {
        let token = self.current().clone();
        let ty = match token.kind {
            TokenKind::IntType => TypeName::Int,
            TokenKind::BoolType => TypeName::Bool,
            TokenKind::UnitType => TypeName::Unit,
            _ => {
                self.error(
                    "E2006",
                    "expected a type (`int`, `bool`, or `unit`)",
                    token.span,
                );
                return None;
            }
        };
        self.advance();
        Some(ty)
    }

    fn block(&mut self) -> Option<Block> {
        let start = self.expect(
            &TokenKind::LeftBrace,
            "E2007",
            "expected `{` to start block",
        )?;
        let mut statements = Vec::new();
        while !self.at(&TokenKind::RightBrace) && !self.at(&TokenKind::Eof) {
            match self.statement() {
                Some(statement) => statements.push(statement),
                None => self.synchronize_statement(),
            }
        }
        let end = self.expect(&TokenKind::RightBrace, "E2008", "expected `}` after block")?;
        Some(Block {
            statements,
            span: start.merge(end),
        })
    }

    fn statement(&mut self) -> Option<Stmt> {
        if self.consume(&TokenKind::Let) {
            return self.let_statement();
        }
        if self.consume(&TokenKind::If) {
            return self.if_statement();
        }
        if self.consume(&TokenKind::While) {
            return self.while_statement();
        }
        if self.consume(&TokenKind::Return) {
            return self.return_statement();
        }
        if self.consume(&TokenKind::Print) {
            return self.print_statement();
        }
        if matches!(self.current().kind, TokenKind::Identifier(_))
            && self.peek_at(&TokenKind::Equal)
        {
            return self.assign_statement();
        }
        let value = self.expression()?;
        let end = self.expect(
            &TokenKind::Semicolon,
            "E2009",
            "expected `;` after expression",
        )?;
        let span = value.span.merge(end);
        Some(Stmt::Expr { value, span })
    }

    fn let_statement(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let (name, name_span) = self.identifier("variable name")?;
        self.expect(
            &TokenKind::Colon,
            "E2010",
            "expected `:` after variable name",
        )?;
        let ty = self.type_name()?;
        self.expect(
            &TokenKind::Equal,
            "E2011",
            "expected `=` before initializer",
        )?;
        let value = self.expression()?;
        let end = self.expect(
            &TokenKind::Semicolon,
            "E2012",
            "expected `;` after variable declaration",
        )?;
        Some(Stmt::Let {
            name,
            name_span,
            ty,
            value,
            span: start.merge(end),
        })
    }

    fn assign_statement(&mut self) -> Option<Stmt> {
        let (name, name_span) = self.identifier("assignment target")?;
        self.expect(&TokenKind::Equal, "E2013", "expected `=` in assignment")?;
        let value = self.expression()?;
        let end = self.expect(
            &TokenKind::Semicolon,
            "E2014",
            "expected `;` after assignment",
        )?;
        Some(Stmt::Assign {
            name,
            name_span,
            value,
            span: name_span.merge(end),
        })
    }

    fn if_statement(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let condition = self.expression()?;
        let then_block = self.block()?;
        let else_block = if self.consume(&TokenKind::Else) {
            Some(self.block()?)
        } else {
            None
        };
        let end = else_block
            .as_ref()
            .map_or(then_block.span, |block| block.span);
        Some(Stmt::If {
            condition,
            then_block,
            else_block,
            span: start.merge(end),
        })
    }

    fn while_statement(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let condition = self.expression()?;
        let body = self.block()?;
        let span = start.merge(body.span);
        Some(Stmt::While {
            condition,
            body,
            span,
        })
    }

    fn return_statement(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let value = if self.at(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.expression()?)
        };
        let end = self.expect(&TokenKind::Semicolon, "E2015", "expected `;` after return")?;
        Some(Stmt::Return {
            value,
            span: start.merge(end),
        })
    }

    fn print_statement(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        self.expect(&TokenKind::LeftParen, "E2016", "expected `(` after `print`")?;
        let value = self.expression()?;
        self.expect(
            &TokenKind::RightParen,
            "E2017",
            "expected `)` after printed value",
        )?;
        let end = self.expect(&TokenKind::Semicolon, "E2018", "expected `;` after `print`")?;
        Some(Stmt::Print {
            value,
            span: start.merge(end),
        })
    }

    fn expression(&mut self) -> Option<Expr> {
        self.binary(0)
    }

    fn binary(&mut self, minimum_precedence: u8) -> Option<Expr> {
        let mut left = self.unary()?;
        while let Some((op, precedence)) = self.current_binary() {
            if precedence < minimum_precedence {
                break;
            }
            self.advance();
            let right = self.binary(precedence + 1)?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }
        Some(left)
    }

    fn unary(&mut self) -> Option<Expr> {
        if self.consume(&TokenKind::Minus) || self.consume(&TokenKind::Bang) {
            let token = self.previous().clone();
            let op = if token.kind == TokenKind::Minus {
                UnaryOp::Negate
            } else {
                UnaryOp::Not
            };
            let operand = self.unary()?;
            let span = token.span.merge(operand.span);
            return Some(Expr {
                kind: ExprKind::Unary {
                    op,
                    operand: Box::new(operand),
                },
                span,
            });
        }
        self.primary()
    }

    fn primary(&mut self) -> Option<Expr> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Integer(value) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Integer(value),
                    span: token.span,
                })
            }
            TokenKind::True | TokenKind::False => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Boolean(token.kind == TokenKind::True),
                    span: token.span,
                })
            }
            TokenKind::Identifier(name) => {
                self.advance();
                if self.consume(&TokenKind::LeftParen) {
                    let mut args = Vec::new();
                    if !self.at(&TokenKind::RightParen) {
                        loop {
                            args.push(self.expression()?);
                            if !self.consume(&TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    let end = self.expect(
                        &TokenKind::RightParen,
                        "E2019",
                        "expected `)` after arguments",
                    )?;
                    Some(Expr {
                        kind: ExprKind::Call {
                            callee: name,
                            callee_span: token.span,
                            args,
                        },
                        span: token.span.merge(end),
                    })
                } else {
                    Some(Expr {
                        kind: ExprKind::Variable(name),
                        span: token.span,
                    })
                }
            }
            TokenKind::LeftParen => {
                self.advance();
                let expression = self.expression()?;
                self.expect(
                    &TokenKind::RightParen,
                    "E2020",
                    "expected `)` after expression",
                )?;
                Some(expression)
            }
            _ => {
                self.error(
                    "E2021",
                    format!("expected expression, found {}", token.kind.description()),
                    token.span,
                );
                None
            }
        }
    }

    fn current_binary(&self) -> Option<(BinaryOp, u8)> {
        let value = match self.current().kind {
            TokenKind::OrOr => (BinaryOp::Or, 1),
            TokenKind::AndAnd => (BinaryOp::And, 2),
            TokenKind::EqualEqual => (BinaryOp::Equal, 3),
            TokenKind::BangEqual => (BinaryOp::NotEqual, 3),
            TokenKind::Less => (BinaryOp::Less, 4),
            TokenKind::LessEqual => (BinaryOp::LessEqual, 4),
            TokenKind::Greater => (BinaryOp::Greater, 4),
            TokenKind::GreaterEqual => (BinaryOp::GreaterEqual, 4),
            TokenKind::Plus => (BinaryOp::Add, 5),
            TokenKind::Minus => (BinaryOp::Subtract, 5),
            TokenKind::Star => (BinaryOp::Multiply, 6),
            TokenKind::Slash => (BinaryOp::Divide, 6),
            TokenKind::Percent => (BinaryOp::Remainder, 6),
            _ => return None,
        };
        Some(value)
    }

    fn identifier(&mut self, context: &str) -> Option<(String, Span)> {
        let token = self.current().clone();
        if let TokenKind::Identifier(name) = token.kind {
            self.advance();
            Some((name, token.span))
        } else {
            self.error(
                "E2022",
                format!("expected {context}, found {}", token.kind.description()),
                token.span,
            );
            None
        }
    }

    fn expect(&mut self, kind: &TokenKind, code: &'static str, message: &str) -> Option<Span> {
        if self.at(kind) {
            let span = self.current().span;
            self.advance();
            Some(span)
        } else {
            let token = self.current();
            self.error(code, message, token.span);
            None
        }
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span).with_label("parse stopped here"));
    }

    fn at(&self, kind: &TokenKind) -> bool {
        discriminant(&self.current().kind) == discriminant(kind)
    }

    fn peek_at(&self, kind: &TokenKind) -> bool {
        self.tokens
            .get(self.cursor + 1)
            .is_some_and(|token| discriminant(&token.kind) == discriminant(kind))
    }

    fn consume(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.cursor.saturating_sub(1)]
    }

    fn advance(&mut self) {
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
    }

    fn synchronize_statement(&mut self) {
        while !self.at(&TokenKind::Eof) && !self.at(&TokenKind::RightBrace) {
            if self.previous().kind == TokenKind::Semicolon {
                return;
            }
            if matches!(
                self.current().kind,
                TokenKind::Let
                    | TokenKind::If
                    | TokenKind::While
                    | TokenKind::Return
                    | TokenKind::Print
            ) {
                return;
            }
            self.advance();
        }
    }

    fn synchronize_top_level(&mut self) {
        while !self.at(&TokenKind::Eof) {
            if self.at(&TokenKind::Fn) {
                return;
            }
            self.advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;

    #[test]
    fn parses_control_flow_and_calls() {
        let tokens =
            lexer::lex("fn f(x: int) -> int { if x > 0 { return x; } else { return 0; } }").tokens;
        let result = parse(&tokens);
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.program.functions.len(), 1);
    }

    #[test]
    fn recovers_to_next_function() {
        let tokens =
            lexer::lex("fn broken( -> int { return 0; } fn good() -> int { return 1; }").tokens;
        let result = parse(&tokens);
        assert!(!result.diagnostics.is_empty());
        assert_eq!(result.program.functions.len(), 1);
        assert_eq!(result.program.functions[0].name, "good");
    }
}
