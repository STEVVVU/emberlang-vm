use crate::span::{line_bounds, location, Span};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub label: Option<String>,
    pub help: Option<String>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
            label: None,
            help: None,
            notes: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    #[must_use]
    pub fn render(&self, filename: &str, source: &str) -> String {
        let mut output = String::new();
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let loc = location(source, self.span.start);
        let (line_start, line_end) = line_bounds(source, self.span.start);
        let line_text = &source[line_start..line_end];
        let marker_start = source[line_start..self.span.start.min(line_end)]
            .chars()
            .count();
        let marker_len = source[self.span.start.min(line_end)..self.span.end.min(line_end)]
            .chars()
            .count()
            .max(1);
        let gutter_width = loc.line.to_string().len();

        let _ = writeln!(
            output,
            "{level}[{}]: {}\n --> {filename}:{}:{}",
            self.code, self.message, loc.line, loc.column
        );
        let _ = writeln!(output, "{:>gutter_width$} |", "");
        let _ = writeln!(output, "{:>gutter_width$} | {line_text}", loc.line);
        let _ = write!(
            output,
            "{:>gutter_width$} | {}{}",
            "",
            " ".repeat(marker_start),
            "^".repeat(marker_len)
        );
        if let Some(label) = &self.label {
            let _ = write!(output, " {label}");
        }
        output.push('\n');
        for note in &self.notes {
            let _ = writeln!(output, " note: {note}");
        }
        if let Some(help) = &self.help {
            let _ = writeln!(output, " help: {help}");
        }
        output
    }
}

#[must_use]
pub fn render_all(filename: &str, source: &str, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.render(filename, source))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_location_and_help() {
        let source = "let x = true;\n";
        let rendered = Diagnostic::error("E1", "bad value", Span::new(8, 12))
            .with_label("expected int")
            .with_help("use 1 instead")
            .render("test.em", source);
        assert!(rendered.contains("test.em:1:9"));
        assert!(rendered.contains("^^^^ expected int"));
        assert!(rendered.contains("help: use 1 instead"));
    }
}
