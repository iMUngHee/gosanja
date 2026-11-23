//! Version compatibility between a writer and a reader of either interface.
//!
//! The rules are stated in `spec/README.md`, "What a reader must do".

use std::fmt;
use std::str::FromStr;

/// A `major.minor` interface version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Incremented by a breaking change.
    pub major: u32,
    /// Incremented by an additive change.
    pub minor: u32,
}

impl Version {
    /// Builds a version from its two numbers.
    #[must_use]
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// A string that is not `major.minor` with two non-negative integers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidVersion(String);

impl fmt::Display for InvalidVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "version must be major.minor, got {:?}", self.0)
    }
}

impl std::error::Error for InvalidVersion {}

impl FromStr for Version {
    type Err = InvalidVersion;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidVersion(s.to_owned());
        let (major, minor) = s.split_once('.').ok_or_else(invalid)?;
        let number = |part: &str| -> Result<u32, InvalidVersion> {
            // `u32::from_str` accepts a leading `+`, which the specification does not.
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            part.parse().map_err(|_overflow| invalid())
        };
        Ok(Self {
            major: number(major)?,
            minor: number(minor)?,
        })
    }
}

/// How much of a writer's output a reader understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    /// Every row the writer can produce is known to the reader.
    Full,
    /// The writer is a newer minor: rows holding a value the reader does not know are skipped
    /// and counted, together with the rows that depend on them.
    SkipUnknown,
}

/// Why a reader refuses a writer's output before doing any work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The major versions differ.
    Major {
        /// Version the reader implements.
        reader: Version,
        /// Version the output was written with.
        writer: Version,
    },
    /// The major is 0, where a minor change may break, and the minors differ.
    UnstableMinor {
        /// Version the reader implements.
        reader: Version,
        /// Version the output was written with.
        writer: Version,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Major { reader, writer } => {
                write!(
                    f,
                    "written with version {writer}, this reader implements {reader}"
                )
            }
            Self::UnstableMinor { reader, writer } => write!(
                f,
                "written with version {writer}, this reader implements {reader}; \
                 0.x versions are only compatible with themselves"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// Decides whether a reader may read what a writer produced.
///
/// # Errors
///
/// Returns a [`Refusal`] when the majors differ, or when the major is 0 and the minors differ.
pub fn accept(reader: Version, writer: Version) -> Result<Acceptance, Refusal> {
    if reader.major != writer.major {
        return Err(Refusal::Major { reader, writer });
    }
    if reader.major == 0 {
        return if reader.minor == writer.minor {
            Ok(Acceptance::Full)
        } else {
            Err(Refusal::UnstableMinor { reader, writer })
        };
    }
    Ok(if writer.minor <= reader.minor {
        Acceptance::Full
    } else {
        Acceptance::SkipUnknown
    })
}

/// What a reader does with one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowVerdict {
    /// The row is understood.
    Keep,
    /// The row holds a value from a newer minor: skip it and count it.
    Skip,
    /// The row is broken; the interface decides what follows.
    Malformed,
}

/// Classifies a row from whether it is otherwise well-formed and whether it holds a value in a
/// closed enumeration that this reader does not know.
///
/// The unknown-value rule comes first: under a newer writer minor such a row is skipped even
/// though it fails the reader's schema.
#[must_use]
pub const fn classify_row(
    acceptance: Acceptance,
    well_formed: bool,
    unknown_value: bool,
) -> RowVerdict {
    match (well_formed, unknown_value, acceptance) {
        (false, _, _) | (true, true, Acceptance::Full) => RowVerdict::Malformed,
        (true, true, Acceptance::SkipUnknown) => RowVerdict::Skip,
        (true, false, _) => RowVerdict::Keep,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        text.parse().expect("test versions are well-formed")
    }

    #[test]
    fn parses_and_prints_major_minor() {
        assert_eq!(v("0.1"), Version::new(0, 1));
        assert_eq!(v("12.340").to_string(), "12.340");
    }

    #[test]
    fn rejects_anything_but_two_plain_integers() {
        for text in [
            "",
            "1",
            "1.",
            ".1",
            "1.2.3",
            "a.1",
            "1.b",
            "+1.2",
            "1.-2",
            " 1.2",
            "4294967296.0",
        ] {
            assert!(text.parse::<Version>().is_err(), "{text:?} must not parse");
        }
    }

    #[test]
    fn acceptance_table() {
        use Acceptance::{Full, SkipUnknown};
        let cases: &[(&str, &str, Result<Acceptance, ()>)] = &[
            // reader, writer
            ("0.1", "0.1", Ok(Full)),
            ("0.1", "0.2", Err(())),
            ("0.2", "0.1", Err(())),
            ("0.1", "1.0", Err(())),
            ("1.0", "0.9", Err(())),
            ("1.2", "1.2", Ok(Full)),
            ("1.2", "1.0", Ok(Full)),
            ("1.2", "1.3", Ok(SkipUnknown)),
            ("1.2", "2.0", Err(())),
            ("2.0", "1.9", Err(())),
        ];
        for (reader, writer, expected) in cases {
            let got = accept(v(reader), v(writer)).map_err(|_refusal| ());
            assert_eq!(got, *expected, "reader {reader}, writer {writer}");
        }
    }

    #[test]
    fn refusal_names_the_kind() {
        assert!(matches!(
            accept(v("0.1"), v("0.2")),
            Err(Refusal::UnstableMinor { .. })
        ));
        assert!(matches!(
            accept(v("1.0"), v("2.0")),
            Err(Refusal::Major { .. })
        ));
        assert!(matches!(
            accept(v("0.1"), v("1.1")),
            Err(Refusal::Major { .. })
        ));
    }

    #[test]
    fn unknown_value_rule_precedes_malformed_only_for_a_newer_minor() {
        use Acceptance::{Full, SkipUnknown};
        use RowVerdict::{Keep, Malformed, Skip};
        assert_eq!(classify_row(Full, true, false), Keep);
        assert_eq!(classify_row(SkipUnknown, true, false), Keep);
        assert_eq!(classify_row(SkipUnknown, true, true), Skip);
        assert_eq!(classify_row(Full, true, true), Malformed);
        assert_eq!(classify_row(Full, false, false), Malformed);
        assert_eq!(classify_row(SkipUnknown, false, true), Malformed);
    }
}
