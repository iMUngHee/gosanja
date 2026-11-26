//! Runs an adapter and keeps only contract-validated inputs and stable diagnostics.
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use gosanja_schema::{
    compat::{self, Acceptance, RowVerdict, Version},
    contract::{
        AdapterManifest, AdapterResult, BuildRow, CONTRACT_VERSION, CallEvidence, LayerName,
        LayerRequest, LayerResult, LayerStatus, RESULT_FILE, SkippedFile, SyntaxFact, SyntaxLine,
        TestRow,
    },
    snapshot::{BuildContext, FailedDetail, GapKind, GapRow, Layer, LayerState, Layers},
};
use jsonschema::Validator;

use crate::schemas::{self, Fit, Schema, validator};
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
    /// Test rows of the syntax layer, apart from its syntax rows.
    pub tests: Vec<TestRow>,
    pub calls: Vec<CallEvidence>,
    /// Rows of the build layer, read and shape-checked, not yet joined.
    pub build: Vec<BuildRow>,
    /// The build entry's `skipped`, which the build layer's phases judge before any of it
    /// becomes a gap.
    pub build_skipped: Vec<SkippedFile>,
    /// Symbols the base layer states are declared at several sites (`merged`).
    pub merged: Vec<String>,
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

fn read_result(directory: &Path, contract: &str, passed: &[&str]) -> io::Result<AdapterResult> {
    let mut value: Value =
        serde_json::from_reader(regular(&directory.join(RESULT_FILE))?).map_err(io::Error::from)?;
    if value.get("contract").and_then(Value::as_str) != Some(contract) {
        return Err(io::Error::other("result contract differs from describe"));
    }
    if let Some(layers) = value.get_mut("layers").and_then(Value::as_object_mut) {
        layers.retain(|k, _| passed.contains(&k.as_str()));
        // `merged` belongs to the base entry; elsewhere it is an unknown field, which a
        // reader ignores whatever its value.
        for (name, entry) in layers.iter_mut() {
            if name != "base"
                && let Some(entry) = entry.as_object_mut()
            {
                entry.remove("merged");
            }
        }
    }
    if !passed.contains(&"calls")
        && let Some(object) = value.as_object_mut()
    {
        object.remove("builds");
    }
    let result: AdapterResult = parse(validator(Schema::AdapterResult), value)?;
    if let Some(merged) = result.layers.get("base").and_then(|b| b.merged.as_deref()) {
        check_merged(merged)?;
    }
    let mut ids = std::collections::HashSet::new();
    if result
        .builds
        .iter()
        .flatten()
        .any(|context| !ids.insert(&context.id))
    {
        return Err(io::Error::other("duplicate build context id"));
    }
    Ok(result)
}

/// The base entry's `merged`: global SCIP symbols by the grammar alone, sorted by bytes, each
/// once. It is checked whatever the base layer's status, as part of reading `result.json`.
fn check_merged(merged: &[String]) -> io::Result<()> {
    if let Some(symbol) = merged.iter().find(|s| {
        !matches!(
            crate::identity::parse_strict(s),
            Ok(crate::identity::Parsed::Global(_))
        )
    }) {
        return Err(io::Error::other(format!(
            "merged: {symbol:?} is not a global SCIP symbol"
        )));
    }
    if let Some(pair) = merged.windows(2).find(|pair| pair[0] >= pair[1]) {
        return Err(io::Error::other(if pair[0] == pair[1] {
            format!("merged: {:?} is listed twice", pair[0])
        } else {
            format!(
                "merged: {:?} comes after {:?}; the list is not sorted",
                pair[1], pair[0]
            )
        }));
    }
    Ok(())
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
    schema: Schema,
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
        let fit = schemas::fit(schema, &value);
        let unknown = fit == Fit::UnknownValueOnly;
        let well_formed = !matches!(fit, Fit::Invalid(_));
        let row = serde_json::from_value(value)?;
        match compat::classify_row(acceptance, well_formed && valid(&row), unknown) {
            RowVerdict::Keep => rows.push(row),
            RowVerdict::Skip => skipped += 1,
            RowVerdict::Malformed => return Err(io::Error::other("invalid side-car row")),
        }
    }
}

