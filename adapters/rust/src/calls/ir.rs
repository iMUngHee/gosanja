//! The lines of a textual LLVM module that the calls layer understands.
//!
//! Function definitions and declarations, call and invoke instructions, metadata nodes of a
//! few kinds and the target triple. Everything else is passed over by its kind.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead as _, BufReader, Seek as _};
use std::path::Path;

/// A line this reader understands that it could not follow, or a module that ends inside a
/// function. The layer fails with `ir-unreadable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The 1-based line of the module, 0 for the module as a whole.
    pub line: usize,
    pub what: String,
}

impl Unreadable {
    pub fn new(line: usize, what: impl Into<String>) -> Self {
        Self {
            line,
            what: what.into(),
        }
    }
}

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.what)
        } else {
            write!(f, "line {}: {}", self.line, self.what)
        }
    }
}

/// The metadata kinds the reader understands; a node of another kind is passed over.
pub const KINDS: [&str; 6] = [
    "DISubprogram",
    "DILocation",
    "DILexicalBlock",
    "DILexicalBlockFile",
    "DIFile",
    "DICompileUnit",
];

/// The fields the calls layer reads from those nodes. A node's other fields are parsed, so
/// that a malformed one still fails the module, and then left out: a module holds a node for
/// every call and most of a node's text is fields nothing asks for.
const KEPT: [&str; 12] = [
    "checksum",
    "checksumkind",
    "column",
    "directory",
    "file",
    "filename",
    "inlinedAt",
    "isOptimized",
    "line",
    "name",
    "producer",
    "scope",
];

/// The kept fields of a node, by name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fields(Vec<(&'static str, Value)>);

impl Fields {
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        debug_assert!(KEPT.contains(&key), "the reader does not keep {key}");
        self.0
            .iter()
            .find_map(|(name, value)| (*name == key).then_some(value))
    }
}

/// A field value of a metadata node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// `!N`.
    Ref(u32),
    /// `null`.
    Null,
    /// A string, decoded.
    Str(String),
    /// Anything else, as written: a number, a flag set, a keyword.
    Raw(String),
}

/// A metadata node of one of the understood kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: &'static str,
    pub fields: Fields,
    /// The 1-based line it was written on.
    pub line: usize,
}

/// What a call instruction calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A global name, `@symbol`.
    Direct(String),
    /// A local value, `%name`.
    Indirect,
}

/// A call or invoke instruction that calls code of the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub target: Target,
    /// The node of its `!dbg`.
    pub dbg: Option<u32>,
    /// The 1-based line of the instruction.
    pub line: usize,
}

/// A defined function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub symbol: String,
    /// The node of its `!dbg`.
    pub dbg: Option<u32>,
    pub calls: Vec<Call>,
    /// The 1-based line of its `define`.
    pub line: usize,
}

/// The understood content of one module.
#[derive(Debug, Default)]
pub struct Text {
    pub functions: Vec<Function>,
    pub declared: BTreeSet<String>,
    pub nodes: BTreeMap<u32, Node>,
    pub triple: Option<String>,
}

/// Reads a module. The metadata is read in a first pass over the text and the functions in a
/// second, as in a module's text the metadata follows the functions.
///
/// # Errors
/// When an understood line does not parse, a node is defined twice, or the text ends inside
/// a function.
pub fn read(text: &str) -> Result<Text, Unreadable> {
    passes(|| Ok(text.lines().map(Ok)))
}

/// Reads a module from a file as [`read`] does, a line at a time, so that neither pass holds
/// the module's text.
///
/// # Errors
/// As [`read`], and when the file cannot be read or is not UTF-8.
pub fn read_file(path: &Path) -> Result<Text, Unreadable> {
    // One open file for both passes, so that both read the same module.
    let file = File::open(path).map_err(|error| Unreadable::new(0, error.to_string()))?;
    passes(|| {
        (&file).rewind()?;
        Ok(BufReader::new(&file).lines())
    })
}

