//! A module's debug records as the calls layer reads them.
//!
//! Subprograms, locations, the scopes between them, files and the compile unit. Every
//! reference the layer follows is checked when the module is read, so a module either reads
//! whole or fails.

use super::ir::{self, Node, Target, Unreadable, Value};
use md5::{Digest, Md5};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Why a module was refused, with the toolchain its compile unit names when it was read
/// that far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// `ir-unreadable`.
    Unreadable {
        error: Unreadable,
        toolchain: Option<String>,
    },
    /// `build-unsupported`: the compile unit does not say `isOptimized: false`.
    Optimised { toolchain: String },
}

impl Refused {
    /// The toolchain the module's compile unit names, if it was read that far.
    #[must_use]
    pub fn toolchain(&self) -> Option<&str> {
        match self {
            Self::Unreadable { toolchain, .. } => toolchain.as_deref(),
            Self::Optimised { toolchain } => Some(toolchain),
        }
    }
}

/// A `DISubprogram`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subprogram {
    pub name: String,
    pub file: Option<u32>,
    /// 1-based; 0 when absent.
    pub line: u32,
}

/// A `DILocation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// 1-based; 0 names no place.
    pub line: u32,
    /// 1-based UTF-8 bytes; 0 when absent.
    pub column: u32,
    pub scope: u32,
}

/// A `DIFile`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct File {
    pub directory: String,
    pub filename: String,
    /// Lowercase hexadecimal, when the checksum kind is MD5.
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Subprogram { file: Option<u32> },
    Block { parent: u32, file: Option<u32> },
}

/// A call instruction of a defined function, with its records resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub target: Target,
    pub location: Option<Location>,
}

/// A defined function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub symbol: String,
    /// The node of its subprogram.
    pub subprogram: Option<u32>,
    pub instructions: Vec<Instruction>,
}

/// One module, read and checked.
#[derive(Debug)]
pub struct Module {
    /// `rustc ` and the version the compile unit's producer names.
    pub toolchain: String,
    pub triple: String,
    pub functions: Vec<Function>,
    /// Symbols the module only declares.
    pub declared: BTreeSet<String>,
    subprograms: BTreeMap<u32, Subprogram>,
    scopes: BTreeMap<u32, Scope>,
    files: BTreeMap<u32, File>,
    defined: BTreeMap<String, Option<u32>>,
}

impl Module {
    /// Reads `text` as a module of rustc: it holds a target triple and one compile unit whose
    /// producer names rustc with a version, and every record a call instruction leads to
    /// resolves to a node of the expected kind.
    ///
    /// # Errors
    /// [`Refused::Unreadable`] when a requirement fails, and only then
    /// [`Refused::Optimised`] when the compile unit does not say it was not optimised.
    pub fn read(text: &str) -> Result<Self, Refused> {
        let early = |error| Refused::Unreadable {
            error,
            toolchain: None,
        };
        let ir = ir::read(text).map_err(early)?;
        let nodes = Nodes(&ir.nodes);
        let (toolchain, optimised) = nodes.compile_unit().map_err(early)?;
        let m =
            Self::resolve(&ir, &nodes, toolchain.clone()).map_err(|error| Refused::Unreadable {
                error,
                toolchain: Some(toolchain.clone()),
            })?;
        if optimised {
            return Err(Refused::Optimised { toolchain });
        }
        Ok(m)
    }

    fn resolve(ir: &ir::Text, nodes: &Nodes<'_>, toolchain: String) -> Result<Self, Unreadable> {
        let triple = ir
            .triple
            .clone()
            .ok_or_else(|| Unreadable::new(0, "no target triple"))?;
        let mut m = Self {
            toolchain,
            triple,
            functions: Vec::with_capacity(ir.functions.len()),
            declared: ir.declared.clone(),
            subprograms: BTreeMap::new(),
            scopes: BTreeMap::new(),
            files: BTreeMap::new(),
            defined: BTreeMap::new(),
        };
        for f in &ir.functions {
            if let Some(id) = f.dbg {
                m.subprogram_node(nodes, id, f.line)?;
            }
            let subprogram = f.dbg;
            let mut instructions = Vec::with_capacity(f.calls.len());
            for call in &f.calls {
                let location = call
                    .dbg
                    .map(|id| m.location(nodes, id, call.line))
                    .transpose()?;
                instructions.push(Instruction {
                    target: call.target.clone(),
                    location,
                });
            }
            if m.defined.insert(f.symbol.clone(), subprogram).is_some() {
                return Err(Unreadable::new(
                    f.line,
                    format!("{} defined twice", f.symbol),
                ));
            }
            m.functions.push(Function {
                symbol: f.symbol.clone(),
                subprogram,
                instructions,
            });
        }
        Ok(m)
    }

