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
        layers: vec![LayerName::Base, LayerName::Syntax],
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
pub fn publish(
    out: &Path,
    syntax: Result<LayerResult, Problem>,
    tools: Vec<Tool>,
) -> io::Result<()> {
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
        layers: BTreeMap::from([("base".into(), base), ("syntax".into(), syntax)]),
        tools: (!tools.is_empty()).then_some(tools),
        builds: None,
    };
    serde_json::to_writer(File::create(out.join(RESULT_FILE))?, &result)?;
    Ok(())
}
