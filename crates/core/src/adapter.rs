//! Runs an adapter and keeps only contract-validated inputs and stable diagnostics.
use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

use gosanja_schema::{
    compat::{self, Acceptance, RowVerdict, Version},
    contract::{
        AdapterManifest, AdapterResult, CONTRACT_VERSION, CallEvidence, LayerName, LayerStatus,
        RESULT_FILE, SyntaxFact,
    },
    snapshot::{BuildContext, FailedDetail, GapKind, GapRow, Layer, LayerState, Layers},
};
use jsonschema::Validator;

use crate::schemas::{Schema, validator};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tempfile::TempDir;

/// Base index and optional layers for one invocation. The index stays open until consumed.
#[derive(Debug)]
pub struct AdapterOutput {
    pub index: File,
    pub manifest: AdapterManifest,
    pub builds: Vec<BuildContext>,
    pub layers: Layers,
    pub syntax: Vec<SyntaxFact>,
    pub calls: Vec<CallEvidence>,
    pub gaps: Vec<GapRow>,
    _directory: TempDir,
}

/// Compatibility refusals and adapter/input failures have distinct CLI outcomes.
#[derive(Debug)]
pub enum AdapterError {
    Incompatible(compat::Refusal),
    Failed(io::Error),
}

impl AdapterError {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Incompatible(_) => 2,
            Self::Failed(_) => 3,
        }
    }
}
impl From<io::Error> for AdapterError {
    fn from(error: io::Error) -> Self {
        Self::Failed(error)
    }
}
impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incompatible(reason) => reason.fmt(f),
            Self::Failed(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for AdapterError {}

fn parse<T: DeserializeOwned>(schema: &Validator, value: Value) -> io::Result<T> {
    if let Err(error) = schema.validate(&value) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            error.to_string(),
        ));
    }
    Ok(serde_json::from_value(value)?)
}

fn regular(path: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected a regular file at {}", path.display()),
        ));
    }
    File::open(path)
}

fn read_rows<T: DeserializeOwned>(
    mut reader: impl BufRead,
    schema: &Validator,
    acceptance: Acceptance,
    valid: impl Fn(&T) -> bool,
) -> io::Result<(Vec<T>, u64)> {
    let mut rows = Vec::new();
    let mut skipped = 0;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok((rows, skipped));
        }
        if !line.ends_with('\n') || line.ends_with("\r\n") {
            return Err(io::Error::other("expected an LF-terminated JSONL row"));
        }
        let value: Value = serde_json::from_str(&line)?;
        let mut unknown = false;
        let mut well_formed = true;
        for error in schema.iter_errors(&value) {
            if error.kind().keyword() == "enum" && error.instance().is_string() {
                unknown = true;
            } else {
                well_formed = false;
                break;
            }
        }
        let row = serde_json::from_value(value)?;
        match compat::classify_row(acceptance, well_formed && valid(&row), unknown) {
            RowVerdict::Keep => rows.push(row),
            RowVerdict::Skip => skipped += 1,
            RowVerdict::Malformed => return Err(io::Error::other("invalid side-car row")),
        }
    }
}

/// Reads syntax facts and returns the count skipped under the compatibility policy.
///
/// # Errors
/// An invalid row or stream invalidates the whole layer.
pub fn read_syntax(
    reader: impl BufRead,
    acceptance: Acceptance,
) -> io::Result<(Vec<SyntaxFact>, u64)> {
    read_rows(
        reader,
        validator(Schema::SyntaxFact),
        acceptance,
        crate::sidecar::valid_ranges,
    )
}

/// Reads compiler call observations and returns the count skipped under the compatibility policy.
///
/// # Errors
/// An invalid row or stream invalidates the whole layer.
pub fn read_calls(
    reader: impl BufRead,
    acceptance: Acceptance,
) -> io::Result<(Vec<CallEvidence>, u64)> {
    read_rows(reader, validator(Schema::CallEvidence), acceptance, |_| {
        true
    })
}

struct OptionalLayer<'a> {
    adapter: &'a str,
    acceptance: Acceptance,
    name: &'static str,
    layer: Layer,
    offered: bool,
}

impl OptionalLayer<'_> {
    fn read<T: DeserializeOwned>(
        &self,
        dir: &Path,
        result: &AdapterResult,
        gaps: &mut Vec<GapRow>,
        read: impl FnOnce(BufReader<File>, Acceptance) -> io::Result<(Vec<T>, u64)>,
    ) -> (LayerState, Vec<T>) {
        let mut gap = GapRow::new(
            GapKind::FailedLayer,
            format!("adapter:{}/{}", self.adapter, self.name),
        );
        gap.layer = Some(self.layer);
        if !self.offered {
            gap.gap = GapKind::MissingLayer;
            gaps.push(gap);
            return (LayerState::Absent, vec![]);
        }
        gap.detail = Some(match result.layers.get(self.name) {
            None => FailedDetail::NoStatus,
            Some(entry) if entry.status == LayerStatus::Failed => {
                gap.code = entry
                    .diagnostics
                    .as_deref()
                    .and_then(|ds| ds.first())
                    .map(|d| d.code.clone());
                FailedDetail::ReportedFailed
            }
            Some(_) => match regular(&dir.join(format!("{}.jsonl", self.name))) {
                Err(_) => FailedDetail::FileMissing,
                Ok(file) => match read(BufReader::new(file), self.acceptance) {
                    Ok((rows, skipped)) => {
                        if skipped > 0 {
                            gap.gap = GapKind::UnknownValue;
                            gap.count = Some(skipped);
                            gaps.push(gap);
                        }
                        return (LayerState::Present, rows);
                    }
                    Err(_) => FailedDetail::RowInvalid,
                },
            },
        });
        gaps.push(gap);
        (LayerState::Failed, vec![])
    }
}

