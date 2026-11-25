#![cfg(unix)]
mod support;
use gosanja_core::schemas::{Schema, validator};
use protobuf::Message;
use scip::types::{Document, Index};
use serde_json::{Value, json};
use std::{
    fmt::Write as _,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
};
use support::{Case, listing};

fn failed(c: &Case, cmd: &mut Command, code: &str, tools: &[&str]) -> Vec<u8> {
    let before = listing(&c.root);
    let o = cmd.output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        String::from_utf8_lossy(&o.stderr).contains(code),
        "{code}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    if code == "index-incomplete" && c.root.join("third/src/lib.rs").exists() {
        assert!(String::from_utf8_lossy(&o.stderr).contains("third/src/lib.rs"));
    }
    let bytes = fs::read(c.out.join("result.json")).unwrap();
    let r: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(validator(Schema::AdapterResult).is_valid(&r), "{r}");
    assert_eq!(
        r["layers"]["base"],
        json!({"status":"failed","diagnostics":[{"code":code}]})
    );
    let names: Vec<_> = r["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, tools);
    assert_eq!(r.get("tools").is_some(), !tools.is_empty());
    assert!(!c.out.join("index.scip").exists());
    assert!(!c.out.join("syntax.jsonl").exists());
    for layer in ["syntax", "calls"] {
        assert_eq!(
            r["layers"][layer],
            json!({"status":"failed","diagnostics":[{"code":"base-failed"}]})
        );
    }
    assert!(r.get("builds").is_none());
    assert!(!c.out.join("calls.jsonl").exists());
    assert_eq!(before, listing(&c.root));
    bytes
}

#[test]
fn manifest() {
    let c = Case::new();
    let o = c.adapter().arg("describe").output().unwrap();
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(validator(Schema::AdapterManifest).is_valid(&r), "{r}");
    assert_eq!(
        r,
        json!({"name":"rust","version":env!("CARGO_PKG_VERSION"),"contract":"0.4","languages":["rust"],"layers":["base","syntax","calls"],"position_encoding":"utf-32","markers":["Cargo.toml"],"requires":[{"tool":"rust-analyzer"},{"tool":"cargo"}]})
    );
}
#[test]
fn invalid_invocation() {
    let c = Case::new();
    let missing = c.temp.path().join("missing");
    let link = c.temp.path().join("link");
    let nested = c.root.join("empty");
    fs::create_dir(&nested).unwrap();
    symlink(&nested, &link).unwrap();
    let file = c.root.join("Cargo.toml");
    for (root, out) in [
        (&missing, &c.out),
        (&file, &c.out),
        (&c.root, &missing),
        (&c.root, &file),
        (&c.root, &c.bin),
        (&c.root, &c.root),
        (&c.root, &nested),
        (&c.root, &link),
    ] {
        let before = listing(c.temp.path());
        let o = c
            .adapter()
            .arg("index")
            .arg("--out")
            .arg(out)
            .arg("--root")
            .arg(root)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        let invalid = if root == &c.root { out } else { root };
        assert!(String::from_utf8_lossy(&o.stderr).contains(invalid.to_str().unwrap()));
        assert_eq!(listing(c.temp.path()), before, "{root:?} -> {out:?}");
    }
}
#[test]
fn cli_options_and_order() {
    let c = Case::new();
    for (args, reverse) in [
        (["--root", "--bogus"], false),
        (["--bogus", "--out"], false),
        (["--out", "--bogus"], true),
        (["--bogus", "--root"], true),
    ] {
        let (first, second) = if reverse {
            (&c.out, &c.root)
        } else {
            (&c.root, &c.out)
        };
        let o = c
            .adapter()
            .arg("index")
            .arg(args[0])
            .arg(first)
            .arg(args[1])
            .arg(second)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2), "{args:?}");
    }
    assert_eq!(
        c.adapter().arg("unknown").output().unwrap().status.code(),
        Some(2)
    );
    assert_eq!(
        c.run(
            c.adapter()
                .arg("index")
                .arg("--out")
                .arg(&c.out)
                .arg("--root")
                .arg(&c.root)
        )["layers"]["base"]["status"],
        "ok"
    );
}

