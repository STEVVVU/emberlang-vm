use crate::diagnostic::Diagnostic;
use crate::span::Span;
use crate::token::{Token, TokenKind};

#[derive(Debug)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

#[must_use]
pub fn lex(source: &str) -> LexResult {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            cursor: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> LexResult {
        while self.cursor < self.bytes.len() {
            let start = self.cursor;
            let byte = self.advance();
            match byte {
                b' ' | b'\t' | b'\r' | b'\n' => {}
                b'0'..=b'9' => self.number(start),
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.identifier(start),
                b'(' => self.push(TokenKind::LeftParen, start),
                b')' => self.push(TokenKind::RightParen, start),
                b'{' => self.push(TokenKind::LeftBrace, start),
                b'}' => self.push(TokenKind::RightBrace, start),
                b':' => self.push(TokenKind::Colon, start),
                b';' => self.push(TokenKind::Semicolon, start),
                b',' => self.push(TokenKind::Comma, start),
                b'+' => self.push(TokenKind::Plus, start),
                b'*' => self.push(TokenKind::Star, start),
                b'%' => self.push(TokenKind::Percent, start),
                b'-' if self.matches(b'>') => self.push(TokenKind::Arrow, start),
                b'-' => self.push(TokenKind::Minus, start),
                b'=' if self.matches(b'=') => self.push(TokenKind::EqualEqual, start),
                b'=' => self.push(TokenKind::Equal, start),
                b'!' if self.matches(b'=') => self.push(TokenKind::BangEqual, start),
                b'!' => self.push(TokenKind::Bang, start),
                b'<' if self.matches(b'=') => self.push(TokenKind::LessEqual, start),
                b'<' => self.push(TokenKind::Less, start),
                b'>' if self.matches(b'=') => self.push(TokenKind::GreaterEqual, start),
                b'>' => self.push(TokenKind::Greater, start),
                b'&' if self.matches(b'&') => self.push(TokenKind::AndAnd, start),
                b'|' if self.matches(b'|') => self.push(TokenKind::OrOr, start),
                b'/' if self.matches(b'/') => self.line_comment(),
                b'/' if self.matches(b'*') => self.block_comment(start),
                b'/' => self.push(TokenKind::Slash, start),
                _ => {
                    let end = self.cursor;
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1001",
                            format!("unexpected character `{}`", &self.source[start..end]),
                            Span::new(start, end),
                        )
                        .with_help("remove the character or replace it with a valid token"),
                    );
                }
            }
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.cursor, self.cursor),
        });
        LexResult {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn advance(&mut self) -> u8 {
        let byte = self.bytes[self.cursor];
        self.cursor += 1;
        byte
    }

    fn matches(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.cursor) == Some(&expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.cursor),
        });
    }

    fn number(&mut self, start: usize) {
        while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
            self.cursor += 1;
        }
        let text = &self.source[start..self.cursor];
        match text.parse::<i64>() {
            Ok(value) => self.push(TokenKind::Integer(value), start),
            Err(_) => self.diagnostics.push(
                Diagnostic::error(
                    "E1002",
                    "integer literal does not fit in signed 64-bit range",
                    Span::new(start, self.cursor),
                )
                .with_help("use a value between -9223372036854775808 and 9223372036854775807"),
            ),
        }
    }

    fn identifier(&mut self, start: usize) {
        while self
            .bytes
            .get(self.cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.cursor += 1;
        }
        let text = &self.source[start..self.cursor];
        let kind = match text {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "return" => TokenKind::Return,
            "print" => TokenKind::Print,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "int" => TokenKind::IntType,
            "bool" => TokenKind::BoolType,
            "unit" => TokenKind::UnitType,
            _ => TokenKind::Identifier(text.to_owned()),
        };
        self.push(kind, start);
    }

    fn line_comment(&mut self) {
        while self.cursor < self.bytes.len() && self.bytes[self.cursor] != b'\n' {
            self.cursor += 1;
        }
    }

    fn block_comment(&mut self, start: usize) {
        while self.cursor + 1 < self.bytes.len() {
            if self.bytes[self.cursor] == b'*' && self.bytes[self.cursor + 1] == b'/' {
                self.cursor += 2;
                return;
            }
            self.cursor += 1;
        }
        self.cursor = self.bytes.len();
        self.diagnostics.push(
            Diagnostic::error(
                "E1003",
                "unterminated block comment",
                Span::new(start, self.cursor),
            )
            .with_help("close the comment with `*/`"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_keywords_operators_and_comments() {
        let result = lex("fn main() -> int { /* x */ return 2 <= 3 && true; }");
        assert!(result.diagnostics.is_empty());
        assert!(result
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::Fn));
        assert!(result
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::LessEqual));
    }

    #[test]
    fn recovers_after_unknown_character() {
        let result = lex("@ let x: int = 1;");
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::Let));
    }
}
