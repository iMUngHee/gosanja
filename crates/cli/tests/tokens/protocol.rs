fn token_engines() -> impl Iterator<Item = (Command, &'static str)> {
    std::iter::once((binary(), "0.6")).chain(
        std::env::var_os("GOSANJA_TOKEN_BASELINE_BIN").map(|path| (Command::new(path), "0.5")),
    )
}

fn token_index(engine: &mut Command, contract: &str, mode: &str, dir: &Path) -> Output {
    let adapter = dir.join("token-adapter");
    fs::write(
        &adapter,
        "#!/bin/sh\nexec \"$TOKEN_FIXTURE\" --contract \"$TOKEN_CONTRACT\" \
         --layers '[\"base\",\"syntax\"]' \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&adapter, fs::Permissions::from_mode(0o755)).unwrap();
    engine
        .args(["index", "--adapter"])
        .arg(adapter)
        .args(["--layers", "base,syntax", "--root"])
        .arg(fixture().join("rust-basic"))
        .arg("--out")
        .arg(dir.join("snapshot"))
        .env("TOKEN_FIXTURE", fixture().join("fake-adapter/adapter.sh"))
        .env("TOKEN_CONTRACT", contract)
        .env("GOSANJA_FAKE_RECORD", dir.join("commands"));
    configure(engine, mode, &dir.join("cache"));
    engine.output().unwrap()
}

#[test]
fn token_sites_malformed_requested_result_prevents_publication() {
    for (mut engine, contract) in token_engines() {
        let good = tempdir().unwrap();
        check(&token_index(&mut engine, contract, "good", good.path()), 0);
        let manifest: Value =
            serde_json::from_slice(&fs::read(good.path().join("snapshot/manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["scopes"][0]["adapter"]["contract"], contract);
        assert_eq!(manifest["scopes"][0]["layers"]["syntax"], "present");
    }
    for (mut engine, contract) in token_engines() {
        let bad = tempdir().unwrap();
        let result = token_index(&mut engine, contract, "token-malformed-result", bad.path());
        check(&result, 3);
        let err = String::from_utf8(result.stderr).unwrap();
        assert!(
            err.contains("23 is not one of \"ok\" or \"failed\""),
            "{contract}: {err}"
        );
        assert!(!bad.path().join("snapshot").exists());
        assert_eq!(
            fs::read_to_string(bad.path().join("commands")).unwrap(),
            "describe\nindex\n"
        );
    }
}

#[test]
fn token_sites_refuse_old_contract_at_the_right_phase() {
    let old_describe = tempdir().unwrap();
    let result = token_index(&mut binary(), "0.5", "good", old_describe.path());
    check(&result, 2);
    assert!(String::from_utf8(result.stderr).unwrap().contains("0.5"));
    assert_eq!(
        fs::read_to_string(old_describe.path().join("commands")).unwrap(),
        "describe\n"
    );
    assert!(!old_describe.path().join("snapshot").exists());

    let old_result = tempdir().unwrap();
    let result = token_index(&mut binary(), "0.6", "old-token-result", old_result.path());
    check(&result, 3);
    let err = String::from_utf8(result.stderr).unwrap();
    assert!(
        err.contains("result contract differs from describe"),
        "{err}"
    );
    assert_eq!(
        fs::read_to_string(old_result.path().join("commands")).unwrap(),
        "describe\nindex\n"
    );
    assert!(!old_result.path().join("snapshot").exists());
}

#[test]
fn token_sites_reader_rejects_marks_outside_reference_sites() {
    for (mut engine, contract) in token_engines() {
        let scratch = tempdir().unwrap();
        check(
            &token_index(&mut engine, contract, "good", scratch.path()),
            0,
        );
        let out = scratch.path().join("snapshot");
        let table = out.join("relations.jsonl");
        let site = json!(["src/lib.rs", 4, 15, 4, 21]);
        let mut changed = 0;
        let rows = fs::read_to_string(&table)
            .unwrap()
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                if row["rel"] == "ref"
                    && row["to"] == "cargo:demo:views/render()."
                    && row["sites"] == json!([site])
                {
                    row["roles"] = json!(["import"]);
                    row["syntax_import_sites"] = json!([["src/lib.rs", 4, 8, 4, 13]]);
                    changed += 1;
                    format!("{row}\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        assert_eq!(changed, 1);
        for rooted in [false, true] {
            let validate = || {
                let mut cmd = Command::new(engine.get_program());
                cmd.arg("validate").arg(&out);
                if rooted {
                    cmd.arg("--root").arg(fixture().join("rust-basic"));
                }
                cmd.output().unwrap()
            };
            check(&validate(), 0);
            let before = fs::read(&table).unwrap();
            fs::write(&table, &rows).unwrap();
            let result = validate();
            check(&result, i32::from(contract == "0.6"));
            if contract == "0.6" {
                let report = format!(
                    "{}{}",
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(report.contains("relations.jsonl"), "{report}");
                assert!(report.contains("syntax-import-subset"), "{report}");
            }
            fs::write(&table, before).unwrap();
        }
    }
}