/// Reads and validates the manifest printed by an adapter and selects the reader policy.
///
/// # Errors
/// Reports process or manifest errors and refuses incompatible contract versions.
pub fn describe(mut command: Command) -> Result<(AdapterManifest, Acceptance), AdapterError> {
    let describe = command.arg("describe").stderr(Stdio::inherit()).output()?;
    if !describe.status.success() {
        return Err(io::Error::other(format!("describe exited {}", describe.status)).into());
    }
    let value: Value = serde_json::from_slice(&describe.stdout).map_err(io::Error::from)?;
    let version: Version = value["contract"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .map_err(io::Error::other)?;
    let acceptance =
        compat::accept(CONTRACT_VERSION, version).map_err(AdapterError::Incompatible)?;
    let manifest: AdapterManifest = parse(validator(Schema::AdapterManifest), value)?;
    Ok((manifest, acceptance))
}

/// Runs describe and index in sequence, with a fresh command for each invocation.
///
/// # Errors
/// Refuses incompatible manifests before indexing. Reports process, protocol and base-layer
/// failures. Optional layer failures are returned as gaps alongside the base index.
pub fn run(
    command: impl Fn() -> Command,
    root: &Path,
    scratch_parent: &Path,
) -> Result<AdapterOutput, AdapterError> {
    let root = root.canonicalize()?;
    let parent = scratch_parent.canonicalize()?;
    if parent.starts_with(&root) {
        return Err(io::Error::other("adapter output would be inside source tree").into());
    }
    let (manifest, acceptance) = describe(command())?;
    let dir = tempfile::tempdir_in(parent)?;
    let status = command()
        .arg("index")
        .arg("--root")
        .arg(&root)
        .arg("--out")
        .arg(dir.path())
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!("index exited {status}")).into());
    }
    let mut value: Value = serde_json::from_reader(regular(&dir.path().join(RESULT_FILE))?)
        .map_err(io::Error::from)?;
    // Unknown fields under layers need no status, regardless of their JSON type.
    if let Some(layers) = value.get_mut("layers").and_then(Value::as_object_mut) {
        layers.retain(|k, _| matches!(k.as_str(), "base" | "syntax" | "calls"));
    }
    let result: AdapterResult = parse(validator(Schema::AdapterResult), value)?;
    if result.contract != manifest.contract {
        return Err(io::Error::other("result contract differs from describe").into());
    }
    for entry in result.layers.values() {
        for diagnostic in entry.diagnostics.iter().flatten() {
            if let Some(message) = &diagnostic.message {
                eprintln!("{}: {message}", diagnostic.code);
            }
        }
    }
    if !result
        .layers
        .get("base")
        .is_some_and(|entry| entry.status == LayerStatus::Ok)
    {
        return Err(io::Error::other("base layer did not succeed").into());
    }
    let index = regular(&dir.path().join("index.scip"))?;
    let mut gaps = Vec::new();
    let (syntax_state, syntax) = OptionalLayer {
        adapter: &manifest.name,
        acceptance,
        name: "syntax",
        layer: Layer::Syntax,
        offered: manifest.layers.contains(&LayerName::Syntax),
    }
    .read(dir.path(), &result, &mut gaps, read_syntax);
    let (calls_state, calls) = OptionalLayer {
        adapter: &manifest.name,
        acceptance,
        name: "calls",
        layer: Layer::Calls,
        offered: manifest.layers.contains(&LayerName::Calls),
    }
    .read(dir.path(), &result, &mut gaps, read_calls);
    let builds = result.builds.unwrap_or_default();
    let mut ids = std::collections::HashSet::new();
    if builds.iter().any(|context| !ids.insert(&context.id)) {
        return Err(io::Error::other("duplicate build context id").into());
    }
    Ok(AdapterOutput {
        index,
        manifest,
        builds,
        layers: Layers {
            base: LayerState::Present,
            syntax: syntax_state,
            calls: calls_state,
        },
        syntax,
        calls,
        gaps,
        _directory: dir,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn newer_minor_reports_a_count_only_for_a_retained_layer() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("syntax.jsonl");
        let result: AdapterResult =
            serde_json::from_value(json!({"contract":"1.1","layers":{"syntax":{"status":"ok"}}}))
                .unwrap();
        let layer = OptionalLayer {
            name: "syntax",
            layer: Layer::Syntax,
            offered: true,
            acceptance: Acceptance::SkipUnknown,
            adapter: "fake",
        };
        let future = r#"{"anchor":{"file":"x","line":0,"col":0},"outline":[{"k":"future","range":[0,0,0,1]}]}"#;
        let known = r#"{"anchor":{"file":"x","line":0,"col":0}}"#;
        fs::write(&file, format!("{future}\n{known}\n{future}\n")).unwrap();
        let mut gaps = Vec::new();
        let (state, rows) = layer.read(dir.path(), &result, &mut gaps, read_syntax);
        assert_eq!((state, rows.len()), (LayerState::Present, 1));
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].gap, GapKind::UnknownValue);
        assert_eq!(gaps[0].count, Some(2));
        assert_eq!(gaps[0].src, "adapter:fake/syntax");
        fs::write(file, format!("{future}\n{known}\n{{}}\n")).unwrap();
        gaps.clear();
        let (state, rows) = layer.read(dir.path(), &result, &mut gaps, read_syntax);
        assert_eq!((state, rows.len()), (LayerState::Failed, 0));
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].detail, Some(FailedDetail::RowInvalid));
    }
}