#[test]
fn retains_tool_stderr_outside_the_result() {
    let c = Case::new();
    c.stub("if [ \"$1\" = --version ]; then echo 'rust-analyzer test'; else echo 'test indexer error' >&2; exit 1; fi");
    let o = c.index().output().unwrap();
    assert!(o.status.success());
    let stderr = String::from_utf8(o.stderr).unwrap();
    assert!(stderr.contains("indexer-failed\ntest indexer error"));
    let bytes = fs::read_to_string(c.out.join("result.json")).unwrap();
    assert!(!bytes.contains("test indexer error"));
}
#[test]
fn unwritable_output() {
    if Command::new("id").arg("-u").output().unwrap().stdout == b"0\n" {
        eprintln!("skipping permission case as super-user");
        return;
    }
    let c = Case::new();
    fs::set_permissions(&c.out, fs::Permissions::from_mode(0o555)).unwrap();
    let o = c.index().output().unwrap();
    fs::set_permissions(&c.out, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert_ne!(o.stderr, b"");
}
fn record_probes(c: &Case) {
    let real = c.bin.join("real");
    fs::create_dir(&real).unwrap();
    let cargo = c.bin.join("cargo");
    fs::rename(&cargo, real.join("cargo")).unwrap();
    fs::write(
        &cargo,
        r#"#!/bin/sh
set -eu
if [ "$1" = --version ]; then echo 'cargo --version' >> "$HOME/probes"; fi
exec "${0%/*}/real/cargo" "$@"
"#,
    )
    .unwrap();
    fs::set_permissions(cargo, fs::Permissions::from_mode(0o755)).unwrap();
    c.stub(
        r#"echo "rust-analyzer $*" >> "$HOME/probes"
if [ "$1" = --version ]; then echo 'rust-analyzer test'; else exit 1; fi"#,
    );
}

fn readonly_failure(c: &Case, cmd: &mut Command, code: &str, tools: &[&str]) -> Vec<u8> {
    let entries = listing(&c.root);
    for (path, (mode, _)) in &entries {
        fs::set_permissions(c.root.join(path), fs::Permissions::from_mode(mode & !0o222)).unwrap();
    }
    let log = c.temp.path().join("home/probes");
    fs::write(&log, "").unwrap();
    let bytes = failed(c, cmd, code, tools);
    let expected: Vec<_> = tools
        .iter()
        .map(|tool| format!("{tool} --version"))
        .collect();
    assert_eq!(
        fs::read_to_string(log).unwrap().lines().collect::<Vec<_>>(),
        expected,
        "{code}"
    );
    for (path, (mode, _)) in entries {
        fs::set_permissions(c.root.join(path), fs::Permissions::from_mode(mode)).unwrap();
    }
    bytes
}

#[test]
fn preconditions() {
    for (scenario, code, n) in [
        ("manifest", "manifest-missing", 0),
        ("tool", "toolchain-missing", 1),
        ("invalid", "manifest-invalid", 2),
        ("member", "not-workspace-root", 2),
        ("lock", "lockfile-missing", 2),
        ("stale", "locked-metadata-failed", 2),
    ] {
        let mut c = Case::new();
        record_probes(&c);
        match scenario {
            "manifest" => fs::remove_file(c.root.join("Cargo.toml")).unwrap(),
            "tool" => fs::remove_file(c.bin.join("rust-analyzer")).unwrap(),
            "invalid" => fs::write(c.root.join("Cargo.toml"), "[").unwrap(),
            "member" => c.root = c.root.join("app"),
            "lock" => fs::remove_file(c.root.join("Cargo.lock")).unwrap(),
            _ => {
                let p = c.root.join("app/Cargo.toml");
                fs::write(
                    &p,
                    fs::read_to_string(&p).unwrap().replace("0.1.0", "0.2.0"),
                )
                .unwrap();
            }
        }
        let tools = &["cargo", "rust-analyzer"][..n];
        let bytes = readonly_failure(&c, &mut c.index(), code, tools);
        if scenario == "stale" {
            fs::remove_dir_all(&c.out).unwrap();
            fs::create_dir(&c.out).unwrap();
            assert_eq!(bytes, readonly_failure(&c, &mut c.index(), code, tools));
        }
    }
}
#[test]
fn tool_home_paths() {
    for kind in [
        "inside",
        "relative",
        "symlink",
        "registry",
        "git",
        "rustup",
        "backtrack",
        "non-directory",
        "dot",
    ] {
        let c = Case::new();
        let cache = c.temp.path().join("cargo-home");
        let (key, path) = match kind {
            "relative" => ("CARGO_HOME", "cache".into()),
            "symlink" => {
                let p = c.temp.path().join("linked-home");
                symlink(&c.root, &p).unwrap();
                ("CARGO_HOME", p)
            }
            "registry" | "git" => {
                symlink(&c.root, cache.join(kind)).unwrap();
                ("CARGO_HOME", cache)
            }
            "rustup" => ("RUSTUP_HOME", c.root.join("new")),
            "non-directory" => {
                let f = c.temp.path().join("regular");
                fs::write(&f, "").unwrap();
                ("CARGO_HOME", f.join("child"))
            }
            "backtrack" => ("CARGO_HOME", c.temp.path().join("new/../project/cache")),
            "dot" => ("CARGO_HOME", c.temp.path().join("new/./other")),
            _ => ("CARGO_HOME", c.root.clone()),
        };
        let code = if kind == "non-directory" {
            "tool-home-unresolvable"
        } else {
            "tool-home-inside-root"
        };
        record_probes(&c);
        readonly_failure(&c, c.index().env(key, path), code, &[]);
    }
}
#[test]
fn optional_tool_environment() {
    let keys = [
        "RUSTUP_TOOLCHAIN",
        "RUSTUP_DIST_SERVER",
        "RUSTUP_UPDATE_ROOT",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ];
    for supplied in [false, true] {
        let c = Case::new();
        c.stub(
            r#"if [ "$1" = --version ]; then echo 'rust-analyzer test'; else
            env > "$HOME/indexer-env"
            cp "$2/index.scip" "$4"
        fi"#,
        );
        let mut cmd = c.index();
        if supplied {
            for key in keys {
                cmd.env(key, format!("value-{key}"));
            }
        }
        assert_eq!(c.run(&mut cmd)["layers"]["base"]["status"], "ok");
        let log = fs::read_to_string(c.temp.path().join("home/indexer-env")).unwrap();
        for key in keys {
            let actual = log
                .lines()
                .filter_map(|line| line.split_once('='))
                .find(|(k, _)| *k == key);
            let value = format!("value-{key}");
            assert_eq!(actual, supplied.then_some((key, value.as_str())), "{key}");
        }
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "each mode's environment and writes are checked side by side"
)]
fn child_environment_and_workspace_writes() {
    for mode in ["readonly", "configured", "hostile"] {
        let c = Case::new();
        let original = listing(&c.root);
        if mode == "readonly" {
            for (p, (m, _)) in &original {
                fs::set_permissions(c.root.join(p), fs::Permissions::from_mode(m & !0o222))
                    .unwrap();
            }
        }
        if mode == "configured" {
            fs::create_dir(c.root.join(".cargo")).unwrap();
            fs::write(
                c.root.join(".cargo/config.toml"),
                "[build]\ntarget-dir='configured-target'\nbuild-dir='configured-build'\n",
            )
            .unwrap();
        }
        let record = c.temp.path().join("record");
        let quoted = record.to_str().unwrap().replace('\'', "'\\''");
        c.stub(&format!("printf '%s\\n' \"$PWD\" \"$@\" >> '{quoted}'\nenv >> '{quoted}'\nif [ \"$1\" = --version ]; then echo 'rust-analyzer test'; else cp \"$2/index.scip\" \"$4\"; fi"));
        let mut cmd = c.index();
        cmd.env("CARGO_HOME", c.temp.path().join("new-cache"))
            .env("RUSTUP_HOME", c.temp.path().join("new-rustup"));
        if mode == "hostile" {
            for key in [
                "RA_LOG",
                "RA_LOG_FILE",
                "TMPDIR",
                "RUSTFLAGS",
                "CARGO_TARGET_DIR",
            ] {
                cmd.env(key, c.root.join("bad"));
            }
        }
        let before = listing(&c.root);
        let r = c.run(&mut cmd);
        assert_eq!(r["layers"]["base"]["status"], "ok", "{r}");
        assert_eq!(r["tools"][1]["version"], "test");
        assert_eq!(r["layers"]["syntax"], json!({"status":"ok"}));
        assert!(fs::metadata(c.out.join("syntax.jsonl")).unwrap().len() > 0);
        assert_eq!(
            fs::read(c.root.join("index.scip")).unwrap(),
            fs::read(c.out.join("index.scip")).unwrap()
        );
        let log = fs::read_to_string(record).unwrap();
        assert!(log.contains(&format!(
            "{}\nscip\n.\n--output\n{}\n",
            c.root.display(),
            c.out.join("work/index.scip").display()
        )));
        for (k, dir) in [
            ("CARGO_TARGET_DIR", "target"),
            ("CARGO_BUILD_BUILD_DIR", "build"),
            ("TMPDIR", "tmp"),
        ] {
            let p = c.out.join("work").join(dir);
            assert!(p.is_dir());
            assert!(log.contains(&format!("{k}={}\n", p.display())));
        }
        for key in log
            .lines()
            .filter_map(|s| s.split_once('=').map(|(k, _)| k))
        {
            assert!(
                [
                    "PWD",
                    "_",
                    "SHLVL",
                    "HOME",
                    "PATH",
                    "TMPDIR",
                    "CARGO_TARGET_DIR",
                    "CARGO_BUILD_BUILD_DIR",
                    "RUSTUP_AUTO_INSTALL",
                    "CARGO_HOME",
                    "RUSTUP_HOME"
                ]
                .contains(&key),
                "{key}"
            );
        }
        assert!(log.contains("RUSTUP_AUTO_INSTALL=0\n"));
        for (key, dir) in [("CARGO_HOME", "new-cache"), ("RUSTUP_HOME", "new-rustup")] {
            assert!(log.contains(&format!("{key}={}\n", c.temp.path().join(dir).display())));
        }
        let mut published: Vec<_> = fs::read_dir(&c.out)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        published.sort();
        assert_eq!(
            published,
            [
                "calls.jsonl",
                "index.scip",
                "result.json",
                "syntax.jsonl",
                "work"
            ]
        );

        assert_eq!(before, listing(&c.root));
        for (p, (mode, _)) in original {
            fs::set_permissions(c.root.join(p), fs::Permissions::from_mode(mode)).unwrap();
        }
    }
}
#[test]
fn index_validation() {
    for scenario in [
        "third",
        "empty",
        "gated",
        "optional",
        "malformed",
        "absent",
        "exit",
    ] {
        let c = Case::new();
        let toml = c.root.join("Cargo.toml");
        let idx = c.root.join("index.scip");
        match scenario {
            "third" => {
                fs::write(
                    &toml,
                    fs::read_to_string(&toml)
                        .unwrap()
                        .replace("\"kit\"", "\"kit\",\"third\""),
                )
                .unwrap();
                fs::create_dir_all(c.root.join("third/src")).unwrap();
                fs::write(
                    c.root.join("third/Cargo.toml"),
                    "[package]\nname='third'\nversion='0.1.0'\nedition='2021'",
                )
                .unwrap();
                fs::write(c.root.join("third/src/lib.rs"), "").unwrap();
            }
            "empty" | "gated" | "optional" => {
                fs::write(toml,"[package]\nname='probe'\nversion='0.1.0'\nedition='2021'\n[features]\nopt=[]\n[[bin]]\nname='gated'\npath='src/bin.rs'\nrequired-features=['opt']\n").unwrap();
                fs::create_dir(c.root.join("src")).unwrap();
                fs::write(c.root.join("src/bin.rs"), "fn main() {}").unwrap();
                let mut index = Index::new();
                if scenario != "empty" {
                    let mut doc = Document::new();
                    doc.relative_path = if scenario == "optional" {
                        fs::write(c.root.join("src/lib.rs"), "").unwrap();
                        "src/lib.rs"
                    } else {
                        "src/bin.rs"
                    }
                    .to_owned();
                    index.documents.push(doc);
                }
                fs::write(idx, index.write_to_bytes().unwrap()).unwrap();
            }
            "malformed" => fs::write(idx, [0xff]).unwrap(),
            _ => {
                let exit = i32::from(scenario == "exit");
                c.stub(&format!("if [ \"$1\" = --version ]; then echo 'rust-analyzer test'; else exit {exit}; fi"));
            }
        }
        let o = c
            .command(&c.bin.join("cargo"))
            .current_dir(&c.root)
            .args(["generate-lockfile", "--offline"])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        match scenario {
            "optional" | "gated" => {
                assert_eq!(c.run(&mut c.index())["layers"]["base"]["status"], "ok");
            }
            s => {
                let code = match s {
                    "malformed" => "index-unreadable",
                    "absent" | "exit" => "indexer-failed",
                    _ => "index-incomplete",
                };
                failed(&c, &mut c.index(), code, &["cargo", "rust-analyzer"]);
            }
        }
    }
}
#[test]
fn failed_base_reaches_engine() {
    let c = Case::new();
    fs::remove_file(c.root.join("Cargo.lock")).unwrap();
    symlink(
        support::adapter_binary(),
        c.bin.join("gosanja-adapter-rust"),
    )
    .unwrap();
    let o = c
        .command(support::engine())
        .arg("index")
        .arg("--root")
        .arg(&c.root)
        .arg("--out")
        .arg(&c.out)
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("base layer failed: lockfile-missing"));
}

