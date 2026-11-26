use super::capture::Capture;
use crate::project::Project;
use gosanja_adapter_rust::build::dep_info;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(super) struct Graph {
    pub root: PathBuf,
    pub documents: BTreeSet<String>,
    pub package_names: BTreeMap<String, String>,
}

struct Unit<'a> {
    artifact: &'a Value,
    package: &'a Value,
    source: PathBuf,
    cwd: &'a Path,
    args: Vec<&'a str>,
    depfile: PathBuf,
    outputs: Vec<PathBuf>,
    vars: Value,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn text<'a>(object: &'a Value, field: &str) -> io::Result<&'a str> {
    object[field]
        .as_str()
        .ok_or_else(|| invalid(format!("missing Cargo field {field}")))
}

fn strings(value: &Value) -> io::Result<Vec<&str>> {
    value
        .as_array()
        .ok_or_else(|| invalid("expected a JSON array"))?
        .iter()
        .map(|value| value.as_str().ok_or_else(|| invalid("expected a string")))
        .collect()
}

fn flag<'a, 'b>(args: &'b [&'a str], name: &'b str) -> impl Iterator<Item = &'a str> + 'b {
    args.iter().enumerate().filter_map(move |(index, arg)| {
        if *arg == name {
            args.get(index + 1).copied()
        } else {
            arg.strip_prefix(name)?.strip_prefix('=')
        }
    })
}

fn required_flag<'a>(args: &[&'a str], name: &str) -> io::Result<&'a str> {
    flag(args, name)
        .next()
        .ok_or_else(|| invalid(format!("missing rustc {name}")))
}

impl<'a> Unit<'a> {
    fn read(
        record: &'a Value,
        artifacts: &BTreeMap<PathBuf, Vec<&'a Value>>,
        packages: &BTreeMap<&str, &'a Value>,
    ) -> io::Result<Self> {
        let args = strings(&record["args"])?;
        let cwd = Path::new(text(record, "cwd")?);
        let out = Path::new(required_flag(&args, "--out-dir")?);
        let name = required_flag(&args, "--crate-name")?;
        let suffix = flag(&args, "-C")
            .find_map(|option| option.strip_prefix("extra-filename="))
            .unwrap_or_default();
        let stem = format!("{name}{suffix}");
        let mut matches = Vec::new();
        for artifact in artifacts.get(out).into_iter().flatten() {
            let target = &artifact["target"];
            let source = PathBuf::from(text(target, "src_path")?);
            if !args.iter().any(|arg| cwd.join(arg) == source) {
                continue;
            }
            let script = target["kind"][0] == "custom-build";
            let outputs: Vec<PathBuf> = strings(&artifact["filenames"])?
                .into_iter()
                .map(PathBuf::from)
                .filter(|path| {
                    path.parent() == Some(out)
                        && (script
                            || path.file_stem().and_then(|part| part.to_str()).is_some_and(
                                |part| part == stem || part.strip_prefix("lib") == Some(&stem),
                            ))
                })
                .collect();
            if !outputs.is_empty() {
                matches.push((*artifact, source, outputs));
            }
        }
        let [(artifact, source, outputs)] = matches.as_mut_slice() else {
            return Err(invalid(format!(
                "cannot match rustc unit {stem} to one Cargo artifact"
            )));
        };
        let package = *packages
            .get(text(artifact, "package_id")?)
            .ok_or_else(|| invalid("Cargo artifact is missing package metadata"))?;
        Ok(Self {
            artifact,
            package,
            source: std::mem::take(source),
            cwd,
            args,
            depfile: out.join(format!("{stem}.d")),
            outputs: std::mem::take(outputs),
            vars: record["env"].clone(),
        })
    }

    fn proc_macro(&self) -> io::Result<Option<&Path>> {
        if self.artifact["target"]["kind"][0] != "proc-macro" || self.args.contains(&"--test") {
            return Ok(None);
        }
        self.outputs
            .iter()
            .find(|path| {
                path.extension()
                    .is_some_and(|ext| matches!(ext.to_str(), Some("so" | "dll" | "dylib")))
                    && path.is_file()
            })
            .map(|path| Some(path.as_path()))
            .ok_or_else(|| invalid("missing proc-macro library"))
    }

    fn cfg(&self) -> BTreeSet<&str> {
        let mut flags: BTreeSet<_> = flag(&self.args, "--cfg").collect();
        if self.args.contains(&"--test") {
            flags.insert("test");
        }
        flags
    }

    fn priority(&self) -> (bool, bool, &str) {
        let target = &self.artifact["target"];
        (
            self.args.contains(&"--test"),
            !(target["kind"][0] == "lib" || target["kind"][0] == "proc-macro"),
            target["name"].as_str().unwrap_or_default(),
        )
    }
}

