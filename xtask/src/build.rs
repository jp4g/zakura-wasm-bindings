use crate::{generate, inputs, support::*, tools};
use serde_json::{json, Value};
use std::{env, fs, path::Path};

fn validate_paths(output: &Path, cache: &Path) -> Result<()> {
    if output.try_exists()? || output.is_symlink() {
        return Err(format!(
            "output already exists: {}; choose a new --output directory",
            output.display()
        )
        .into());
    }
    if output.starts_with(cache) || cache.starts_with(output) {
        return Err("output and cache must be separate directories".into());
    }
    Ok(())
}

pub fn build(root: &Path, output: &Path, cache: &Path) -> Result<()> {
    let output = absolute(output)?;
    let cache = absolute(cache)?;
    validate_paths(&output, &cache)?;
    tools::archives(env::consts::OS, env::consts::ARCH)?;
    let mut run = Runner::new();
    let git = |args: &[&str]| -> Result<String> {
        Ok(Runner::new().capture(root, "git", args)?.trim().into())
    };
    if !git(&["diff", "HEAD", "--"])?.is_empty() {
        return Err(
            "commit tracked source changes first so the build receipt identifies the source".into(),
        );
    }
    let revision = git(&["rev-parse", "HEAD"])?;
    let toolchain: toml::Value = fs::read_to_string(root.join("rust-toolchain.toml"))?.parse()?;
    let channel = toolchain["toolchain"]["channel"]
        .as_str()
        .ok_or("missing Rust toolchain channel")?;
    let cargo = cache.join("cargo");
    run.set("CARGO_HOME", &cargo);
    run.set("CARGO_TARGET_DIR", cache.join("target/wallet"));
    run.set("CARGO_BUILD_JOBS", "2");
    run.set("RUSTUP_TOOLCHAIN", channel);
    run.run(
        root,
        "rustup",
        &[
            "toolchain",
            "install",
            channel,
            "--profile",
            "minimal",
            "--target",
            "wasm32-unknown-unknown",
            "--no-self-update",
        ],
    )?;
    if run
        .capture(root, "node", &["-p", "process.versions.node.split('.')[0]"])?
        .trim()
        .parse::<u32>()?
        < 22
    {
        return Err("Node 22 or newer is required".into());
    }
    // Fail before downloads if the host compilation tools are missing.
    for tool in ["cc", "ar", "curl"] {
        let found = env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .any(|path| path.join(tool).is_file());
        if !found {
            return Err(format!("missing prerequisite: {tool}; see BUILDING.md").into());
        }
    }
    let (sdk, bindgen) = tools::install(&run, root, &cache)?;
    let policy = json(&root.join("native-policy/upstream.json"))?;
    let scratch = tempfile::tempdir()?;
    for (name, package) in policy.as_object().ok_or("invalid native policy")? {
        run.run(
            scratch.path(),
            "cargo",
            &[
                "info",
                &format!(
                    "{name}@{}",
                    package["version"].as_str().ok_or("missing version")?
                ),
            ],
        )?;
    }
    inputs::prepare(&run, root, &cargo)?;
    run.run(root, "cargo", &["fetch", "--locked"])?;
    fs::create_dir_all(output.parent().ok_or("output has no parent")?)?;
    fs::create_dir(&output)?;
    let metadata: Value = serde_json::from_str(&run.capture(
        root,
        "cargo",
        &[
            "metadata",
            "--locked",
            "--offline",
            "--features",
            "wallet-storage",
            "--format-version",
            "1",
        ],
    )?)?;
    let packages = inputs::verify(&run, root, &cargo, &metadata)?;
    let sqlite = metadata["packages"]
        .as_array()
        .ok_or("missing packages")?
        .iter()
        .find(|p| p["name"] == "libsqlite3-sys")
        .ok_or("missing sqlite")?;
    let sqlite = Path::new(
        sqlite["manifest_path"]
            .as_str()
            .ok_or("missing sqlite manifest")?,
    )
    .parent()
    .unwrap()
    .join("sqlite3");
    run.set("WALLET_SDK", &sdk);
    run.set("WALLET_SQLITE", sqlite);
    run.set("CC_wasm32_unknown_unknown", sdk.join("bin/clang"));
    run.set("AR_wasm32_unknown_unknown", sdk.join("bin/llvm-ar"));
    run.set("CFLAGS_wasm32_unknown_unknown", format!("--target=wasm32-wasi --sysroot={}/share/wasi-sysroot -DSQLITE_OS_OTHER=1 -USQLITE_THREADSAFE -DSQLITE_THREADSAFE=0 -DSQLITE_TEMP_STORE=3 -DSQLITE_OMIT_LOAD_EXTENSION=1", sdk.display()));
    run.set(
        "LIBSQLITE3_FLAGS",
        "-DSQLITE_ENABLE_MEMSYS5 -DSQLITE_ZERO_MALLOC -DLONGDOUBLE_TYPE=double -DSQLITE_OMIT_WAL",
    );
    run.set("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", format!("--remap-path-prefix={}=/source --remap-path-prefix={}=/cargo -C link-arg=--max-memory=268435456", root.display(), cargo.display()));
    run.run(root, "cargo", &["test", "--locked", "--offline", "--lib"])?;
    for name in ["primitive", "bundle"] {
        let mut args = vec![
            "build",
            "--locked",
            "--offline",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--lib",
        ];
        if name == "bundle" {
            args.extend(["--features", "wallet-storage"]);
        }
        run.run(root, "cargo", &args)?;
        let destination = output.join(name);
        run.run(
            root,
            text(&bindgen),
            &[
                "--target",
                "web",
                "--keep-lld-exports",
                "--out-dir",
                text(&destination),
                "--out-name",
                "bindings",
                text(&cache.join(
                    "target/wallet/wasm32-unknown-unknown/release/zakura_network_bindings.wasm",
                )),
            ],
        )?;
        let mut modules = vec!["bytes.mjs", "network.mjs", "transaction.mjs"];
        if name == "bundle" {
            modules.extend(["wallet.mjs", "views.mjs"]);
        }
        for module in modules {
            fs::copy(root.join(module), destination.join(module))?;
        }
    }
    copy_tree(
        &root.join("wallet-host"),
        &output.join("bundle/wallet-host"),
    )?;
    run.run(
        root,
        "node",
        &["tests/node.mjs", text(&output.join("primitive"))],
    )?;
    run.run(
        root,
        "node",
        &["tests/transaction.mjs", text(&output.join("primitive"))],
    )?;
    let inspection: Value = serde_json::from_str(&run.capture(
        root,
        "node",
        &["tests/wallet-inspect.mjs", text(&output.join("bundle"))],
    )?)?;
    if inspection["pass"] != true {
        return Err("native wallet inspection failed".into());
    }
    generate::generate(root, true)?;
    let mut codecs = json!({});
    for name in ["lightwire", "transparent-address"] {
        let source = root.join(name);
        run.set("CARGO_TARGET_DIR", cache.join("target").join(name));
        run.run(&source, "cargo", &["fetch", "--locked"])?;
        codec(
            &run,
            &source,
            &output.join(name),
            &cargo,
            &cache.join("target").join(name),
            &bindgen,
        )?;
        codecs[name] = sha(&output.join(name).join("receipt.json"))?.into();
    }
    if !git(&["diff", "HEAD", "--"])?.is_empty() || git(&["rev-parse", "HEAD"])? != revision {
        return Err("tracked sources changed during the build".into());
    }
    let (system, machine) = match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => ("Darwin", "arm64"),
        ("macos", arch) => ("Darwin", arch),
        (_, arch) => ("Linux", arch),
    };
    write_json(
        &output.join("build.json"),
        &json!({
            "format": "zcash-js-native-build/1", "complete": true, "revision": revision,
            "host": {"system": system, "machine": machine}, "tree": git(&["rev-parse", "HEAD^{tree}"])?,
            "lockSha256": sha(&root.join("Cargo.lock"))?, "nativePolicy": json(&root.join("native-policy/vendor/receipt.json"))?,
            "packages": packages, "rustc": run.capture(root, "rustc", &["-Vv"])?,
            "producerSha256": digest(&serde_json::to_vec(&inventory(&root.join("xtask/src"))?)?),
            "bindgenSha256": sha(&bindgen)?, "sdkClangSha256": sha(&sdk.join("bin/clang"))?,
            "inspection": inspection, "artifacts": inventory(&output.join("bundle"))?,
            "primitiveArtifacts": inventory(&output.join("primitive"))?, "codecs": codecs
        }),
    )?;
    println!("Native SDK build complete: {}", output.display());
    Ok(())
}

