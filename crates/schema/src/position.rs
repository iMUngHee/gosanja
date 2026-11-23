//! Positions shared by the snapshot and the adapter contract.

use serde::{Deserialize, Serialize};

/// `[startLine, startCol, endLine, endCol]`, zero-based, end exclusive.
///
/// In a snapshot the columns count UTF-8 bytes. In adapter output they count the unit the
/// adapter declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Range(pub [u32; 4]);

impl Range {
    /// Builds a range from its four coordinates.
    #[must_use]
    pub const fn new(start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> Self {
        Self([start_line, start_col, end_line, end_col])
    }

    /// Line and column where the range starts.
    #[must_use]
    pub const fn start(self) -> (u32, u32) {
        (self.0[0], self.0[1])
    }

    /// Line and column just past the end of the range.
    #[must_use]
    pub const fn end(self) -> (u32, u32) {
        (self.0[2], self.0[3])
    }

    /// Whether `inner` lies within this range, ends included.
    #[must_use]
    pub fn contains(self, inner: Self) -> bool {
        self.start() <= inner.start() && inner.end() <= self.end()
    }

    /// Whether the two ranges share a position without one containing the other.
    #[must_use]
    pub fn overlaps_without_nesting(self, other: Self) -> bool {
        let disjoint = self.end() <= other.start() || other.end() <= self.start();
        !disjoint && !self.contains(other) && !other.contains(self)
    }
}

/// A range in a file: `[file, startLine, startCol, endLine, endCol]`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Site(pub String, pub u32, pub u32, pub u32, pub u32);

impl Site {
    /// Builds a site from a file path and a range.
    #[must_use]
    pub fn new(file: impl Into<String>, range: Range) -> Self {
        let [start_line, start_col, end_line, end_col] = range.0;
        Self(file.into(), start_line, start_col, end_line, end_col)
    }

    /// The file the site is in.
    #[must_use]
    pub fn file(&self) -> &str {
        &self.0
    }

    /// The range within the file.
    #[must_use]
    pub const fn range(&self) -> Range {
        Range([self.1, self.2, self.3, self.4])
    }
}

/// The unit in which a producer counts columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PositionEncoding {
    /// Columns count UTF-8 bytes.
    #[serde(rename = "utf-8")]
    Utf8,
    /// Columns count UTF-16 code units.
    #[serde(rename = "utf-16")]
    Utf16,
    /// Columns count Unicode scalar values.
    #[serde(rename = "utf-32")]
    Utf32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_and_site_serialise_as_flat_arrays() {
        let range = Range::new(2, 7, 2, 13);
        assert_eq!(
            serde_json::to_string(&range).expect("serialises"),
            "[2,7,2,13]"
        );
        let site = Site::new("src/views.rs", range);
        assert_eq!(
            serde_json::to_string(&site).expect("serialises"),
            r#"["src/views.rs",2,7,2,13]"#
        );
        assert_eq!(site.range(), range);
    }

    #[test]
    fn containment_includes_both_ends() {
        let body = Range::new(2, 0, 8, 1);
        assert!(body.contains(Range::new(2, 0, 8, 1)));
        assert!(body.contains(Range::new(4, 8, 4, 13)));
        assert!(!body.contains(Range::new(8, 0, 8, 2)));
        assert!(!body.contains(Range::new(1, 9, 2, 3)));
    }

    #[test]
    fn overlap_without_nesting_excludes_nested_and_disjoint_ranges() {
        let a = Range::new(1, 0, 5, 0);
        assert!(a.overlaps_without_nesting(Range::new(3, 0, 7, 0)));
        assert!(!a.overlaps_without_nesting(Range::new(2, 0, 3, 0)));
        assert!(!a.overlaps_without_nesting(Range::new(5, 0, 6, 0)));
        assert!(!a.overlaps_without_nesting(a));
    }
}
