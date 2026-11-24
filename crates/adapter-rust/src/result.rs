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
fn failed(code: &str) -> LayerResult {
    LayerResult {
        status: LayerStatus::Failed,
        diagnostics: Some(vec![Diagnostic {
            code: code.into(),
            message: None,
        }]),
        skipped: None,
    }
}
/// What each layer gave.
pub struct Layers {
    /// The syntax layer's entry, or why the base layer failed.
    pub syntax: Result<LayerResult, Problem>,
    pub calls: Outcome,
}

pub fn publish(out: &Path, layers: Layers, mut tools: Vec<Tool>) -> io::Result<()> {
    let Layers { syntax, calls } = layers;
    if let Some(version) = calls.rustc {
        tools.push(Tool {
            name: "rustc".into(),
            version,
        });
    }
    let (base, syntax) = match syntax {
        Ok(layer) => (
            LayerResult {
                status: LayerStatus::Ok,
                diagnostics: None,
                skipped: None,
            },
            layer,
        ),
        Err(p) => {
            eprintln!("{}", p.code);
            if !p.detail.is_empty() {
                eprintln!("{}", p.detail);
            }
            (failed(p.code), failed("base-failed"))
        }
    };
    let result = AdapterResult {
        contract: CONTRACT_VERSION.to_string(),
        layers: BTreeMap::from([
            ("base".into(), base),
            ("calls".into(), calls.layer),
            ("syntax".into(), syntax),
        ]),
        tools: (!tools.is_empty()).then_some(tools),
        builds: calls.builds,
    };
    serde_json::to_writer(File::create(out.join(RESULT_FILE))?, &result)?;
    Ok(())
}
