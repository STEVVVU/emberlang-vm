use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self::new(
            if self.start < other.start {
                self.start
            } else {
                other.start
            },
            if self.end > other.end {
                self.end
            } else {
                other.end
            },
        )
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

#[must_use]
pub fn location(source: &str, offset: usize) -> Location {
    let safe_offset = offset.min(source.len());
    let prefix = &source[..safe_offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.chars().count() + 1, |(_, tail)| {
            tail.chars().count() + 1
        });
    Location { line, column }
}

#[must_use]
pub fn line_bounds(source: &str, offset: usize) -> (usize, usize) {
    let safe_offset = offset.min(source.len());
    let start = source[..safe_offset]
        .rfind('\n')
        .map_or(0, |position| position + 1);
    let end = source[safe_offset..]
        .find('\n')
        .map_or(source.len(), |position| safe_offset + position);
    (start, end)
}
