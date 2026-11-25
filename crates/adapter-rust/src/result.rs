use gosanja_adapter_rust::calls::Outcome;
use gosanja_schema::contract::{
    AdapterManifest, AdapterResult, CONTRACT_VERSION, Diagnostic, LayerName, LayerResult,
    LayerStatus, RESULT_FILE, Requirement,
};
use gosanja_schema::position::PositionEncoding;
use gosanja_schema::snapshot::Tool;
use std::{collections::BTreeMap, fs::File, io, path::Path};

pub fn manifest() -> AdapterManifest {
    AdapterManifest {
        contract: CONTRACT_VERSION.to_string(),
        name: "rust".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        languages: vec!["rust".into()],
        position_encoding: PositionEncoding::Utf32,
        layers: vec![LayerName::Base, LayerName::Syntax, LayerName::Calls],
        requires: Some(
            ["rust-analyzer", "cargo"]
                .map(|name| Requirement {
                    tool: name.into(),
                    version: None,
                })
                .to_vec(),
        ),
        markers: Some(vec!["Cargo.toml".into()]),
    }
}
pub struct Problem {
    pub code: &'static str,
    pub detail: String,
}
impl Problem {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}
pub fn failed(code: &str) -> LayerResult {
    LayerResult {
        status: LayerStatus::Failed,
        diagnostics: Some(vec![Diagnostic {
            code: code.into(),
            message: None,
        }]),
        skipped: None,
        merged: None,
    }
}
/// What the requested layers gave.
pub struct Layers {
    pub base: Result<(), Problem>,
    pub syntax: Option<LayerResult>,
    pub calls: Option<Outcome>,
}

pub fn publish(out: &Path, layers: Layers, mut tools: Vec<Tool>) -> io::Result<()> {
    let Layers {
        base,
        syntax,
        calls,
    } = layers;
    let base = match base {
        Ok(()) => LayerResult {
            status: LayerStatus::Ok,
            diagnostics: None,
            skipped: None,
            merged: None,
        },
        Err(p) => {
            eprintln!("{}", p.code);
            if !p.detail.is_empty() {
                eprintln!("{}", p.detail);
            }
            failed(p.code)
        }
    };
    let mut entries = BTreeMap::from([("base".into(), base)]);
    if let Some(syntax) = syntax {
        entries.insert("syntax".into(), syntax);
    }
    let builds = calls.and_then(|calls| {
        if let Some(version) = calls.rustc {
            tools.push(Tool {
                name: "rustc".into(),
                version,
            });
        }
        entries.insert("calls".into(), calls.layer);
        calls.builds
    });
    let result = AdapterResult {
        contract: CONTRACT_VERSION.to_string(),
        layers: entries,
        tools: (!tools.is_empty()).then_some(tools),
        builds,
    };
    serde_json::to_writer(File::create(out.join(RESULT_FILE))?, &result)?;
    Ok(())
}
