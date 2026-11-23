#![cfg(unix)]
mod support;
use gosanja_core::schemas::{Schema, validator};
use protobuf::Message;
use scip::types::{Document, Index};
use serde_json::{Value, json};
use std::{
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
    assert_eq!(
        r["layers"]["syntax"],
        json!({"status":"failed","diagnostics":[{"code":"base-failed"}]})
    );
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
        json!({"name":"rust","version":env!("CARGO_PKG_VERSION"),"contract":"0.2","languages":["rust"],"layers":["base","syntax"],"position_encoding":"utf-32","markers":["Cargo.toml"],"requires":[{"tool":"rust-analyzer"},{"tool":"cargo"}]})
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
    let before = listing(&c.root);
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
    }
    assert_eq!(before, listing(&c.root));
    assert_eq!(fs::read_dir(&c.out).unwrap().count(), 0);
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
        c.stub("if [ \"$1\" = --version ]; then echo 'rust-analyzer test'; else echo called > \"$HOME/invoked\"; fi");
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
        let entries = listing(&c.root);
        for (path, (mode, _)) in &entries {
            fs::set_permissions(c.root.join(path), fs::Permissions::from_mode(mode & !0o222))
                .unwrap();
        }
        let tools = &["cargo", "rust-analyzer"][..n];
        let bytes = failed(&c, &mut c.index(), code, tools);
        if scenario == "stale" {
            fs::remove_dir_all(&c.out).unwrap();
            fs::create_dir(&c.out).unwrap();
            assert_eq!(bytes, failed(&c, &mut c.index(), code, tools));
        }
        for (path, (mode, _)) in entries {
            fs::set_permissions(c.root.join(path), fs::Permissions::from_mode(mode)).unwrap();
        }
        assert!(!c.temp.path().join("home/invoked").exists());
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
        failed(&c, c.index().env(key, path), code, &[]);
    }
}
#[test]
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
            ["index.scip", "result.json", "syntax.jsonl", "work"]
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