fn cargo_units<'a>(capture: &'a Capture, metadata: &'a Value) -> io::Result<Vec<Unit<'a>>> {
    let mut artifacts = BTreeMap::<PathBuf, Vec<_>>::new();
    let mut count = 0;
    for artifact in capture
        .messages
        .iter()
        .filter(|item| item["reason"] == "compiler-artifact")
    {
        count += 1;
        let mut parents = BTreeSet::new();
        for filename in strings(&artifact["filenames"])? {
            if let Some(parent) = Path::new(filename).parent()
                && parents.insert(parent)
            {
                artifacts
                    .entry(parent.to_owned())
                    .or_default()
                    .push(artifact);
            }
        }
    }
    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| invalid("Cargo metadata has no packages"))?
        .iter()
        .map(|package| Ok((text(package, "id")?, package)))
        .collect::<io::Result<BTreeMap<_, _>>>()?;
    let mut units = capture
        .records
        .iter()
        .map(|record| Unit::read(record, &artifacts, &packages))
        .collect::<io::Result<Vec<_>>>()?;
    if units.len() != count {
        return Err(invalid(format!(
            "captured {} of {count} Cargo units",
            units.len()
        )));
    }
    units.sort_by(|a, b| {
        (
            a.package["name"].as_str(),
            a.package["version"].as_str(),
            a.package["id"].as_str(),
            a.priority(),
            &a.source,
            a.cfg(),
            &a.depfile,
        )
            .cmp(&(
                b.package["name"].as_str(),
                b.package["version"].as_str(),
                b.package["id"].as_str(),
                b.priority(),
                &b.source,
                b.cfg(),
                &b.depfile,
            ))
    });
    Ok(units)
}

fn copy_sources(unit: &Unit<'_>, origin: &Path, view: &Path) -> io::Result<()> {
    let input = fs::read_to_string(&unit.depfile)?;
    let mut outputs = vec![unit.depfile.clone()];
    if unit.artifact["target"]["kind"][0] == "custom-build" {
        outputs.push(unit.depfile.with_extension(std::env::consts::EXE_EXTENSION));
    } else {
        outputs.extend_from_slice(&unit.outputs);
    }
    let prerequisites = dep_info(&input, &outputs).ok_or_else(|| {
        invalid(format!(
            "cannot read rustc source inputs for {}",
            unit.source.display()
        ))
    })?;
    for path in prerequisites {
        let original = unit.cwd.join(path).canonicalize()?;
        if let Ok(relative) = original.strip_prefix(origin) {
            let destination = view.join(relative);
            fs::create_dir_all(
                destination
                    .parent()
                    .ok_or_else(|| invalid("missing source parent"))?,
            )?;
            fs::copy(&original, destination)?;
        }
    }
    Ok(())
}

fn dependencies(unit: &Unit<'_>, by_path: &BTreeMap<&Path, usize>) -> io::Result<Vec<Value>> {
    flag(&unit.args, "--extern")
        .filter(|value| *value != "proc_macro")
        .map(|value| {
            let (name, file) = value
                .split_once('=')
                .ok_or_else(|| invalid("unqualified rustc dependency"))?;
            let name = name.rsplit(':').next().unwrap_or(name);
            let path = unit.cwd.join(file);
            let dependency = by_path
                .get(path.as_path())
                .ok_or_else(|| invalid(format!("no rustc unit for {}", path.display())))?;
            Ok(json!({"crate": dependency, "name": name}))
        })
        .collect()
}

fn crate_vars(
    unit: &Unit<'_>,
    cfg: &BTreeSet<&str>,
    build_scripts: &BTreeMap<&str, &Value>,
) -> io::Result<Value> {
    let mut vars = unit.vars.clone();
    if let Some(script) = vars["OUT_DIR"]
        .as_str()
        .and_then(|out| build_scripts.get(out))
    {
        if script["package_id"] != unit.artifact["package_id"]
            || strings(&script["cfgs"])?
                .iter()
                .any(|flag| !cfg.contains(flag))
        {
            return Err(invalid(
                "Cargo build-script output does not match rustc unit",
            ));
        }
        for entry in script["env"].as_array().into_iter().flatten() {
            let parts = strings(entry)?;
            let [key, value] = parts.as_slice() else {
                return Err(invalid("invalid build-script environment entry"));
            };
            vars[*key] = json!(value);
        }
    }
    Ok(vars)
}

fn sysroot(project: &Project<'_>, capture: &Capture) -> io::Result<(PathBuf, PathBuf)> {
    let compilers = capture
        .records
        .iter()
        .map(|record| text(record, "compiler"))
        .collect::<io::Result<BTreeSet<_>>>()?;
    if compilers.len() != 1 {
        return Err(invalid(
            "Cargo used more than one compiler for split contexts",
        ));
    }
    let compiler = compilers
        .first()
        .ok_or_else(|| invalid("Cargo did not report a compiler"))?;
    let output = project.run(compiler, &["--print", "sysroot"])?;
    if !output.status.success() {
        return Err(invalid("could not read the Cargo toolchain sysroot"));
    }
    let sysroot = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let sysroot_src = sysroot.join("lib/rustlib/src/rust/library");
    if !sysroot_src.is_dir() {
        return Err(invalid(
            "the Cargo toolchain needs rust-src to index split contexts",
        ));
    }
    Ok((sysroot, sysroot_src))
}