fn codec(
    run: &Runner,
    source: &Path,
    output: &Path,
    cargo: &Path,
    target: &Path,
    bindgen: &Path,
) -> Result<()> {
    let snapshot = inventory(source)?;
    fs::create_dir(output)?;
    let receipt_path = output.join("receipt.json");
    let mut receipt = json!({"status": "incomplete", "source": snapshot});
    write_json(&receipt_path, &receipt)?;
    let log = |name: &str, program: &str, args: &[&str]| -> Result<()> {
        run.log(source, &output.join(format!("{name}.log")), program, args)?;
        if inventory(source)? != snapshot {
            return Err(format!("source mutated during {name}").into());
        }
        Ok(())
    };
    log("rustc", "rustc", &["-vV"])?;
    log("cargo", "cargo", &["--version"])?;
    log("bindgen-version", text(bindgen), &["--version"])?;
    receipt["bindgen_sha256"] = sha(bindgen)?.into();
    let mut tool_hashes = json!({});
    for tool in ["rustc", "cargo"] {
        let path = run.capture(source, "rustup", &["which", tool])?;
        tool_hashes[path.trim()] = sha(Path::new(path.trim()))?.into();
    }
    receipt["tools"] = tool_hashes;
    let sysroot = run.capture(source, "rustc", &["--print", "sysroot"])?;
    let libraries = Path::new(sysroot.trim()).join("lib/rustlib/wasm32-unknown-unknown/lib");
    let mut hashes = json!({});
    for (name, hash) in inventory(&libraries)? {
        hashes[text(&libraries.join(name))] = hash.into();
    }
    if hashes.as_object().unwrap().is_empty() {
        return Err("missing installed wasm target libraries".into());
    }
    receipt["target_libraries"] = hashes;
    log(
        "metadata",
        "cargo",
        &[
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--filter-platform",
            "wasm32-unknown-unknown",
        ],
    )?;
    let metadata = json(&output.join("metadata.log"))?;
    let packages = inputs::verify(run, source, cargo, &metadata)?;
    receipt["graph"] = metadata["packages"].as_array().unwrap().iter().filter(|p| !p["source"].is_null()).map(|p| json!({
        "name": p["name"], "version": p["version"], "source": p["source"],
        "checksum": packages[format!("{}@{}", p["name"].as_str().unwrap(), p["version"].as_str().unwrap())]["checksum"]
    })).collect::<Vec<_>>().into();
    receipt["metadata_sha256"] = sha(&output.join("metadata.log"))?.into();
    receipt["graph_sha256"] = digest(&serde_json::to_vec(&metadata["resolve"])?).into();
    log("native", "cargo", &["test", "--locked", "--offline"])?;
    log(
        "wasm",
        "cargo",
        &[
            "build",
            "--locked",
            "--offline",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ],
    )?;
    let crate_name = if source.ends_with("lightwire") {
        "zakura_lightwire"
    } else {
        "zakura_transparent_address"
    };
    let wasm = target.join(format!("wasm32-unknown-unknown/release/{crate_name}.wasm"));
    log(
        "bindgen",
        text(bindgen),
        &[
            "--target",
            "web",
            "--out-dir",
            text(&output.join("wasm")),
            text(&wasm),
        ],
    )?;
    fs::copy(source.join("codec.mjs"), output.join("codec.mjs"))?;
    let mut artifacts = Inventory::from([("codec.mjs".into(), sha(&output.join("codec.mjs"))?)]);
    for (name, hash) in inventory(&output.join("wasm"))? {
        artifacts.insert(format!("wasm/{name}"), hash);
    }
    receipt["raw_wasm_sha256"] = sha(&wasm)?.into();
    receipt["artifacts"] = serde_json::to_value(artifacts)?;
    inputs::verify(run, source, cargo, &metadata)?;
    if inventory(source)? != snapshot {
        return Err("source mutated before completion".into());
    }
    receipt["status"] = "built".into();
    write_json(&receipt_path, &receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn source_mutation_leaves_only_an_incomplete_receipt() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source");
        let bin = temp.path().join("bin");
        fs::create_dir(&source)?;
        fs::create_dir(&bin)?;
        fs::write(source.join("codec.mjs"), "original")?;
        let rustc = bin.join("rustc");
        fs::write(&rustc, "#!/bin/sh\nprintf mutated > codec.mjs\n")?;
        fs::set_permissions(&rustc, fs::Permissions::from_mode(0o755))?;
        let mut run = Runner::new();
        run.set("PATH", &bin);
        let output = temp.path().join("output");
        let error = codec(
            &run,
            &source,
            &output,
            temp.path(),
            temp.path(),
            temp.path(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("source mutated during rustc"));
        assert_eq!(json(&output.join("receipt.json"))?["status"], "incomplete");
        assert!(!output.join("wasm").exists());
        assert!(!output.join("build.json").exists());
        Ok(())
    }
    #[test]
    fn refuses_existing_or_overlapping_output_without_mutating_it() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let cache = temp.path().join("cache");
        let output = temp.path().join("output");
        fs::create_dir(&output)?;
        fs::write(output.join("keep"), "keep")?;
        assert!(validate_paths(&output, &cache).is_err());
        assert_eq!(fs::read_to_string(output.join("keep"))?, "keep");
        assert!(validate_paths(&cache.join("inside"), &cache).is_err());
        assert!(validate_paths(&cache, &cache.join("inside")).is_err());
        assert!(validate_paths(&temp.path().join("fresh"), &cache).is_ok());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path(), temp.path().join("alias"))?;
            assert_eq!(absolute(&temp.path().join("alias/cache"))?, cache);
        }
        Ok(())
    }
}