/// Reads the syntax and test rows of `syntax.jsonl` and returns the count skipped under the
/// compatibility policy.
///
/// # Errors
/// An invalid row or stream invalidates the whole layer.
pub fn read_syntax(
    reader: impl BufRead,
    acceptance: Acceptance,
) -> io::Result<(Vec<SyntaxLine>, u64)> {
    read_rows(reader, Schema::SyntaxFact, acceptance, |line| match line {
        SyntaxLine::Fact(fact) => crate::sidecar::valid_ranges(fact),
        SyntaxLine::Test(_) => true,
    })
}

/// Reads the rows of `build.jsonl` and returns the count skipped under the compatibility
/// policy.
///
/// # Errors
/// An invalid row or stream invalidates the whole layer.
pub fn read_build(
    reader: impl BufRead,
    acceptance: Acceptance,
) -> io::Result<(Vec<BuildRow>, u64)> {
    read_rows(reader, Schema::Build, acceptance, |_| true)
}

/// Reads compiler call observations and returns the count skipped under the compatibility policy.
///
/// # Errors
/// An invalid row or stream invalidates the whole layer.
pub fn read_calls(
    reader: impl BufRead,
    acceptance: Acceptance,
) -> io::Result<(Vec<CallEvidence>, u64)> {
    read_rows(reader, Schema::CallEvidence, acceptance, |_| true)
}

fn skipped_files(entry: &LayerResult, layer: Layer, src: &str) -> impl Iterator<Item = GapRow> {
    let distinct: std::collections::BTreeSet<_> = entry.skipped.iter().flatten().collect();
    distinct.into_iter().map(move |file| {
        let mut gap = GapRow::new(GapKind::SkippedFile, src);
        gap.layer = Some(layer);
        gap.file = Some(file.file.clone());
        gap.code = Some(file.code.clone());
        gap
    })
}

struct OptionalLayer<'a> {
    adapter: &'a str,
    acceptance: Acceptance,
    name: &'static str,
    layer: Layer,
    offered: bool,
    requested: bool,
    /// The scope a build-layer outcome gap names; `None` for the other layers.
    scope: Option<&'a str>,
    /// Where the entry's `skipped` goes when the layer's own phases judge it before it
    /// becomes a gap; `None` turns it into gaps at once.
    deferred: Option<&'a mut Vec<SkippedFile>>,
}

impl OptionalLayer<'_> {
    fn read<T: DeserializeOwned>(
        self,
        dir: &Path,
        result: &AdapterResult,
        gaps: &mut Vec<GapRow>,
        open: impl FnOnce(&Path) -> io::Result<File>,
        read: impl FnOnce(BufReader<File>, Acceptance) -> io::Result<(Vec<T>, u64)>,
    ) -> (LayerState, Vec<T>) {
        if !self.requested {
            return (LayerState::NotRequested, vec![]);
        }
        let mut gap = GapRow::new(
            GapKind::FailedLayer,
            format!("adapter:{}/{}", self.adapter, self.name),
        );
        gap.layer = Some(self.layer);
        gap.scope = self.scope.map(str::to_owned);
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
            Some(entry) => match open(&dir.join(format!("{}.jsonl", self.name))) {
                Err(_) => FailedDetail::FileMissing,
                Ok(file) => match read(BufReader::new(file), self.acceptance) {
                    Ok((rows, skipped)) => {
                        match self.deferred {
                            Some(deferred) => {
                                deferred.extend(entry.skipped.iter().flatten().cloned())
                            }
                            None => gaps.extend(skipped_files(entry, self.layer, &gap.src)),
                        }
                        gap.scope = None;
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

/// The fixed program, leading arguments and working directory of an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    program: PathBuf,
    args: Vec<OsString>,
    dir: PathBuf,
}

impl Invocation {
    /// Records the current working directory for both protocol commands.
    ///
    /// # Errors
    /// The program must be absolute and the current directory must be accessible.
    pub fn new(program: PathBuf, args: Vec<OsString>) -> io::Result<Self> {
        if !program.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "adapter program must be an absolute path",
            ));
        }
        Ok(Self {
            program,
            args,
            dir: std::env::current_dir()?,
        })
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args).current_dir(&self.dir);
        cmd
    }
}