/// Reads a module from its lines, which `pass` gives from the start each time it is called.
fn passes<L: AsRef<str>, I: Iterator<Item = io::Result<L>>>(
    pass: impl Fn() -> io::Result<I>,
) -> Result<Text, Unreadable> {
    let whole = |error: io::Error| Unreadable::new(0, error.to_string());
    let at = |n: usize| move |error: io::Error| Unreadable::new(n, error.to_string());
    let mut out = Text::default();
    for (n, line) in pass().map_err(whole)?.enumerate() {
        let n = n + 1;
        let line = line.map_err(at(n))?;
        let line = line.as_ref();
        if line.starts_with('!') {
            if let Some((id, node)) = metadata(line, n)?
                && out.nodes.insert(id, node).is_some()
            {
                return Err(Unreadable::new(n, format!("node !{id} defined twice")));
            }
        } else if let Some(rest) = line.strip_prefix("target triple = ") {
            let (value, tail) = string(rest).ok_or_else(|| bad(n, "target triple"))?;
            if !tail.trim().is_empty() || out.triple.is_some() {
                return Err(bad(n, "target triple"));
            }
            out.triple = Some(value);
        }
    }
    let mut lines = pass()
        .map_err(whole)?
        .enumerate()
        .map(|(n, line)| line.map(|line| (n + 1, line)).map_err(at(n + 1)));
    while let Some(next) = lines.next() {
        let (n, line) = next?;
        let line = line.as_ref();
        if line.starts_with("define ") {
            out.functions.push(function(line, n, &mut lines)?);
        } else if line.starts_with("declare ") {
            out.declared.insert(symbol_after(line, "declare ", n)?);
        }
    }
    Ok(out)
}

fn bad(line: usize, what: &str) -> Unreadable {
    Unreadable::new(line, format!("a {what} line that does not parse"))
}

fn function<L: AsRef<str>>(
    line: &str,
    n: usize,
    lines: &mut impl Iterator<Item = Result<(usize, L), Unreadable>>,
) -> Result<Function, Unreadable> {
    let head = line
        .trim_end()
        .strip_suffix('{')
        .ok_or_else(|| bad(n, "define"))?;
    let mut f = Function {
        symbol: symbol_after(head, "define ", n)?,
        dbg: dbg(head, n)?,
        calls: Vec::new(),
        line: n,
    };
    while let Some(next) = lines.next() {
        let (m, body) = next?;
        let body = body.as_ref();
        if body == "}" {
            return Ok(f);
        }
        let Some((keyword, rest)) = instruction(body) else {
            continue;
        };
        let mut text = rest.to_owned();
        if keyword == "invoke" {
            let (_, next) = lines
                .next()
                .ok_or_else(|| Unreadable::new(0, "the module ends inside a function"))??;
            let next = next.as_ref();
            if !next.trim_start().starts_with("to label ") {
                return Err(bad(m, "invoke"));
            }
            text.push(' ');
            text.push_str(next.trim_start());
        }
        if let Some(target) = callee(&text, m)? {
            f.calls.push(Call {
                target,
                dbg: dbg(&text, m)?,
                line: m,
            });
        }
    }
    Err(Unreadable::new(0, "the module ends inside a function"))
}

/// The keyword (`call` or `invoke`) and the text after it, when `line` is such an
/// instruction.
fn instruction(line: &str) -> Option<(&'static str, &str)> {
    let mut rest = line.trim_start();
    if rest.starts_with('%') {
        let (_, after) = rest.split_once(" = ")?;
        rest = after;
    }
    for prefix in ["tail ", "musttail ", "notail "] {
        if let Some(after) = rest.strip_prefix(prefix) {
            rest = after;
            break;
        }
    }
    if let Some(after) = rest.strip_prefix("call ") {
        Some(("call", after))
    } else {
        rest.strip_prefix("invoke ").map(|after| ("invoke", after))
    }
}