#[test]
fn repeatable_results_preserve_extractors_skipped_files() {
    let mut c = Case::new();
    fs::write(c.root.join("kit/src/lib.rs"), "fn invalid(").unwrap();
    let mut first = None;
    for name in ["first", "second"] {
        c.out = c.temp.path().join(name);
        fs::create_dir(&c.out).unwrap();
        let result = c.run(&mut c.index());
        assert_eq!(
            result["layers"]["syntax"],
            json!({"status":"ok", "skipped":[{"file":"kit/src/lib.rs","code":"parse-failed"}]})
        );
        let bytes = fs::read(c.out.join("result.json")).unwrap();
        if let Some(expected) = &first {
            assert_eq!(&bytes, expected);
        } else {
            first = Some(bytes);
        }
    }
}

#[test]
fn successful_result_bytes_match_between_output_directories() {
    let mut c = Case::new();
    c.run(&mut c.index());
    let first = fs::read(c.out.join("result.json")).unwrap();
    c.out = c.temp.path().join("second-out");
    fs::create_dir(&c.out).unwrap();
    let r = c.run(&mut c.index());
    assert_eq!(r["layers"]["syntax"]["status"], "ok");
    assert_eq!(first, fs::read(c.out.join("result.json")).unwrap());
}

#[test]
fn isolated_adapter_binary() {
    use std::os::unix::fs::MetadataExt;
    let cmd = Case::new().adapter();
    let path = std::path::Path::new(cmd.get_program());
    assert!(path.starts_with(env!("CARGO_TARGET_TMPDIR")));
    assert_eq!(
        fs::metadata(path).unwrap().ino(),
        fs::metadata(env!("CARGO_BIN_EXE_gosanja-adapter-rust"))
            .unwrap()
            .ino()
    );
}

// The calls layer. A probe package replaces the fixture: a library whose `caller` calls
// `leaf`, with an index written here, so that a row exists when the layer succeeds.

const PROBE_LIB: &str =
    "pub fn leaf() -> u32 {\n    1\n}\n\npub fn caller() -> u32 {\n    leaf()\n}\n";

