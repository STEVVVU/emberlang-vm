use crate::span::Span;
use ariadne::{CharSet, Config, IndexType, Label, LabelAttach, Report, ReportKind, Source};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Controls the presentation of a rendered diagnostic.
///
/// Presentation is separated from analysis so machine-checked output (snapshot
/// tests, piped stderr) never depends on the terminal, the platform, or the
/// active code page. `Plain` is the default for exactly that reason.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderStyle {
    /// ASCII gutters, no ANSI escapes. Byte-identical on every platform.
    #[default]
    Plain,
    /// Unicode gutters and ANSI colors, for an interactive terminal.
    Colored,
}

impl RenderStyle {
    fn config(self) -> Config {
        // `IndexType::Byte` matches `Span`, which stores byte offsets. Ariadne
        // still reports columns as character counts, so multi-byte source is
        // pointed at correctly without us re-scanning the line ourselves.
        //
        // `LabelAttach::Start` puts the arrow under the first column of the
        // span, matching where the previous hand-rolled renderer put its caret.
        let config = Config::default()
            .with_index_type(IndexType::Byte)
            .with_label_attach(LabelAttach::Start);
        match self {
            Self::Plain => config.with_color(false).with_char_set(CharSet::Ascii),
            Self::Colored => config.with_color(true).with_char_set(CharSet::Unicode),
        }
    }
}

/// A secondary source location that gives a diagnostic its context: the earlier
/// definition a redefinition collides with, the signature a call disagrees with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecondarySpan {
    pub span: Span,
    pub message: String,
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
    pub secondary: Vec<SecondarySpan>,
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
            secondary: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Attaches a second underlined location to the same diagnostic.
    #[must_use]
    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(SecondarySpan {
            span,
            message: message.into(),
        });
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

    /// Renders the diagnostic in the deterministic [`RenderStyle::Plain`] style.
    #[must_use]
    pub fn render(&self, filename: &str, source: &str) -> String {
        self.render_with(filename, source, RenderStyle::Plain)
    }

    /// Renders the diagnostic with an explicit presentation style.
    ///
    /// # Panics
    ///
    /// Panics only if writing into an in-memory `Vec<u8>` fails, which cannot
    /// happen short of allocation failure. Rendering never touches the terminal
    /// or the filesystem, so there is no I/O error for a caller to handle.
    #[must_use]
    pub fn render_with(&self, filename: &str, source: &str, style: RenderStyle) -> String {
        let primary = clamp(self.span, source);
        // `ReportKind::Custom` would let us choose the heading word, but it
        // carries its own `Color` and bypasses `Config::with_color(false)`,
        // emitting ANSI escapes unconditionally. The built-in kinds honour the
        // config, which is what keeps `Plain` output escape-free.
        let kind = match self.severity {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
        };

        // Ariadne only draws the underline for a label that carries a message,
        // so fall back to the severity word when a diagnostic has no label of
        // its own. Losing the underline would be a real regression: it is the
        // part of the render that shows *which characters* are at fault.
        let label_message = self.label.clone().unwrap_or_else(|| match self.severity {
            Severity::Error => "error".to_owned(),
            Severity::Warning => "warning".to_owned(),
        });
        // Ariadne folds labels into one source group only while their lines are
        // non-decreasing, and it derives each later group's header location from
        // the *primary* span. A secondary span earlier in the file would
        // therefore be split into a second group carrying the wrong `file:l:c`.
        // Sorting by offset and assigning an explicit render order keeps every
        // label in a single correctly-headed group.
        let mut labels: Vec<(std::ops::Range<usize>, &str, i32)> =
            vec![(primary.clone(), label_message.as_str(), 0)];
        for secondary in &self.secondary {
            // Priority -1 keeps the primary span's underline on top where the
            // two overlap; `order` below only controls grouping and layout.
            labels.push((
                clamp(secondary.span, source),
                secondary.message.as_str(),
                -1,
            ));
        }
        labels.sort_by_key(|(span, _, _)| (span.start, span.end));

        let mut report = Report::build(kind, (filename, primary))
            .with_config(style.config())
            .with_code(self.code)
            .with_message(&self.message);
        for (order, (span, message, priority)) in labels.into_iter().enumerate() {
            report = report.with_label(
                Label::new((filename, span))
                    .with_message(message)
                    .with_priority(priority)
                    .with_order(i32::try_from(order).unwrap_or(i32::MAX)),
            );
        }
        for note in &self.notes {
            report = report.with_note(note);
        }
        if let Some(help) = &self.help {
            report = report.with_help(help);
        }

        let mut buffer = Vec::new();
        report
            .finish()
            .write((filename, Source::from(source)), &mut buffer)
            .expect("writing a diagnostic into an in-memory buffer cannot fail");
        normalize(&String::from_utf8_lossy(&buffer))
    }
}

/// Clamps a span into `source` as a byte range ariadne will accept.
///
/// Ariadne panics on a reversed range and silently drops a label that points
/// past the end of the source. EOF diagnostics legitimately carry
/// `source.len()..source.len()`, so clamping rather than rejecting is correct.
fn clamp(span: Span, source: &str) -> std::ops::Range<usize> {
    let start = span.start.min(source.len());
    let end = span.end.min(source.len()).max(start);
    start..end
}

/// Normalizes line endings to `\n` and trims trailing padding, so rendered
/// output is stable across platforms, `core.autocrlf`, and snapshot review.
fn normalize(rendered: &str) -> String {
    let mut output = String::with_capacity(rendered.len());
    for line in rendered.lines() {
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output
}

#[must_use]
pub fn render_all(filename: &str, source: &str, diagnostics: &[Diagnostic]) -> String {
    render_all_with(filename, source, diagnostics, RenderStyle::Plain)
}

#[must_use]
pub fn render_all_with(
    filename: &str,
    source: &str,
    diagnostics: &[Diagnostic],
    style: RenderStyle,
) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.render_with(filename, source, style))
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
        assert!(rendered.contains("expected int"));
        assert!(rendered.contains("Help: use 1 instead"));
    }

    #[test]
    fn plain_style_has_no_ansi_or_unicode() {
        let source = "let x = true;\n";
        let rendered = Diagnostic::error("E1", "bad value", Span::new(8, 12))
            .with_label("expected int")
            .with_note("a note")
            .with_help("use 1 instead")
            .render("test.em", source);
        assert!(!rendered.contains('\u{1b}'), "ANSI escape in plain output");
        assert!(rendered.is_ascii(), "non-ASCII byte in plain output");
        assert!(
            !rendered.contains(" \n"),
            "trailing padding in plain output"
        );
        assert!(!rendered.contains('\r'), "carriage return in plain output");
    }

    #[test]
    fn colored_style_is_opt_in() {
        let source = "let x = true;\n";
        let rendered = Diagnostic::error("E1", "bad value", Span::new(8, 12)).render_with(
            "test.em",
            source,
            RenderStyle::Colored,
        );
        assert!(rendered.contains('\u{1b}'), "expected ANSI escapes");
    }

    #[test]
    fn clamps_spans_past_the_end_of_source() {
        let rendered = Diagnostic::error("E1", "unexpected end of file", Span::new(9, 9))
            .render("test.em", "let x = 1");
        assert!(rendered.contains("E1"));
        assert!(rendered.contains("unexpected end of file"));
    }
}
