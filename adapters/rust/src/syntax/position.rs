//! Positions: syn spans as character ranges, and the index's byte columns converted to them.

use gosanja_schema::position::Range;
use proc_macro2::Span;

/// The range of a span: zero-based lines, columns in characters.
pub fn range(span: Span) -> Range {
    let (start, end) = (span.start(), span.end());
    Range::new(
        line(start.line),
        col(start.column),
        line(end.line),
        col(end.column),
    )
}

/// From the start of `first` to the end of `last`.
pub fn between(first: Span, last: Span) -> Range {
    let (start, end) = (range(first), range(last));
    Range::new(start.0[0], start.0[1], end.0[2], end.0[3])
}

fn line(one_based: usize) -> u32 {
    u32::try_from(one_based.saturating_sub(1)).unwrap_or(u32::MAX)
}

fn col(column: usize) -> u32 {
    u32::try_from(column).unwrap_or(u32::MAX)
}

/// The lines of a file as the engine counts them: split at `\n`, so that the position at
/// column 0 just after a final line break lies on an empty last line.
#[derive(Debug)]
pub struct Lines<'a> {
    lines: Vec<&'a str>,
}

impl<'a> Lines<'a> {
    /// Indexes `text`.
    pub fn new(text: &'a str) -> Self {
        Self {
            lines: text.split('\n').collect(),
        }
    }

    /// The character column of byte column `bytes` on `line`, when it is the start of a
    /// character or the end of the line.
    pub fn char_column(&self, line: u32, bytes: u32) -> Option<u32> {
        let text = self.lines.get(usize::try_from(line).ok()?)?;
        let bytes = usize::try_from(bytes).ok()?;
        let prefix = text.get(..bytes)?;
        u32::try_from(prefix.chars().count()).ok()
    }

    /// Converts SCIP columns to scalar offsets after checking both endpoints.
    pub fn scip_range(
        &self,
        value: Range,
        encoding: scip::types::PositionEncoding,
    ) -> Option<Range> {
        use scip::types::PositionEncoding::{
            UTF8CodeUnitOffsetFromLineStart as Utf8, UTF16CodeUnitOffsetFromLineStart as Utf16,
            UTF32CodeUnitOffsetFromLineStart as Utf32,
        };
        if encoding == Utf8 {
            return self.char_range(value);
        }
        let column = |line: u32, units: u32| {
            let line = self.lines.get(line as usize)?;
            let mut remaining = usize::try_from(units).ok()?;
            let mut count = 0;
            for ch in line.chars() {
                if remaining == 0 {
                    break;
                }
                remaining = remaining.checked_sub(match encoding {
                    Utf16 => ch.len_utf16(),
                    Utf32 => 1,
                    _ => return None,
                })?;
                count += 1;
            }
            (remaining == 0).then_some(count)
        };
        let [sl, sc, el, ec] = value.0;
        Some(Range::new(sl, column(sl, sc)?, el, column(el, ec)?))
    }

    /// A byte range converted to characters.
    pub fn char_range(&self, bytes: Range) -> Option<Range> {
        let [sl, sc, el, ec] = bytes.0;
        Some(Range::new(
            sl,
            self.char_column(sl, sc)?,
            el,
            self.char_column(el, ec)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_columns_convert_on_existing_positions_only() {
        let lines = Lines::new("aé b\nx\n");
        assert_eq!(lines.char_column(0, 3), Some(2));
        assert_eq!(lines.char_column(0, 2), None, "inside é");
        assert_eq!(lines.char_column(0, 5), Some(4), "end of the line");
        assert_eq!(lines.char_column(0, 6), None, "past the end");
        assert_eq!(lines.char_column(1, 1), Some(1));
        // Column 0 after the final line break exists; nothing after it does.
        assert_eq!(lines.char_column(2, 0), Some(0));
        assert_eq!(lines.char_column(2, 1), None);
        assert_eq!(lines.char_column(3, 0), None);
        assert_eq!(
            lines.char_range(Range::new(0, 1, 2, 0)),
            Some(Range::new(0, 1, 2, 0))
        );
        assert_eq!(lines.char_range(Range::new(0, 2, 1, 0)), None);
    }
}