    /// Whether the module defines `symbol`, and its subprogram if it has one.
    #[must_use]
    pub fn definition(&self, symbol: &str) -> Option<Option<&Subprogram>> {
        self.defined
            .get(symbol)
            .map(|sp| sp.map(|id| &self.subprograms[&id]))
    }

    #[must_use]
    pub fn subprogram(&self, id: u32) -> &Subprogram {
        &self.subprograms[&id]
    }

    #[must_use]
    pub fn file(&self, id: u32) -> &File {
        &self.files[&id]
    }

    /// The files the module's subprograms and scopes name.
    pub fn files(&self) -> impl Iterator<Item = (u32, &File)> {
        self.files.iter().map(|(id, f)| (*id, f))
    }

    /// The file of the innermost scope of `scope` that names one.
    #[must_use]
    pub fn scope_file(&self, mut scope: u32) -> Option<u32> {
        loop {
            match self.scopes[&scope] {
                Scope::Subprogram { file } => return file,
                Scope::Block { file: Some(f), .. } => return Some(f),
                Scope::Block { parent, file: None } => scope = parent,
            }
        }
    }

    /// The subprogram that holds `scope`.
    #[must_use]
    pub fn scope_subprogram(&self, mut scope: u32) -> u32 {
        loop {
            match self.scopes[&scope] {
                Scope::Subprogram { .. } => return scope,
                Scope::Block { parent, .. } => scope = parent,
            }
        }
    }

    fn subprogram_node(&mut self, nodes: &Nodes<'_>, id: u32, at: usize) -> Result<(), Unreadable> {
        if self.subprograms.contains_key(&id) {
            return Ok(());
        }
        let node = nodes.get(id, &["DISubprogram"], at)?;
        let name = match node.fields.get("name") {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err(nodes.wrong(id, "a DISubprogram without a name")),
        };
        let file = self.file_field(nodes, node, id)?;
        let line = number(node, "line", id)?;
        self.subprograms.insert(id, Subprogram { name, file, line });
        self.scopes.insert(id, Scope::Subprogram { file });
        Ok(())
    }

    fn location(&mut self, nodes: &Nodes<'_>, id: u32, at: usize) -> Result<Location, Unreadable> {
        let node = nodes.get(id, &["DILocation"], at)?;
        let scope = match node.fields.get("scope") {
            Some(Value::Ref(s)) => *s,
            _ => return Err(nodes.wrong(id, "a DILocation without a scope")),
        };
        self.scope(nodes, scope, node.line)?;
        if let Some(v) = node.fields.get("inlinedAt") {
            match v {
                Value::Ref(i) => {
                    self.location(nodes, *i, node.line)?;
                }
                _ => return Err(nodes.wrong(id, "an inlinedAt that is no location")),
            }
        }
        Ok(Location {
            line: number(node, "line", id)?,
            column: number(node, "column", id)?,
            scope,
        })
    }

    fn scope(&mut self, nodes: &Nodes<'_>, id: u32, at: usize) -> Result<(), Unreadable> {
        let mut chain = Vec::new();
        let mut current = id;
        loop {
            if self.scopes.contains_key(&current) {
                break;
            }
            if chain.contains(&current) {
                return Err(nodes.wrong(current, "a scope that holds itself"));
            }
            let node = nodes.get(
                current,
                &["DISubprogram", "DILexicalBlock", "DILexicalBlockFile"],
                at,
            )?;
            if node.kind == "DISubprogram" {
                self.subprogram_node(nodes, current, at)?;
                break;
            }
            let parent = match node.fields.get("scope") {
                Some(Value::Ref(p)) => *p,
                _ => return Err(nodes.wrong(current, "a lexical block without a scope")),
            };
            let file = self.file_field(nodes, node, current)?;
            chain.push(current);
            self.scopes.insert(current, Scope::Block { parent, file });
            current = parent;
        }
        Ok(())
    }

    fn file_field(
        &mut self,
        nodes: &Nodes<'_>,
        node: &Node,
        id: u32,
    ) -> Result<Option<u32>, Unreadable> {
        let file = match node.fields.get("file") {
            None | Some(Value::Null) => return Ok(None),
            Some(Value::Ref(f)) => *f,
            Some(_) => return Err(nodes.wrong(id, "a file that is no reference")),
        };
        if let std::collections::btree_map::Entry::Vacant(e) = self.files.entry(file) {
            e.insert(nodes.file(file, node.line)?);
        }
        Ok(Some(file))
    }
}

fn number(node: &Node, key: &str, id: u32) -> Result<u32, Unreadable> {
    match node.fields.get(key) {
        None => Ok(0),
        Some(Value::Raw(n)) => n
            .parse()
            .map_err(|e| Unreadable::new(node.line, format!("!{id}: {key}: {e}"))),
        Some(_) => Err(Unreadable::new(
            node.line,
            format!("!{id}: {key} is not a number"),
        )),
    }
}