/// What the instruction calls: the operand before its argument list, which is the last
/// parenthesised group at the top level. `None` for inline assembly.
fn callee(text: &str, n: usize) -> Result<Option<Target>, Unreadable> {
    let groups = top_level(text).ok_or_else(|| bad(n, "call"))?;
    let open = groups
        .iter()
        .filter(|g| g.0 == '(')
        .map(|g| g.1)
        .next_back()
        .ok_or_else(|| bad(n, "call"))?;
    let head = &text[..open];
    if head.split_whitespace().any(|w| w == "asm") {
        return Ok(None);
    }
    let operand = last_token(head).ok_or_else(|| bad(n, "call"))?;
    if let Some(name) = operand.strip_prefix('@') {
        Ok(Some(Target::Direct(
            global(name).ok_or_else(|| bad(n, "call"))?,
        )))
    } else if operand.starts_with('%') {
        Ok(Some(Target::Indirect))
    } else {
        Err(bad(n, "call"))
    }
}

/// The last whitespace-separated token of `head`, a quoted name counted as one token.
fn last_token(head: &str) -> Option<&str> {
    let head = head.trim_end();
    if let Some(body) = head.strip_suffix('"') {
        // A quoted name holds no raw quote, so the opening one is the last in `body`.
        let start = body.rfind('"')?;
        let sigil = start.checked_sub(1)?;
        return matches!(head.get(sigil..start), Some("@" | "%")).then(|| &head[sigil..]);
    }
    head.rsplit(char::is_whitespace)
        .next()
        .filter(|t| !t.is_empty())
}

/// The name of a global after its `@`: bare, or a quoted string.
fn global(name: &str) -> Option<String> {
    if name.starts_with('"') {
        let (value, tail) = string(name)?;
        return tail.is_empty().then_some(value);
    }
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '$' | '.' | '_'));
    ok.then(|| name.to_owned())
}

/// The symbol of a `define` or `declare` line: its first global name at the top level.
fn symbol_after(line: &str, keyword: &str, n: usize) -> Result<String, Unreadable> {
    let what = keyword.trim_end();
    let rest = &line[keyword.len()..];
    let groups = top_level(rest).ok_or_else(|| bad(n, what))?;
    let at = rest
        .char_indices()
        .find(|&(i, c)| c == '@' && !inside(&groups, i))
        .map(|(i, _)| i)
        .ok_or_else(|| bad(n, what))?;
    let tail = &rest[at + 1..];
    let end = if tail.starts_with('"') {
        string(tail).map(|(_, t)| tail.len() - t.len())
    } else {
        tail.find('(')
    }
    .ok_or_else(|| bad(n, what))?;
    global(&tail[..end]).ok_or_else(|| bad(n, what))
}

fn inside(groups: &[(char, usize, usize)], i: usize) -> bool {
    groups.iter().any(|&(_, open, close)| open < i && i < close)
}

/// The node of the `!dbg` attachment in `text`, if any. The attachment is outside every
/// bracket.
fn dbg(text: &str, n: usize) -> Result<Option<u32>, Unreadable> {
    let groups = top_level(text).ok_or_else(|| bad(n, "dbg"))?;
    let mut found = None;
    for (i, _) in text.match_indices("!dbg ") {
        if inside(&groups, i) || in_string(text, i) {
            continue;
        }
        let id = node_ref(&text[i + 5..]).ok_or_else(|| bad(n, "dbg"))?;
        if found.replace(id).is_some() {
            return Err(bad(n, "dbg"));
        }
    }
    Ok(found)
}

fn in_string(text: &str, i: usize) -> bool {
    text[..i].bytes().filter(|&b| b == b'"').count() % 2 == 1
}

/// `!N` at the start of `text`.
fn node_ref(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('!')?;
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse().ok()
}

