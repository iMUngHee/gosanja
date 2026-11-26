use crate::project::Project;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const RECORDS: &str = "GOSANJA_RUSTC_RECORDS";

pub(crate) fn wrapper() -> Option<io::Result<ExitCode>> {
    let records = PathBuf::from(env::var_os(RECORDS)?);
    Some(record_and_run(&records))
}

fn record_and_run(records: &Path) -> io::Result<ExitCode> {
    let mut invocation = env::args_os().skip(1);
    let compiler = invocation
        .next()
        .ok_or_else(|| io::Error::other("missing compiler path"))?;
    let args: Vec<OsString> = invocation.collect();
    // Cargo requests JSON diagnostics for its own compilations. A build script
    // can also invoke the wrapper through Cargo's RUSTC, for compiler probes.
    if args.iter().any(|arg| arg == "--error-format=json")
        && args.iter().any(|arg| {
            arg.to_str()
                .is_some_and(|arg| arg.starts_with("--emit=") && arg.contains("dep-info"))
        })
    {
        let dir = env::current_dir()?;
        let vars: BTreeMap<_, _> = env::vars_os()
            .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
            .filter(|(key, _)| {
                key.starts_with("CARGO_PKG_")
                    || matches!(
                        key.as_str(),
                        "OUT_DIR" | "CARGO_MANIFEST_DIR" | "CARGO_MANIFEST_PATH" | "CARGO_BIN_NAME"
                    )
            })
            .collect();
        let args = args
            .iter()
            .map(|arg| arg.to_str())
            .collect::<Option<Vec<_>>>();
        let path = records.join(format!("{}.json", std::process::id()));
        let writer = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        serde_json::to_writer(
            writer,
            &json!({ "cwd": dir, "compiler": compiler.to_str(),
                    "args": args, "env": vars }),
        )?;
    }
    let status = Command::new(compiler)
        .args(args)
        .env_remove(RECORDS)
        .status()?;
    Ok(ExitCode::from(
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
    ))
}

pub(super) struct Capture {
    pub messages: Vec<Value>,
    pub records: Vec<Value>,
}

fn split_contexts(messages: &[Value]) -> bool {
    let mut packages = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    let mut features = BTreeMap::new();
    for message in messages {
        if message["reason"] == "compiler-artifact" {
            let target = &message["target"];
            if let (Some(package), Some(source), Some(name)) = (
                message["package_id"].as_str(),
                target["src_path"].as_str(),
                target["name"].as_str(),
            ) {
                let current = &message["features"];
                if features
                    .insert((package, source, name), current)
                    .is_some_and(|prior| prior != current)
                {
                    return true;
                }
            }
            continue;
        }
        if message["reason"] != "build-script-executed" {
            continue;
        }
        if let (Some(package), Some(out)) =
            (message["package_id"].as_str(), message["out_dir"].as_str())
            && outputs.insert((package, out))
            && !packages.insert(package)
        {
            return true;
        }
    }
    false
}

pub(super) enum Build {
    Native,
    Replay(PathBuf),
    Split(Capture),
}

pub(super) fn collect(project: &Project<'_>, work: &Path, metadata: &Value) -> io::Result<Build> {
    let no_dependencies = metadata["resolve"]["nodes"]
        .as_array()
        .is_some_and(|nodes| {
            nodes
                .iter()
                .all(|node| node["deps"].as_array().is_some_and(Vec::is_empty))
        });
    let no_build_targets = metadata["packages"].as_array().is_some_and(|packages| {
        packages.iter().all(|package| {
            package["targets"].as_array().is_some_and(|targets| {
                targets.iter().all(|target| {
                    target["kind"][0] != "custom-build" && target["kind"][0] != "proc-macro"
                })
            })
        })
    });
    if no_dependencies && no_build_targets {
        return Ok(Build::Native);
    }
    let records = work.join("rustc-records");
    fs::create_dir(&records)?;
    let wrapper = env::current_exe()?;
    let target = work.join("unit-target");
    let build = work.join("unit-build");
    let output = project.run_with(
        "cargo",
        &[
            "check",
            "--workspace",
            "--all-targets",
            "--locked",
            "--message-format=json",
        ],
        &[
            (RECORDS, records.as_os_str()),
            ("CARGO_TARGET_DIR", target.as_os_str()),
            ("CARGO_BUILD_BUILD_DIR", build.as_os_str()),
            ("RUSTC_WRAPPER", wrapper.as_os_str()),
            ("RUSTC_WORKSPACE_WRAPPER", OsStr::new("")),
        ],
    )?;
    let messages: Vec<Value> = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect();
    if !split_contexts(&messages) {
        if output.status.success()
            && messages
                .iter()
                .any(|message| message["reason"] == "build-finished" && message["success"] == true)
        {
            let path = work.join("cargo-build-messages.jsonl");
            fs::write(&path, &output.stdout)?;
            return Ok(Build::Replay(path));
        }
        return Ok(Build::Native);
    }
    if !output.status.success() {
        return Err(io::Error::other(
            "Cargo check did not complete after reporting multiple build contexts",
        ));
    }
    let records = fs::read_dir(records)?
        .map(|entry| -> io::Result<Value> {
            Ok(serde_json::from_reader(fs::File::open(entry?.path())?)?)
        })
        .collect::<io::Result<_>>()?;
    Ok(Build::Split(Capture { messages, records }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_build_script_outputs_are_order_independent() {
        let mut messages = [
            json!({"reason":"build-script-executed","package_id":"shared","out_dir":"/out/host"}),
            json!({"reason":"build-script-executed","package_id":"shared","out_dir":"/out/target"}),
        ];
        assert!(split_contexts(&messages));
        messages.reverse();
        assert!(split_contexts(&messages));
        messages[1] = messages[0].clone();
        assert!(!split_contexts(&messages));
    }

    #[test]
    fn different_features_are_detected_without_a_build_script() {
        let build = |feature| {
            json!({
                "reason":"compiler-artifact", "package_id":"shared",
                "target":{"name":"shared","src_path":"/shared/src/lib.rs"}, "features":[feature]
            })
        };
        let first = build("host");
        let second = build("target");
        assert!(split_contexts(&[first.clone(), second]));
        assert!(!split_contexts(&[first.clone(), first]));
    }
}