/// An invocation whose manifest and protocol version have been accepted.
#[derive(Debug)]
pub struct Described {
    invocation: Invocation,
    manifest: AdapterManifest,
    acceptance: Acceptance,
}

/// Reads and validates the manifest printed by an adapter and selects the reader policy.
///
/// # Errors
/// Reports process or manifest errors and refuses incompatible contract versions.
pub fn describe(invocation: Invocation) -> Result<Described, AdapterError> {
    let describe = invocation
        .command()
        .arg("describe")
        .stderr(Stdio::inherit())
        .output()?;
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
    Ok(Described {
        invocation,
        manifest,
        acceptance,
    })
}

impl Described {
    /// The manifest used to choose this adapter.
    #[must_use]
    pub const fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    /// The invocation bound to this manifest.
    #[must_use]
    pub const fn invocation(&self) -> &Invocation {
        &self.invocation
    }

    /// Runs the described adapter and reads its index and optional layers.
    ///
    /// # Errors
    /// Reports process, protocol and base-layer failures.
    pub fn run(self, root: &Path, scratch_parent: &Path) -> Result<AdapterOutput, AdapterError> {
        self.run_with_layers(root, scratch_parent, LayerRequest::ALL)
    }

    /// Runs the adapter for the requested layers it offers.
    /// # Errors
    /// Reports process, protocol and base-layer failures.
    pub fn run_with_layers(
        self,
        root: &Path,
        scratch_parent: &Path,
        request: LayerRequest,
    ) -> Result<AdapterOutput, AdapterError> {
        let Self {
            invocation,
            manifest,
            acceptance,
        } = self;
        let passed: Vec<_> = request
            .iter()
            .filter(|layer| manifest.layers.contains(layer))
            .map(LayerName::as_str)
            .collect();
        let root = root.canonicalize()?;
        let parent = scratch_parent.canonicalize()?;
        if parent.starts_with(&root) {
            return Err(io::Error::other("adapter output would be inside source tree").into());
        }
        let dir = tempfile::tempdir_in(parent)?;
        let status = invocation
            .command()
            .arg("index")
            .arg("--root")
            .arg(&root)
            .arg("--out")
            .arg(dir.path())
            .arg("--layers")
            .arg(passed.join(","))
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!("index exited {status}")).into());
        }
        let result = read_result(dir.path(), &manifest.contract, &passed)?;
        for entry in result.layers.values() {
            for diagnostic in entry.diagnostics.iter().flatten() {
                if let Some(message) = &diagnostic.message {
                    eprintln!("{}: {message}", diagnostic.code);
                }
            }
        }
        let base = result.layers.get("base");
        let Some(base) = base.filter(|entry| entry.status == LayerStatus::Ok) else {
            let first = base.and_then(|entry| entry.diagnostics.as_deref()?.first());
            let reason = first.map_or_else(
                || "base layer failed".to_owned(),
                |d| format!("base layer failed: {}", d.code),
            );
            return Err(io::Error::other(reason).into());
        };
        let index = regular(&dir.path().join("index.scip"))?;
        let mut gaps = skipped_files(
            base,
            Layer::Base,
            &format!("adapter:{}/base", manifest.name),
        )
        .collect();
        let (syntax_state, lines) = OptionalLayer {
            adapter: &manifest.name,
            acceptance,
            name: "syntax",
            layer: Layer::Syntax,
            offered: manifest.layers.contains(&LayerName::Syntax),
            requested: request.contains(LayerName::Syntax),
            scope: None,
            deferred: None,
        }
        .read(dir.path(), &result, &mut gaps, regular, read_syntax);
        let (calls_state, calls) = OptionalLayer {
            adapter: &manifest.name,
            acceptance,
            name: "calls",
            layer: Layer::Calls,
            offered: manifest.layers.contains(&LayerName::Calls),
            requested: request.contains(LayerName::Calls),
            scope: None,
            deferred: None,
        }
        .read(dir.path(), &result, &mut gaps, regular, read_calls);
        let mut build_skipped = Vec::new();
        let (build_state, build) = OptionalLayer {
            adapter: &manifest.name,
            acceptance,
            name: "build",
            layer: Layer::Build,
            offered: manifest.layers.contains(&LayerName::Build),
            requested: request.contains(LayerName::Build),
            scope: Some(crate::graph::SCOPE),
            deferred: Some(&mut build_skipped),
        }
        .read(dir.path(), &result, &mut gaps, regular, read_build);
        let mut syntax = Vec::new();
        let mut tests = Vec::new();
        for line in lines {
            match line {
                SyntaxLine::Fact(fact) => syntax.push(fact),
                SyntaxLine::Test(test) => tests.push(test),
            }
        }
        let builds = result.builds.unwrap_or_default();
        let merged = base.merged.clone().unwrap_or_default();
        Ok(AdapterOutput {
            index,
            manifest,
            builds,
            layers: Layers {
                base: LayerState::Present,
                syntax: syntax_state,
                calls: calls_state,
                build: build_state,
            },
            syntax,
            tests,
            calls,
            build,
            build_skipped,
            merged,
            gaps,
            _directory: dir,
        })
    }
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
            serde_json::from_value(json!({"contract":"1.1","layers":{"syntax":{"status":"ok","skipped":[{"file":"omitted","code":"unreadable"}]}}}))
                .unwrap();
        let layer = OptionalLayer {
            name: "syntax",
            layer: Layer::Syntax,
            offered: true,
            requested: true,
            acceptance: Acceptance::SkipUnknown,
            adapter: "fake",
            scope: None,
            deferred: None,
        };
        let future = r#"{"anchor":{"file":"x","line":0,"col":0},"outline":[{"k":"future","range":[0,0,0,1]}]}"#;
        let known = r#"{"anchor":{"file":"x","line":0,"col":0}}"#;
        fs::write(&file, format!("{future}\n{known}\n{future}\n")).unwrap();
        let mut gaps = Vec::new();
        let (state, rows) = layer.read(dir.path(), &result, &mut gaps, regular, read_syntax);
        assert_eq!((state, rows.len()), (LayerState::Present, 1));
        let layer = OptionalLayer {
            name: "syntax",
            layer: Layer::Syntax,
            offered: true,
            requested: true,
            acceptance: Acceptance::SkipUnknown,
            adapter: "fake",
            scope: None,
            deferred: None,
        };
        assert_eq!(gaps.len(), 2);
        assert!(
            gaps.iter()
                .any(|g| g.gap == GapKind::SkippedFile && g.file.as_deref() == Some("omitted"))
        );
        let unknown = gaps
            .iter()
            .find(|g| g.gap == GapKind::UnknownValue)
            .unwrap();
        assert_eq!(unknown.count, Some(2));
        assert_eq!(unknown.src, "adapter:fake/syntax");
        fs::write(file, format!("{future}\n{known}\n{{}}\n")).unwrap();
        gaps.clear();
        let (state, rows) = layer.read(dir.path(), &result, &mut gaps, regular, read_syntax);
        assert_eq!((state, rows.len()), (LayerState::Failed, 0));
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].detail, Some(FailedDetail::RowInvalid));
    }

    #[test]
    fn optional_layer_opens_its_file_only_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("syntax.jsonl");
        fs::write(&file, "").unwrap();
        let result = serde_json::from_value(json!({
            "contract": CONTRACT_VERSION.to_string(),
            "layers": {"syntax": {"status":"ok"}},
        }))
        .unwrap();
        for (requested, expected) in [
            (false, LayerState::NotRequested),
            (true, LayerState::Present),
        ] {
            let mut opened = Vec::new();
            let mut gaps = Vec::new();
            let (state, rows) = OptionalLayer {
                adapter: "probe",
                acceptance: Acceptance::Full,
                name: "syntax",
                layer: Layer::Syntax,
                offered: true,
                requested,
                scope: None,
                deferred: None,
            }
            .read(
                dir.path(),
                &result,
                &mut gaps,
                |path| {
                    opened.push(path.to_path_buf());
                    regular(path)
                },
                read_syntax,
            );
            assert_eq!(state, expected);
            assert!(rows.is_empty() && gaps.is_empty());
            assert_eq!(
                opened,
                requested
                    .then(|| file.clone())
                    .into_iter()
                    .collect::<Vec<_>>()
            );
        }
    }
}