struct Nodes<'a>(&'a BTreeMap<u32, Node>);

impl<'a> Nodes<'a> {
    fn get(&self, id: u32, kinds: &[&str], at: usize) -> Result<&'a Node, Unreadable> {
        let node = self
            .0
            .get(&id)
            .ok_or_else(|| Unreadable::new(at, format!("!{id} is not a node this reader knows")))?;
        if kinds.contains(&node.kind) {
            Ok(node)
        } else {
            Err(Unreadable::new(
                at,
                format!("!{id} is a {}, expected {}", node.kind, kinds.join(" or ")),
            ))
        }
    }

    fn wrong(&self, id: u32, what: &str) -> Unreadable {
        let line = self.0.get(&id).map_or(0, |n| n.line);
        Unreadable::new(line, format!("!{id}: {what}"))
    }

    fn file(&self, id: u32, at: usize) -> Result<File, Unreadable> {
        let node = self.get(id, &["DIFile"], at)?;
        let text = |key: &str| match node.fields.get(key) {
            Some(Value::Str(s)) => Ok(s.clone()),
            None => Ok(String::new()),
            Some(_) => Err(self.wrong(id, "a DIFile field that is no string")),
        };
        let md5 = match (node.fields.get("checksumkind"), node.fields.get("checksum")) {
            (Some(Value::Raw(kind)), Some(Value::Str(sum))) if kind == "CSK_MD5" => {
                Some(sum.to_ascii_lowercase())
            }
            _ => None,
        };
        Ok(File {
            directory: text("directory")?,
            filename: text("filename")?,
            md5,
        })
    }

    /// The toolchain the one compile unit names, and whether it says it was optimised.
    fn compile_unit(&self) -> Result<(String, bool), Unreadable> {
        let mut units = self.0.values().filter(|n| n.kind == "DICompileUnit");
        let unit = units
            .next()
            .ok_or_else(|| Unreadable::new(0, "no compile unit"))?;
        if let Some(second) = units.next() {
            return Err(Unreadable::new(second.line, "a second compile unit"));
        }
        let Some(Value::Str(producer)) = unit.fields.get("producer") else {
            return Err(Unreadable::new(
                unit.line,
                "a compile unit without a producer",
            ));
        };
        let version = producer
            .split_once("rustc version ")
            .and_then(|(_, v)| v.strip_suffix(')'))
            .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()))
            .ok_or_else(|| {
                Unreadable::new(
                    unit.line,
                    format!("a module of another producer: {producer}"),
                )
            })?;
        let optimised =
            !matches!(unit.fields.get("isOptimized"), Some(Value::Raw(v)) if v == "false");
        Ok((format!("rustc {version}"), optimised))
    }
}

/// The files of one root that modules name, each read once.
#[derive(Debug)]
pub struct RootFiles {
    root: PathBuf,
    seen: BTreeMap<File, Option<Source>>,
}

/// A file of the root: its path relative to the root and the bytes the compiler read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub path: String,
    pub bytes: Rc<[u8]>,
}

impl RootFiles {
    /// `root` must be resolved.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            seen: BTreeMap::new(),
        }
    }

    /// The file of the root that `file` names, if it names one: its directory and name
    /// joined as they stand give an absolute path that, links resolved, exists inside the
    /// root, whose path relative to the root is one the contract can carry, and whose bytes
    /// have the MD5 checksum the record carries.
    pub fn get(&mut self, file: &File) -> Option<Source> {
        if let Some(known) = self.seen.get(file) {
            return known.clone();
        }
        let found = self.find(file);
        self.seen.insert(file.clone(), found.clone());
        found
    }

    fn find(&self, file: &File) -> Option<Source> {
        let md5 = file.md5.as_deref()?;
        let joined = Path::new(&file.directory).join(&file.filename);
        if !joined.is_absolute() {
            return None;
        }
        let resolved = joined.canonicalize().ok()?;
        let path = resolved.strip_prefix(&self.root).ok()?.to_str()?.to_owned();
        if !carried(&path) {
            return None;
        }
        let bytes = fs::read(&resolved).ok()?;
        let sum = Md5::digest(&bytes);
        let hex = sum.iter().fold(String::with_capacity(32), |mut hex, b| {
            hex.push(char::from(b"0123456789abcdef"[usize::from(b >> 4)]));
            hex.push(char::from(b"0123456789abcdef"[usize::from(b & 15)]));
            hex
        });
        (hex == md5).then(|| Source {
            path,
            bytes: bytes.into(),
        })
    }
}

/// The contract's rule for a path relative to the root.
fn carried(path: &str) -> bool {
    let b = path.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/';
    !path.contains('\\')
        && !path.starts_with('/')
        && !drive
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