/// The top-level bracket groups of `text`: kind, byte offset of the opening and of the
/// closing bracket. Strings are skipped. `None` when brackets do not balance.
fn top_level(text: &str) -> Option<Vec<(char, usize, usize)>> {
    let mut groups = Vec::new();
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut chars = text.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => loop {
                match chars.next() {
                    Some((_, '"')) => break,
                    Some(_) => {}
                    None => return None,
                }
            },
            '(' | '[' | '{' | '<' => stack.push((c, i)),
            ')' | ']' | '}' | '>' => {
                let (open, at) = stack.pop()?;
                let expected = match open {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    _ => '>',
                };
                if c != expected {
                    return None;
                }
                if stack.is_empty() {
                    groups.push((open, at, i));
                }
            }
            _ => {}
        }
    }
    stack.is_empty().then_some(groups)
}

/// An LLVM string at the start of `text`: the decoded value and the text after it. `\\`
/// and `\xx` are decoded and the bytes must be UTF-8.
fn string(text: &str) -> Option<(String, &str)> {
    let body = text.strip_prefix('"')?;
    let end = body.find('"')?;
    let raw = &body.as_bytes()[..end];
    let mut bytes = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\\' {
            if raw.get(i + 1) == Some(&b'\\') {
                bytes.push(b'\\');
                i += 2;
                continue;
            }
            let hex = std::str::from_utf8(raw.get(i + 1..i + 3)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    Some((String::from_utf8(bytes).ok()?, &body[end + 1..]))
}

/// A metadata line `!N = [distinct] !Kind(fields)` of an understood kind. Other metadata
/// lines (tuples, named metadata, other kinds) give `None`.
fn metadata(line: &str, n: usize) -> Result<Option<(u32, Node)>, Unreadable> {
    let Some(id) = node_ref(line) else {
        return Ok(None);
    };
    let digits = id.to_string().len() + 1;
    let Some(rest) = line[digits..].strip_prefix(" = ") else {
        return Ok(None);
    };
    let rest = rest.strip_prefix("distinct ").unwrap_or(rest);
    let Some(rest) = rest.strip_prefix('!') else {
        return Ok(None);
    };
    let name_end = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    let Some(&kind) = KINDS.iter().find(|k| **k == &rest[..name_end]) else {
        return Ok(None);
    };
    let what = format!("{kind} node");
    let args = rest[name_end..]
        .strip_prefix('(')
        .and_then(|a| a.strip_suffix(')'))
        .ok_or_else(|| Unreadable::new(n, format!("a {what} that does not parse")))?;
    let fields =
        fields(args).ok_or_else(|| Unreadable::new(n, format!("a {what} that does not parse")))?;
    Ok(Some((
        id,
        Node {
            kind,
            fields,
            line: n,
        },
    )))
}

fn fields(args: &str) -> Option<Fields> {
    let mut out = Fields::default();
    let mut seen = BTreeSet::new();
    if args.trim().is_empty() {
        return Some(out);
    }
    let groups = top_level(args)?;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, c) in args.char_indices() {
        if c == ',' && !inside(&groups, i) && !in_string(args, i) {
            parts.push(&args[start..i]);
            start = i + 1;
        }
    }
    parts.push(&args[start..]);
    for part in parts {
        let (key, value) = part.split_once(':')?;
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let value = value.trim();
        let value = if value == "null" {
            Value::Null
        } else if value.starts_with('"') {
            let (s, tail) = string(value)?;
            if !tail.is_empty() {
                return None;
            }
            Value::Str(s)
        } else if value.starts_with('!') && value[1..].bytes().all(|b| b.is_ascii_digit()) {
            Value::Ref(node_ref(value)?)
        } else {
            Value::Raw(value.to_owned())
        };
        if !seen.insert(key) {
            return None;
        }
        if let Some(kept) = KEPT.iter().find(|kept| **kept == key) {
            out.0.push((kept, value));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_function_type_before_the_operand_is_not_the_argument_list() {
        let t = callee("i32 (ptr, ...) @printf(ptr %x), !dbg !4", 1).unwrap();
        assert_eq!(t, Some(Target::Direct("printf".into())));
    }

    #[test]
    fn a_quoted_global_is_decoded() {
        let t = callee("void @\"a\\22b\"(i32 1)", 1).unwrap();
        assert_eq!(t, Some(Target::Direct("a\"b".into())));
    }

    #[test]
    fn inline_assembly_is_passed_over() {
        let t = callee(
            "void asm sideeffect \"\", \"r,~{memory}\"(ptr %x) #9, !dbg !93",
            1,
        );
        assert_eq!(t, Ok(None));
    }

    #[test]
    fn a_constant_operand_does_not_parse() {
        callee("void 0(i32 1)", 1).unwrap_err();
    }

    #[test]
    fn an_error_names_the_line_it_is_on() {
        assert_eq!(Unreadable::new(0, "whole").to_string(), "whole");
        assert_eq!(Unreadable::new(3, "part").to_string(), "line 3: part");
        // A metadata line and a function line, each the second line of its text.
        let e = read("; first\n!1 = !DIFile(filename \"x\")\n").unwrap_err();
        assert_eq!(e.line, 2, "{e}");
        let e = read("; first\ndefine void @f() !dbg !1\n").unwrap_err();
        assert_eq!(e.line, 2, "{e}");
        let e = read("; first\ndeclare void f()\n").unwrap_err();
        assert_eq!(e.line, 2, "{e}");
    }

    #[test]
    fn one_target_triple_with_nothing_after_it() {
        let triple = "target triple = \"x86_64-unknown-linux-gnu\"";
        assert_eq!(
            read(triple).unwrap().triple.as_deref(),
            Some("x86_64-unknown-linux-gnu")
        );
        assert_eq!(read(&format!("{triple}\n{triple}\n")).unwrap_err().line, 2);
        assert_eq!(read(&format!("{triple} x\n")).unwrap_err().line, 1);
    }

    #[test]
    fn a_global_name_is_bare_characters_or_one_quoted_string() {
        assert_eq!(global("_R1a.b$c-d"), Some("_R1a.b$c-d".to_owned()));
        assert_eq!(global(""), None);
        assert_eq!(global("a<b"), None);
        assert_eq!(global("\"q\" x"), None);
        let text = "define void @\"a b\"() unnamed_addr {\n}\ndeclare i32 @\"c(d\"(i32)\n";
        let t = read(text).unwrap();
        assert_eq!(t.functions[0].symbol, "a b");
        assert!(t.declared.contains("c(d"));
    }

    #[test]
    fn an_attachment_inside_brackets_or_a_string_is_not_the_instructions() {
        assert_eq!(
            dbg("void @f(metadata !5, !dbg !6) #2, !dbg !9", 1),
            Ok(Some(9))
        );
        assert_eq!(dbg("void @\"!dbg !6\"(), !dbg !9", 1), Ok(Some(9)));
        assert_eq!(dbg("void @f()", 1), Ok(None));
        dbg("void @f(), !dbg !1, !dbg !2", 1).unwrap_err();
    }

    #[test]
    fn escapes_decode_wherever_they_stand() {
        assert_eq!(string(r#""a\\b""#), Some(("a\\b".to_owned(), "")));
        assert_eq!(string(r#""\5Cx\22""#), Some(("\\x\"".to_owned(), "")));
        assert_eq!(string(r#""ab\\\\c""#), Some(("ab\\\\c".to_owned(), "")));
        assert_eq!(string(r#""\FF""#), None, "not UTF-8");
    }

    #[test]
    fn a_field_has_a_plain_key() {
        assert!(fields("line: 3, name: \"x\"").is_some());
        assert_eq!(fields(": 3"), None);
        assert_eq!(fields("a-b: 3"), None);
        assert_eq!(fields("line: 3, line: 4"), None);
    }
}