fn index_names(
    units: &[Unit<'_>],
    workspace: &Path,
) -> io::Result<(Vec<String>, BTreeMap<String, String>)> {
    let mut names = Vec::new();
    let mut published = BTreeMap::new();
    let mut primary = BTreeSet::new();
    let mut regular = BTreeMap::<_, String>::new();
    for unit in units {
        let package = text(unit.package, "name")?;
        let version = text(unit.package, "version")?;
        let stem = unit
            .depfile
            .file_stem()
            .and_then(|part| part.to_str())
            .ok_or_else(|| invalid("missing compiler artifact stem"))?;
        let name = format!("{package}__gosanja_unit_{}", stem.replace('-', "_"));
        let member = unit.source.starts_with(workspace);
        let test = unit.args.contains(&"--test");
        let mut cfg = unit.cfg();
        if test {
            cfg.remove("test");
        }
        let config = (
            text(unit.package, "id")?,
            &unit.source,
            cfg,
            flag(&unit.args, "--target").next(),
            unit.vars["OUT_DIR"].as_str(),
        );
        let public_name = if test
            && member
            && let Some(other) = regular.get(&config)
        {
            other.clone()
        } else if primary.insert((package, version, member.then_some(&unit.source))) {
            package.to_owned()
        } else {
            name.clone()
        };
        if !test && member {
            regular.entry(config).or_insert_with(|| public_name.clone());
        }
        if public_name != name {
            published.insert(name.clone(), public_name);
        }
        names.push(name);
    }
    Ok((names, published))
}

pub(super) fn write(
    project: &Project<'_>,
    work: &Path,
    metadata: &Value,
    capture: &Capture,
) -> io::Result<Graph> {
    let units = cargo_units(capture, metadata)?;
    let mut by_path = BTreeMap::new();
    for (index, unit) in units.iter().enumerate() {
        for path in &unit.outputs {
            if by_path.insert(path.as_path(), index).is_some() {
                return Err(invalid("artifact belongs to multiple units"));
            }
        }
    }
    let (sysroot, sysroot_src) = sysroot(project, capture)?;
    let root = work.join("rust-project");
    fs::create_dir(&root)?;
    let (names, package_names) = index_names(&units, project.root)?;
    let mut graph = Graph {
        root,
        documents: BTreeSet::new(),
        package_names,
    };
    let mut crates = Vec::new();
    let build_scripts = capture
        .messages
        .iter()
        .filter(|message| message["reason"] == "build-script-executed")
        .filter_map(|message| Some((message["out_dir"].as_str()?, message)))
        .collect();
    for (index, unit) in units.iter().enumerate() {
        let version = text(unit.package, "version")?;
        let workspace = unit.source.starts_with(project.root);
        let origin = if workspace {
            project.root
        } else {
            Path::new(text(unit.package, "manifest_path")?)
                .parent()
                .ok_or_else(|| invalid("missing Cargo package directory"))?
        };
        let dir = format!("unit-{index}");
        let view = if workspace { &graph.root } else { work }.join(&dir);
        let root_module = view.join(unit.source.strip_prefix(origin).map_err(io::Error::other)?);
        copy_sources(unit, origin, &view)?;
        if !root_module.is_file() {
            return Err(invalid("compiled crate root missing from source view"));
        }
        let name = &names[index];
        if workspace {
            graph.documents.insert(dir);
        }
        let cfg = unit.cfg();
        let vars = crate_vars(unit, &cfg, &build_scripts)?;
        let mut dirs = vec![view];
        if let Some(out_dir) = vars["OUT_DIR"].as_str() {
            dirs.push(out_dir.into());
        }
        let library = unit.proc_macro()?;
        crates.push(json!({
            "root_module": root_module, "edition": text(&unit.artifact["target"], "edition")?,
            "display_name": name, "version": version, "deps": dependencies(unit, &by_path)?,
            "cfg": cfg, "env": vars, "target": flag(&unit.args, "--target").next(),
            "is_workspace_member": workspace, "is_proc_macro": library.is_some(),
            "proc_macro_dylib_path": library, "proc_macro_cwd": unit.cwd,
            "source": {"include_dirs": dirs, "exclude_dirs": []}
        }));
    }
    serde_json::to_writer(
        fs::File::create(graph.root.join("rust-project.json"))?,
        &json!({
            "sysroot": sysroot, "sysroot_src": sysroot_src, "crates": crates,
        }),
    )?;
    Ok(graph)
}
