//! Reads a SCIP index one top-level field at a time.
//!
//! An `Index` is a sequence of fields, so several concatenated `Index` messages, which
//! scip-go and scip-typescript write, read as one longer sequence. Each document is decoded
//! on its own, and unknown top-level fields are skipped.

use std::fmt;
use std::io::Read;

use protobuf::CodedInputStream;
use protobuf::rt::WireType;
use scip::types::{Document, Metadata, SymbolInformation};

/// One top-level field of an index.
#[derive(Debug)]
pub enum Item {
    /// `Index.metadata`; there is one per concatenated message that carries it.
    Metadata(Box<Metadata>),
    /// `Index.documents`.
    Document(Box<Document>),
    /// `Index.external_symbols`.
    ExternalSymbol(Box<SymbolInformation>),
}

/// The index could not be decoded.
#[derive(Debug)]
pub enum DecodeError {
    /// The protobuf decoder refused the bytes.
    Protobuf(protobuf::Error),
    /// A top-level tag names a wire type that protobuf does not define.
    WireType(u32),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protobuf(e) => write!(f, "the SCIP index cannot be decoded: {e}"),
            Self::WireType(tag) => write!(f, "the SCIP index has an invalid tag {tag}"),
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Protobuf(e) => Some(e),
            Self::WireType(_) => None,
        }
    }
}

const METADATA: u32 = 1;
const DOCUMENTS: u32 = 2;
const EXTERNAL_SYMBOLS: u32 = 3;

/// Calls `visit` with every top-level item of the index, in file order.
///
/// # Errors
///
/// Returns a [`DecodeError`] when the bytes are not a valid SCIP index.
pub fn for_each_item(input: &mut dyn Read, mut visit: impl FnMut(Item)) -> Result<(), DecodeError> {
    let mut stream = CodedInputStream::new(input);
    while let Some(tag) = stream
        .read_raw_tag_or_eof()
        .map_err(DecodeError::Protobuf)?
    {
        let field = tag >> 3;
        let wire = WireType::new(tag & 7).ok_or(DecodeError::WireType(tag))?;
        let item = match (field, wire) {
            (METADATA, WireType::LengthDelimited) => Item::Metadata(Box::new(
                stream.read_message().map_err(DecodeError::Protobuf)?,
            )),
            (DOCUMENTS, WireType::LengthDelimited) => Item::Document(Box::new(
                stream.read_message().map_err(DecodeError::Protobuf)?,
            )),
            (EXTERNAL_SYMBOLS, WireType::LengthDelimited) => Item::ExternalSymbol(Box::new(
                stream.read_message().map_err(DecodeError::Protobuf)?,
            )),
            (_, wire) => {
                stream.skip_field(wire).map_err(DecodeError::Protobuf)?;
                continue;
            }
        };
        visit(item);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use protobuf::Message;
    use scip::types::{Index, ToolInfo};

    use super::*;

    fn index(tool: &str, path: &str) -> Vec<u8> {
        let mut index = Index::new();
        let mut metadata = Metadata::new();
        let mut info = ToolInfo::new();
        info.name = tool.to_owned();
        metadata.tool_info = Some(info).into();
        index.metadata = Some(metadata).into();
        let mut document = Document::new();
        document.relative_path = path.to_owned();
        index.documents.push(document);
        index.write_to_bytes().expect("encodes")
    }

    fn items(bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for_each_item(&mut &*bytes, |item| {
            out.push(match item {
                Item::Metadata(m) => format!("metadata {}", m.tool_info.name),
                Item::Document(d) => format!("document {}", d.relative_path),
                Item::ExternalSymbol(s) => format!("external {}", s.symbol),
            });
        })
        .expect("decodes");
        out
    }

    #[test]
    fn concatenated_messages_read_as_one_sequence() {
        let mut bytes = index("a", "x.rs");
        bytes.extend(index("b", "y.rs"));
        assert_eq!(
            items(&bytes),
            ["metadata a", "document x.rs", "metadata b", "document y.rs"]
        );
    }

    #[test]
    fn errors_name_their_cause() {
        use std::error::Error as _;
        let truncated = &index("a", "x.rs")[..5];
        let error = for_each_item(&mut &*truncated, |_| {}).unwrap_err();
        assert!(error.source().is_some());
        assert!(error.to_string().contains("cannot be decoded"), "{error}");
        // Field 1 with wire type 7, which protobuf does not define.
        let error = for_each_item(&mut &[(1 << 3) | 7][..], |_| {}).unwrap_err();
        assert!(error.source().is_none());
        assert!(error.to_string().contains("invalid tag 15"), "{error}");
    }

    #[test]
    fn unknown_fields_are_skipped_and_garbage_is_refused() {
        // Field 9 with wire type 0 (varint) and the value 5.
        let mut bytes = vec![9 << 3, 5];
        bytes.extend(index("a", "x.rs"));
        assert_eq!(items(&bytes), ["metadata a", "document x.rs"]);
        let truncated = &index("a", "x.rs")[..5];
        assert!(for_each_item(&mut &*truncated, |_| {}).is_err());
    }
}