/// A probe package at the case's root: `Cargo.toml` from `manifest` (a `[package]` named
/// `probe` is written before it), the given files, its lock file and an index of
/// `src/lib.rs` with `leaf`, `caller` and the reference between them.
fn probe(c: &Case, manifest: &str, files: &[(&str, &str)]) {
    fs::remove_dir_all(&c.root).unwrap();
    fs::create_dir_all(c.root.join("src")).unwrap();
    fs::write(
        c.root.join("Cargo.toml"),
        format!("[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{manifest}"),
    )
    .unwrap();
    fs::write(c.root.join("src/lib.rs"), PROBE_LIB).unwrap();
    for (path, text) in files {
        let path = c.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    lock(c);
    write_index(c, &[("src/lib.rs", probe_document())]);
}

fn lock(c: &Case) {
    let o = c
        .command(&c.bin.join("cargo"))
        .current_dir(&c.root)
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

fn write_index(c: &Case, documents: &[(&str, Vec<scip::types::Occurrence>)]) {
    let mut index = Index::new();
    for (path, occurrences) in documents {
        let mut doc = Document::new();
        doc.relative_path.push_str(path);
        doc.position_encoding =
            scip::types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into();
        doc.occurrences.clone_from(occurrences);
        // Every defined symbol is a function, as `rust-analyzer` reports one.
        let defined: std::collections::BTreeSet<&str> = occurrences
            .iter()
            .filter(|o| !o.enclosing_range.is_empty())
            .map(|o| o.symbol.as_str())
            .collect();
        for symbol in defined {
            let mut info = scip::types::SymbolInformation::new();
            info.symbol.push_str(symbol);
            info.kind = scip::types::symbol_information::Kind::Function.into();
            info.display_name.push_str(
                symbol
                    .rsplit(' ')
                    .next()
                    .unwrap()
                    .trim_end_matches("().")
                    .rsplit('/')
                    .next()
                    .unwrap(),
            );
            doc.symbols.push(info);
        }
        index.documents.push(doc);
    }
    fs::write(c.root.join("index.scip"), index.write_to_bytes().unwrap()).unwrap();
}

fn occurrence(
    symbol: &str,
    range: [i32; 3],
    enclosing: Option<[i32; 4]>,
) -> scip::types::Occurrence {
    let mut o = scip::types::Occurrence::new();
    o.symbol.push_str(symbol);
    o.range = range.to_vec();
    if let Some(e) = enclosing {
        o.symbol_roles = scip::types::SymbolRole::Definition as i32;
        o.enclosing_range = e.to_vec();
    }
    o
}

/// `leaf` and `caller` of `PROBE_LIB`, and the reference on `caller`'s body line.
fn probe_document() -> Vec<scip::types::Occurrence> {
    let leaf = "rust-analyzer cargo probe 0.1.0 leaf().";
    let caller = "rust-analyzer cargo probe 0.1.0 caller().";
    vec![
        occurrence(leaf, [0, 7, 11], Some([0, 0, 2, 1])),
        occurrence(caller, [4, 7, 13], Some([4, 0, 6, 1])),
        occurrence(leaf, [5, 4, 8], None),
    ]
}

/// Replaces `name` on the case's `PATH` by a script that runs `body` with `$REAL` the
/// program it replaces. The case's `PATH` holds few programs, so the script calls the
/// host's `cat`, `grep`, `sed` and `rm` by their paths.
fn wrap(c: &Case, name: &str, body: &str) {
    let real = fs::canonicalize(c.bin.join(name)).unwrap();
    let script = c.bin.join(format!("{name}.wrapper"));
    let mut tools = String::new();
    for tool in ["cat", "grep", "sed", "rm"] {
        let path = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|d| d.join(tool))
            .find(|p| p.is_file())
            .unwrap();
        writeln!(tools, "{tool}() {{ '{}' \"$@\"; }}", path.display()).unwrap();
    }
    fs::write(
        &script,
        format!("#!/bin/sh\n{tools}REAL='{}'\n{body}\n", real.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(script, c.bin.join(name)).unwrap();
}

/// Runs the adapter and gives `result.json`, after checking it against its schema.
fn calls_run(c: &Case) -> (Value, String) {
    let o = c.index().output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_slice(&fs::read(c.out.join("result.json")).unwrap()).unwrap();
    assert!(validator(Schema::AdapterResult).is_valid(&r), "{r}");
    (r, String::from_utf8_lossy(&o.stderr).into_owned())
}

/// The calls layer failed with `code` and left no rows and no build context; base and
/// syntax succeeded.
fn calls_failed(r: &Value, c: &Case, code: &str) {
    assert_eq!(
        r["layers"]["calls"],
        json!({"status": "failed", "diagnostics": [{"code": code}]}),
        "{r}"
    );
    assert_eq!(r["layers"]["base"]["status"], "ok", "{r}");
    assert_eq!(r["layers"]["syntax"]["status"], "ok", "{r}");
    assert!(r.get("builds").is_none(), "{r}");
    assert!(!c.out.join("calls.jsonl").exists());
}

fn calls_rows(c: &Case) -> Vec<Value> {
    fs::read_to_string(c.out.join("calls.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn tool_names(r: &Value) -> Vec<String> {
    r["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_probe_gives_its_row() {
    let c = Case::new();
    probe(&c, "", &[]);
    let log = c.temp.path().join("cargo-args");
    wrap(
        &c,
        "cargo",
        &format!("echo \"$*\" >> '{}'\nexec \"$REAL\" \"$@\"", log.display()),
    );
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    assert_eq!(tool_names(&r), ["cargo", "rust-analyzer", "rustc"]);
    let rustc = &r["tools"][2]["version"];
    assert!(rustc.as_str().unwrap().starts_with("1.99.1 ("), "{rustc}");
    // The calls build runs offline and locked, with the explicit IR path.
    let invoked = fs::read_to_string(&log).unwrap();
    let build: Vec<&str> = invoked
        .lines()
        .filter(|l| l.starts_with("rustc "))
        .collect();
    assert_eq!(build.len(), 1, "{invoked}");
    let ir = c.out.join("work/calls/0.ll");
    assert_eq!(
        build[0],
        format!(
            "rustc --locked --offline --message-format=json -p probe@0.1.0 --lib -- \
             --emit=llvm-ir={} -C codegen-units=1 -C debuginfo=2 -C opt-level=0",
            ir.display()
        )
    );
    assert_eq!(
        calls_rows(&c),
        [json!({
            "caller": {"file": "src/lib.rs", "line": 4, "col": 7},
            "callee": {"file": "src/lib.rs", "line": 0, "col": 7},
            "mechanism": "static",
            "resolution": "direct",
            "site": {"file": "src/lib.rs", "line": 5, "col": 4},
            "build": "probe-lib-probe"
        })]
    );
    let builds = r["builds"].as_array().unwrap();
    assert_eq!(builds.len(), 1);
    assert_eq!(builds[0]["id"], "probe-lib-probe");
    assert_eq!(builds[0]["profile"], "dev");
    assert_eq!(builds[0]["target"], "x86_64-unknown-linux-gnu");
    assert!(
        builds[0]["toolchain"]
            .as_str()
            .unwrap()
            .starts_with("rustc 1.99.1 (")
    );
    assert_eq!(builds[0]["features"], json!([]));
}

#[test]
fn calls_outcomes_before_any_module_is_read() {
    // A binary that does not compile.
    let c = Case::new();
    probe(&c, "", &[("src/main.rs", "fn main() { missing() }\n")]);
    write_index(
        &c,
        &[("src/lib.rs", probe_document()), ("src/main.rs", vec![])],
    );
    let (r, stderr) = calls_run(&c);
    calls_failed(&r, &c, "build-failed");
    assert!(stderr.contains("cargo fetch"), "{stderr}");

    // Only a binary whose required features `default` does not enable: nothing to build.
    let c = Case::new();
    probe(
        &c,
        "autolib = false\n[features]\nextra = []\n[[bin]]\nname = \"gated\"\npath = \"src/main.rs\"\nrequired-features = [\"extra\"]\n",
        &[("src/main.rs", "fn main() {}\n")],
    );
    write_index(&c, &[("src/main.rs", vec![])]);
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    assert!(r.get("builds").is_none(), "{r}");
    assert_eq!(fs::read(c.out.join("calls.jsonl")).unwrap(), b"");

    // A profile's optimisation level yields to the adapter's flag.
    let c = Case::new();
    probe(&c, "[profile.dev]\nopt-level = 3\n", &[]);
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    assert_eq!(calls_rows(&c).len(), 1);

    // Configured flags after the adapter's raise it: the module says so.
    let c = Case::new();
    probe(
        &c,
        "",
        &[(
            ".cargo/config.toml",
            "[build]\nrustflags = [\"-C\", \"opt-level=3\"]\n",
        )],
    );
    let (r, _) = calls_run(&c);
    calls_failed(&r, &c, "build-unsupported");
    assert_eq!(tool_names(&r), ["cargo", "rust-analyzer", "rustc"]);

    // Configured flags that send the IR elsewhere after the adapter's path.
    let c = Case::new();
    let elsewhere = c.temp.path().join("elsewhere.ll");
    probe(
        &c,
        "",
        &[(
            ".cargo/config.toml",
            &format!(
                "[build]\nrustflags = [\"--emit=llvm-ir={}\"]\n",
                elsewhere.display()
            ),
        )],
    );
    let (r, _) = calls_run(&c);
    calls_failed(&r, &c, "ir-missing");
}

/// A probe with a binary beside the library: the binary is built first, then the library.
fn probe_with_binary(c: &Case) {
    probe(
        c,
        "",
        &[("src/main.rs", "fn main() {\n    probe::caller();\n}\n")],
    );
    write_index(
        c,
        &[("src/lib.rs", probe_document()), ("src/main.rs", vec![])],
    );
}

/// Wraps `cargo` so that the `n`th `cargo rustc` (from 1) has its standard output passed
/// through `filter`, a shell pipeline over the JSON lines, and then runs `after`.
fn wrap_cargo_rustc(c: &Case, n: u32, filter: &str, after: &str) {
    let count = c.temp.path().join("rustc-count");
    wrap(
        c,
        "cargo",
        &format!(
            r#"if [ "$1" != rustc ]; then exec "$REAL" "$@"; fi
n=$(($(cat '{count}' 2>/dev/null || echo 0) + 1)); echo $n > '{count}'
if [ $n -ne {n} ]; then exec "$REAL" "$@"; fi
out=$("$REAL" "$@") || exit $?
printf '%s\n' "$out" | {filter}
{after}"#,
            count = count.display()
        ),
    );
}

/// The IR path the `n`th target (from 0) is given.
fn ir_path(c: &Case, n: u32) -> std::path::PathBuf {
    c.out.join(format!("work/calls/{n}.ll"))
}

#[test]
fn calls_outcomes_from_what_cargo_reports() {
    let artifacts = r#"grep '"reason":"compiler-artifact"'"#;
    // The second target's artifact message repeated, then dropped: no partial layer.
    for (filter, code) in [
        (
            format!("{{ cat; printf '%s\\n' \"$out\" | {artifacts}; }}"),
            "build-unsupported",
        ),
        (
            r#"grep -v '"reason":"compiler-artifact"'"#.to_owned(),
            "ir-missing",
        ),
    ] {
        let c = Case::new();
        probe_with_binary(&c);
        wrap_cargo_rustc(&c, 2, &filter, "");
        let (r, _) = calls_run(&c);
        calls_failed(&r, &c, code);
    }
    // Two artifact messages and no file: the count is checked first.
    let c = Case::new();
    probe(&c, "", &[]);
    let ir = ir_path(&c, 0);
    wrap_cargo_rustc(
        &c,
        1,
        &format!("{{ cat; printf '%s\\n' \"$out\" | {artifacts}; }}"),
        &format!("rm -f '{}'", ir.display()),
    );
    let (r, _) = calls_run(&c);
    calls_failed(&r, &c, "build-unsupported");

    // The offline build lacks a dependency that the metadata command let through.
    let c = Case::new();
    probe(&c, "", &[]);
    wrap(
        &c,
        "cargo",
        r#"if [ "$1" = rustc ]; then echo 'error: no matching package named `gone` found (offline)' >&2; exit 101; fi
exec "$REAL" "$@""#,
    );
    let (r, stderr) = calls_run(&c);
    calls_failed(&r, &c, "build-failed");
    assert!(stderr.contains("cargo fetch"), "{stderr}");
}

#[test]
fn calls_outcomes_from_the_emitted_module() {
    // A `rustc` that runs the real one and then cuts or removes the file it was told to
    // write. Cut before the compile unit, the module is unreadable and names no compiler.
    for (action, code) in [
        (r#"sed -i '/DICompileUnit/,$d' "$ir""#, "ir-unreadable"),
        (r#"rm -f "$ir""#, "ir-missing"),
    ] {
        let c = Case::new();
        probe(&c, "", &[]);
        wrap(
            &c,
            "rustc",
            &format!(
                r#""$REAL" "$@" || exit $?
for a in "$@"; do case "$a" in --emit=llvm-ir=*) ir="${{a#--emit=llvm-ir=}}"; {action};; esac; done
exit 0"#
            ),
        );
        let (r, _) = calls_run(&c);
        calls_failed(&r, &c, code);
        assert_eq!(tool_names(&r), ["cargo", "rust-analyzer"], "{r}");
    }
}

#[test]
fn calls_outcomes_decided_before_compiling() {
    // An output directory whose path holds a comma: no `cargo rustc` is run.
    let c = Case::new();
    probe(&c, "", &[]);
    let log = c.temp.path().join("cargo-args");
    wrap(
        &c,
        "cargo",
        &format!("echo \"$1\" >> '{}'\nexec \"$REAL\" \"$@\"", log.display()),
    );
    let out = c.temp.path().join("a,b/out");
    fs::create_dir_all(&out).unwrap();
    let o = c
        .adapter()
        .arg("index")
        .arg("--root")
        .arg(&c.root)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_slice(&fs::read(out.join("result.json")).unwrap()).unwrap();
    assert_eq!(
        r["layers"]["calls"],
        json!({"status": "failed", "diagnostics": [{"code": "build-unsupported"}]})
    );
    assert_eq!(r["layers"]["base"]["status"], "ok");
    assert_eq!(r["layers"]["syntax"]["status"], "ok");
    assert!(!out.join("calls.jsonl").exists());
    let invoked = fs::read_to_string(&log).unwrap();
    assert!(!invoked.lines().any(|l| l == "rustc"), "{invoked}");

    // Two targets that give one build context id, the first of which does not compile.
    let c = Case::new();
    fs::remove_dir_all(&c.root).unwrap();
    for (dir, manifest, path, text) in [
        (
            "a",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[[bin]]\nname = \"lib-x\"\npath = \"src/main.rs\"\n",
            "src/main.rs",
            "fn main() { missing() }\n",
        ),
        (
            "a-bin",
            "[package]\nname = \"a-bin\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nname = \"x\"\n",
            "src/lib.rs",
            "pub fn f() {}\n",
        ),
    ] {
        fs::create_dir_all(c.root.join(dir).join("src")).unwrap();
        fs::write(c.root.join(dir).join("Cargo.toml"), manifest).unwrap();
        fs::write(c.root.join(dir).join(path), text).unwrap();
    }
    fs::write(
        c.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"a\", \"a-bin\"]\nresolver = \"3\"\n",
    )
    .unwrap();
    lock(&c);
    write_index(
        &c,
        &[("a/src/main.rs", vec![]), ("a-bin/src/lib.rs", vec![])],
    );
    let (r, stderr) = calls_run(&c);
    calls_failed(&r, &c, "build-context-ambiguous");
    assert!(stderr.contains("a-bin-lib-x"), "{stderr}");
}

/// Two files of the probe: `src/lib.rs` (with a comment line on top) holding `leaf` and
/// `caller`, and `src/other.rs` holding `other_leaf` and `other_caller`, each with its row.
const PROBE_TWO: &str = "// a comment of the probe\npub mod other;\n\npub fn leaf() -> u32 {\n    1\n}\n\npub fn caller() -> u32 {\n    leaf()\n}\n";
const PROBE_OTHER: &str = "pub fn other_leaf() -> u32 {\n    2\n}\n\npub fn other_caller() -> u32 {\n    other_leaf()\n}\n";

fn probe_two(c: &Case, other_path: &str) {
    let lib = PROBE_TWO.replace(
        "pub mod other;",
        &if other_path == "src/other.rs" {
            "pub mod other;".to_owned()
        } else {
            format!(
                "#[path = \"{}\"]\npub mod other;",
                other_path.trim_start_matches("src/")
            )
        },
    );
    probe(c, "", &[(other_path, PROBE_OTHER)]);
    fs::write(c.root.join("src/lib.rs"), &lib).unwrap();
    let shift = i32::try_from(
        lib.lines()
            .position(|l| l.starts_with("pub fn leaf"))
            .unwrap(),
    )
    .unwrap();
    let at = |line: i32| line + shift;
    let leaf = "rust-analyzer cargo probe 0.1.0 leaf().";
    let caller = "rust-analyzer cargo probe 0.1.0 caller().";
    let other_leaf = "rust-analyzer cargo probe 0.1.0 other/other_leaf().";
    let other_caller = "rust-analyzer cargo probe 0.1.0 other/other_caller().";
    write_index(
        c,
        &[
            (
                "src/lib.rs",
                vec![
                    occurrence(leaf, [at(0), 7, 11], Some([at(0), 0, at(2), 1])),
                    occurrence(caller, [at(4), 7, 13], Some([at(4), 0, at(6), 1])),
                    occurrence(leaf, [at(5), 4, 8], None),
                ],
            ),
            (
                other_path,
                vec![
                    occurrence(other_leaf, [0, 7, 17], Some([0, 0, 2, 1])),
                    occurrence(other_caller, [4, 7, 19], Some([4, 0, 6, 1])),
                    occurrence(other_leaf, [5, 4, 14], None),
                ],
            ),
        ],
    );
}

fn row_files(c: &Case) -> Vec<String> {
    calls_rows(c)
        .iter()
        .map(|r| r["site"]["file"].as_str().unwrap().to_owned())
        .collect()
}

/// A tool of the host by its path, for scripts that run with the case's small `PATH`.
fn host(tool: &str) -> String {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join(tool))
        .find(|p| p.is_file())
        .unwrap()
        .display()
        .to_string()
}

#[test]
fn a_file_changed_after_the_build_gives_no_rows() {
    let c = Case::new();
    probe_two(&c, "src/other.rs");
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    assert_eq!(row_files(&c), ["src/lib.rs", "src/other.rs"]);

    // The indexer runs after the calls build. This one changes one byte of the comment,
    // keeping the file's length, its modification time, the call's text and every position.
    let c = Case::new();
    probe_two(&c, "src/other.rs");
    c.stub(&format!(
        "if [ \"$1\" = --version ]; then echo 'rust-analyzer 0.3.3049-standalone'; exit 0; fi\n\
         '{cp}' -p src/lib.rs \"$4.stamp\"\n\
         '{sed}' -i 's/a comment/A comment/' src/lib.rs\n\
         '{touch}' -r \"$4.stamp\" src/lib.rs\n\
         '{rm}' \"$4.stamp\"\n\
         '{cp}' index.scip \"$4\"",
        cp = host("cp"),
        sed = host("sed"),
        touch = host("touch"),
        rm = host("rm"),
    ));
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"]["status"], "ok", "{r}");
    assert_eq!(row_files(&c), ["src/other.rs"]);
}

#[test]
fn modification_times_do_not_decide_rows() {
    let mut outputs = Vec::new();
    for stamp in ["200001010000", "209901010000"] {
        let c = Case::new();
        probe_two(&c, "src/other.rs");
        let o = Command::new(host("touch"))
            .args(["-t", stamp])
            .arg(c.root.join("src/lib.rs"))
            .output()
            .unwrap();
        assert!(o.status.success());
        calls_run(&c);
        outputs.push(fs::read(c.out.join("calls.jsonl")).unwrap());
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(
        String::from_utf8(outputs[0].clone())
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn a_module_file_without_the_rs_ending_gives_rows() {
    let c = Case::new();
    probe_two(&c, "src/other.txt");
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    assert_eq!(row_files(&c), ["src/lib.rs", "src/other.txt"]);
}

#[test]
fn an_unreadable_file_elsewhere_changes_nothing() {
    let c = Case::new();
    probe_two(&c, "src/other.rs");
    let secret = c.root.join("notes/secret.rs");
    fs::create_dir_all(secret.parent().unwrap()).unwrap();
    fs::write(&secret, "fn hidden() {}\n").unwrap();
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
    let (r, _) = calls_run(&c);
    for layer in ["base", "syntax", "calls"] {
        assert_eq!(r["layers"][layer]["status"], "ok", "{r}");
    }
    assert_eq!(row_files(&c), ["src/lib.rs", "src/other.rs"]);
}

#[test]
fn remapped_sources_name_no_file_of_the_root() {
    let mut outputs = Vec::new();
    for (to, start) in [("/nowhere", None), (".", Some("a")), (".", Some("b"))] {
        let c = Case::new();
        probe(
            &c,
            "",
            &[(
                ".cargo/config.toml",
                &format!(
                    "[build]\nrustflags = [\"--remap-path-prefix={}={to}\"]\n",
                    fs::canonicalize(c.temp.path())
                        .unwrap()
                        .join("project")
                        .display()
                ),
            )],
        );
        let mut cmd = c.index();
        if let Some(dir) = start {
            let dir = c.temp.path().join(dir);
            fs::create_dir_all(&dir).unwrap();
            cmd.current_dir(dir);
        }
        let o = cmd.output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let r: Value =
            serde_json::from_slice(&fs::read(c.out.join("result.json")).unwrap()).unwrap();
        assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
        assert_eq!(fs::read(c.out.join("calls.jsonl")).unwrap(), b"");
        if start.is_some() {
            outputs.push((
                fs::read(c.out.join("result.json")).unwrap(),
                fs::read(c.out.join("calls.jsonl")).unwrap(),
            ));
        }
    }
    assert_eq!(outputs[0], outputs[1]);
}

/// The case's root replaced by a copy of `fixtures/<name>`.
fn fixture_root(c: &Case, name: &str) {
    fs::remove_dir_all(&c.root).unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    support::copy_tree(&fixtures.join(name), &c.root);
    let expected = c.root.join("expected");
    if expected.exists() {
        fs::remove_dir_all(expected).unwrap();
    }
}

/// Every string in a JSON value.
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|v| strings(v, out)),
        Value::Object(o) => o.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

#[test]
fn build_contexts_say_what_was_built_and_hold_no_path() {
    let mut runs = Vec::new();
    for parent in ["plain", "@"] {
        let c = Case::new();
        let moved = c.temp.path().join(parent).join("project");
        fixture_root(&c, "rust-calls");
        fs::create_dir_all(moved.parent().unwrap()).unwrap();
        fs::rename(&c.root, &moved).unwrap();
        let o = c
            .adapter()
            .arg("index")
            .arg("--root")
            .arg(&moved)
            .arg("--out")
            .arg(&c.out)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let result = fs::read(c.out.join("result.json")).unwrap();
        let rows = fs::read(c.out.join("calls.jsonl")).unwrap();
        let r: Value = serde_json::from_slice(&result).unwrap();
        assert!(validator(Schema::AdapterResult).is_valid(&r), "{r}");
        assert_eq!(r["layers"]["calls"]["status"], "ok", "{r}");
        let builds = r["builds"].as_array().unwrap();
        let ids: Vec<&str> = builds.iter().map(|b| b["id"].as_str().unwrap()).collect();
        // calls-app needs `engine`, which `default` enables through `front`; calls-extra
        // needs `extra`, which it does not, and is not built.
        assert_eq!(
            ids,
            [
                "calls-app-bin-calls-app",
                "calls-app-bin-calls-twin",
                "calls-derive-lib-calls_derive",
                "calls-lib-lib-calls_lib"
            ]
        );
        assert_eq!(builds[0]["features"], json!(["default", "engine", "front"]));
        assert_eq!(builds[3]["features"], json!(["default", "fast"]));
        for b in builds {
            assert_eq!(b["profile"], "dev");
            assert_eq!(b["target"], "x86_64-unknown-linux-gnu");
            assert!(
                b["toolchain"]
                    .as_str()
                    .unwrap()
                    .starts_with("rustc 1.99.1 ("),
                "{b}"
            );
        }
        let scratch = c.temp.path().display().to_string();
        let mut all = Vec::new();
        strings(&r, &mut all);
        for line in String::from_utf8(rows.clone()).unwrap().lines() {
            strings(&serde_json::from_str(line).unwrap(), &mut all);
        }
        for s in &all {
            assert!(!s.starts_with('/') && !s.contains(&scratch), "{s}");
        }
        assert_ne!(rows, b"");
        runs.push((result, rows));
    }
    assert_eq!(runs[0], runs[1]);
}

#[test]
fn a_library_of_two_crate_types_is_one_context() {
    let c = Case::new();
    probe(&c, "[lib]\ncrate-type = [\"rlib\", \"cdylib\"]\n", &[]);
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    let ids: Vec<&str> = r["builds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["probe-lib-probe"]);
    assert_eq!(calls_rows(&c).len(), 1);
}

#[test]
fn nothing_is_written_under_a_read_only_root() {
    let c = Case::new();
    fixture_root(&c, "rust-calls");
    let original = listing(&c.root);
    for (p, (mode, _)) in &original {
        fs::set_permissions(c.root.join(p), fs::Permissions::from_mode(mode & !0o222)).unwrap();
    }
    let before = listing(&c.root);
    let (r, _) = calls_run(&c);
    for layer in ["base", "syntax", "calls"] {
        assert_eq!(r["layers"][layer]["status"], "ok", "{r}");
    }
    assert_eq!(before, listing(&c.root));
    let modules: Vec<_> = fs::read_dir(c.out.join("work/calls"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(modules.len(), 4, "{modules:?}");
    for (p, (mode, _)) in original {
        fs::set_permissions(c.root.join(p), fs::Permissions::from_mode(mode)).unwrap();
    }
}

#[test]
fn calls_stay_comparable_across_a_release() {
    let mut builds = Vec::new();
    for variant in ["v0.1.0", "v0.2.0", "no-default"] {
        let c = Case::new();
        fixture_root(&c, &format!("rust-calls-variants/{variant}"));
        let (r, _) = calls_run(&c);
        assert_eq!(r["layers"]["calls"]["status"], "ok", "{r}");
        assert!(!calls_rows(&c).is_empty(), "{variant}");
        builds.push(r["builds"].clone());
    }
    assert_eq!(builds[0], builds[1]);
    assert_ne!(builds[0], builds[2]);
    let without = |b: &Value| {
        let mut b = b.clone();
        b[0].as_object_mut().unwrap().remove("features");
        b
    };
    assert_eq!(without(&builds[0]), without(&builds[2]));
}
#[test]
fn a_second_compilation_by_a_build_script_is_not_read() {
    let c = Case::new();
    probe(
        &c,
        "build = \"build.rs\"\n",
        &[(
            "build.rs",
            "use std::{env, path::Path, process::Command};\n\
             fn main() {\n\
             \x20   let out = env::var(\"OUT_DIR\").unwrap();\n\
             \x20   let status = Command::new(env::var(\"RUSTC\").unwrap())\n\
             \x20       .args([\"--crate-type\", \"lib\", \"--crate-name\", \"again\", \"--cfg\", \"again\", \"-g\", \"--emit=llvm-ir\", \"-o\"])\n\
             \x20       .arg(Path::new(&out).join(\"again.ll\"))\n\
             \x20       .arg(\"src/lib.rs\")\n\
             \x20       .status()\n\
             \x20       .unwrap();\n\
             \x20   assert!(status.success());\n\
             }\n",
        )],
    );
    // A function only the second compilation has, calling `leaf`.
    fs::write(
        c.root.join("src/lib.rs"),
        format!("{PROBE_LIB}\n#[cfg(again)]\npub fn again() -> u32 {{\n    leaf()\n}}\n"),
    )
    .unwrap();
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    let rows = calls_rows(&c);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["site"]["line"], 5);
    let mut pending = vec![c.out.join("work")];
    let mut again = false;
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            again |= path.file_name().is_some_and(|n| n == "again.ll");
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    assert!(again, "the build script wrote its own module");
}

/// Runs the engine on the case's root with the adapter on its `PATH` and the adapter cache
/// under `cache`, writing the snapshot to `out`.
fn engine_run(c: &Case, cache: &std::path::Path, out: &std::path::Path) -> std::process::Output {
    let link = c.bin.join("gosanja-adapter-rust");
    if !link.exists() {
        symlink(support::adapter_binary(), link).unwrap();
    }
    c.command(support::engine())
        .env("XDG_CACHE_HOME", cache)
        .arg("index")
        .arg("--root")
        .arg(&c.root)
        .arg("--out")
        .arg(out)
        .output()
        .unwrap()
}

fn snapshot_rows(out: &std::path::Path, file: &str) -> Vec<Value> {
    fs::read_to_string(out.join(file))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn calls_state(out: &std::path::Path) -> Value {
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    manifest["scopes"][0]["layers"]["calls"].clone()
}

fn has_failed_calls_gap(out: &std::path::Path) -> bool {
    snapshot_rows(out, "gaps.jsonl")
        .iter()
        .any(|g| g["gap"] == "failed-layer" && g["layer"] == "calls")
}

fn calls_relations(out: &std::path::Path) -> Vec<Value> {
    snapshot_rows(out, "relations.jsonl")
        .into_iter()
        .filter(|r| r["src"] == "adapter:rust/calls")
        .collect()
}

#[test]
fn a_failed_calls_layer_reaches_the_snapshot_beside_the_syntax_facts() {
    let c = Case::new();
    probe(
        &c,
        "",
        &[(
            ".cargo/config.toml",
            "[build]\nrustflags = [\"-C\", \"opt-level=3\"]\n",
        )],
    );
    let out = c.temp.path().join("snapshot");
    let o = engine_run(&c, &c.temp.path().join("cache"), &out);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(calls_state(&out), "failed");
    assert!(has_failed_calls_gap(&out));
    assert_ne!(snapshot_rows(&out, "syntax.jsonl"), [] as [Value; 0]);
}

#[test]
fn a_comma_in_the_cache_path_and_not_in_the_destination_decides() {
    // The adapter writes into the engine's cache: a comma there disables the layer.
    let c = Case::new();
    probe(&c, "", &[]);
    let out = c.temp.path().join("snapshot");
    let o = engine_run(&c, &c.temp.path().join("ca,che"), &out);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(calls_state(&out), "failed");
    assert!(has_failed_calls_gap(&out));
    assert_ne!(snapshot_rows(&out, "syntax.jsonl"), [] as [Value; 0]);
    // A comma in the snapshot's destination only: every layer succeeds.
    let c = Case::new();
    probe(&c, "", &[]);
    let out = c.temp.path().join("snap,shot");
    let o = engine_run(&c, &c.temp.path().join("cache"), &out);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(calls_state(&out), "present");
    assert_eq!(calls_relations(&out).len(), 1);
}

#[test]
fn a_file_with_a_backslash_in_its_name_is_left_out() {
    let c = Case::new();
    probe(
        &c,
        "",
        &[(
            "src/odd\\name.rs",
            "pub fn through(f: fn() -> u32) -> u32 {\n    f()\n}\n",
        )],
    );
    fs::write(
        c.root.join("src/lib.rs"),
        format!("{PROBE_LIB}\n#[path = \"odd\\\\name.rs\"]\npub mod odd;\n"),
    )
    .unwrap();
    let (r, _) = calls_run(&c);
    assert_eq!(r["layers"]["calls"], json!({"status": "ok"}), "{r}");
    assert_eq!(row_files(&c), ["src/lib.rs"]);
    let out = c.temp.path().join("snapshot");
    let o = engine_run(&c, &c.temp.path().join("cache"), &out);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(calls_relations(&out).len(), 1);
}
#[test]
fn a_dependency_missing_from_the_local_registry_fails_the_base_layer_first() {
    let c = Case::new();
    probe(&c, "", &[(".cargo/config.toml", "[net]\noffline = true\n")]);
    let manifest = fs::read_to_string(c.root.join("Cargo.toml")).unwrap();
    fs::write(
        c.root.join("Cargo.toml"),
        format!("{manifest}[dependencies]\ngosanja-probe-absent = \"1.0.0\"\n"),
    )
    .unwrap();
    fs::write(
        c.root.join("Cargo.lock"),
        "version = 4\n\n\
         [[package]]\nname = \"gosanja-probe-absent\"\nversion = \"1.0.0\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
         checksum = \"0000000000000000000000000000000000000000000000000000000000000000\"\n\n\
         [[package]]\nname = \"probe\"\nversion = \"0.1.0\"\ndependencies = [\n \"gosanja-probe-absent\",\n]\n",
    )
    .unwrap();
    failed(
        &c,
        &mut c.index(),
        "locked-metadata-failed",
        &["cargo", "rust-analyzer"],
    );
}

#[test]
fn calls_refuse_prerelease_producers_after_reading_all_modules() {
    let c = Case::new();
    probe(&c, "", &[]);
    let (result, _) = calls_run(&c);
    assert_eq!(result["layers"]["calls"]["status"], "ok", "{result}");
    assert_eq!(calls_rows(&c).len(), 1);

    for (release, binary, damage, code) in [
        ("1.102.0-nightly", false, "", "build-unsupported"),
        ("1.100.0-beta.3", false, "", "build-unsupported"),
        ("1.102.0-nightly", true, "", "ir-unreadable"),
        (
            "1.102.0-nightly",
            false,
            r#"sed -i '/ = call .* !dbg !/s/!dbg ![0-9][0-9]*/!dbg !999999/' "$ir""#,
            "ir-unreadable",
        ),
    ] {
        let c = Case::new();
        if binary {
            probe_with_binary(&c);
        } else {
            probe(&c, "", &[]);
        }
        wrap(
            &c,
            "rustc",
            &format!(
                r#""$REAL" "$@" || exit $?
for a in "$@"; do
    case "$a" in
        --emit=llvm-ir=*/0.ll)
            ir="${{a#--emit=llvm-ir=}}"
            sed -i '/DICompileUnit/s/rustc version [^ ]*/rustc version {release}/' "$ir"
            {damage}
            ;;
        --emit=llvm-ir=*/1.ll)
            ir="${{a#--emit=llvm-ir=}}"
            sed -i '/DICompileUnit/,$d' "$ir"
            ;;
    esac
done
exit 0"#
            ),
        );
        let (result, stderr) = calls_run(&c);
        calls_failed(&result, &c, code);
        if code == "build-unsupported" {
            assert!(stderr.contains(release), "{stderr}");
        }
    }
}

#[test]
fn requested_layers_control_output_and_the_cargo_rustc_build() {
    let choices = [
        (Some("base,syntax"), true, false),
        (Some("base,calls"), false, true),
        (Some("calls,base"), false, true),
        (Some("base"), false, false),
        (None, true, true),
    ];
    for (i, (list, syntax, calls)) in choices.into_iter().enumerate() {
        let c = Case::new();
        probe_with_binary(&c);
        let deep = format!(
            "fn deep() {{ {} }}",
            "if true { ".repeat(40) + &"} ".repeat(40)
        );
        fs::write(c.root.join("src/unused.rs"), deep).unwrap();
        write_index(
            &c,
            &[
                ("src/lib.rs", probe_document()),
                ("src/main.rs", vec![]),
                ("src/unused.rs", vec![]),
            ],
        );
        let log = c.temp.path().join("cargo.log");
        wrap(
            &c,
            "cargo",
            &format!(
                "{{ printf '%s\\t' \"$@\"; printf '\\n'; }} >> '{}'\nexec \"$REAL\" \"$@\"",
                log.display()
            ),
        );
        let mut command = c.adapter();
        command.arg("index");
        if i == 0 {
            command.args(["--layers", list.unwrap()]);
        }
        command.arg("--root").arg(&c.root);
        if i == 1 {
            command.args(["--layers", list.unwrap()]);
        }
        command.arg("--out").arg(&c.out);
        if i >= 2
            && let Some(selection) = list
        {
            command.args(["--layers", selection]);
        }
        let output = command.output().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{stderr}");
        let result: Value =
            serde_json::from_slice(&fs::read(c.out.join("result.json")).unwrap()).unwrap();
        assert!(
            validator(Schema::AdapterResult).is_valid(&result),
            "{result}"
        );
        let layers = result["layers"].as_object().unwrap();
        assert_eq!(layers.len(), 1 + usize::from(syntax) + usize::from(calls));
        assert_eq!(layers["base"]["status"], "ok", "{result}");
        assert_eq!(layers.contains_key("syntax"), syntax);
        assert_eq!(layers.contains_key("calls"), calls);
        assert_eq!(c.out.join("syntax.jsonl").is_file(), syntax);
        assert_eq!(c.out.join("calls.jsonl").is_file(), calls);
        assert_eq!(result.get("builds").is_some(), calls);
        assert_eq!(stderr.contains("outline-too-deep"), syntax, "{stderr}");
        if calls {
            assert!(!calls_rows(&c).is_empty(), "{result}");
        }
        let cargo = fs::read_to_string(log).unwrap();
        let targets: Vec<_> = cargo
            .lines()
            .filter(|line| line.starts_with("rustc\t"))
            .map(|line| {
                let (cargo_args, _) = line.split_once("\t--\t").expect("rustc argument boundary");
                let (_, target) = cargo_args.split_once("\t-p\t").expect("package selection");
                target
            })
            .collect();
        let expected: &[&str] = if calls {
            &["probe@0.1.0\t--bin\tprobe", "probe@0.1.0\t--lib"]
        } else {
            &[]
        };
        assert_eq!(targets, expected, "{cargo}");
    }

    let c = Case::new();
    probe(&c, "", &[]);
    fs::remove_file(c.root.join("Cargo.lock")).unwrap();
    let result = c.run(c.index().args(["--layers", "base,calls"]));
    assert_eq!(
        result["layers"],
        json!({
            "base": {"status":"failed", "diagnostics":[{"code":"lockfile-missing"}]},
            "calls": {"status":"failed", "diagnostics":[{"code":"base-failed"}]},
        })
    );
}

#[test]
fn invalid_layer_lists_leave_the_output_empty() {
    for (list, diagnostic) in [
        ("syntax", "must include \"base\""),
        ("base,base", "duplicate layer \"base\""),
        ("base,graph", "unknown layer \"graph\""),
        ("base,", "unknown layer \"\""),
        ("base, syntax", "unknown layer \" syntax\""),
        ("", "unknown layer \"\""),
    ] {
        let c = Case::new();
        let output = c.index().args(["--layers", list]).output().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(2), "{stderr}");
        assert!(stderr.contains(diagnostic), "{stderr}");
        assert!(fs::read_dir(&c.out).unwrap().next().is_none());
    }
}

#[test]
fn duplicate_index_options_are_rejected_before_writing() {
    let c = Case::new();
    for (option, value) in [
        ("--root", c.root.as_os_str()),
        ("--out", c.out.as_os_str()),
        ("--layers", std::ffi::OsStr::new("base")),
    ] {
        let mut command = c.index();
        if option == "--layers" {
            command.args(["--layers", "base,syntax"]);
        }
        let output = command.arg(option).arg(value).output().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(2), "{stderr}");
        assert!(
            stderr.contains(&format!("unexpected index option {option}")),
            "{stderr}"
        );
        assert!(fs::read_dir(&c.out).unwrap().next().is_none());
    }
}

#[test]
fn only_index_accepts_index_options() {
    let c = Case::new();
    let output = c
        .adapter()
        .arg("bogus")
        .arg("--root")
        .arg(&c.root)
        .arg("--out")
        .arg(&c.out)
        .args(["--layers", "base"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(fs::read_dir(&c.out).unwrap().next().is_none());
}
