mod build;
mod generate;
mod inputs;
mod support;
mod tools;

use std::{env, path::PathBuf};
use support::Result;

fn main() {
    if let Err(error) = run() {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("build") => {
            let mut output = root.join("build/sdk");
            let mut cache = root.join(".cache/sdk");
            while let Some(option) = args.next() {
                let destination = match option.as_str() {
                    "--output" => &mut output,
                    "--cache" => &mut cache,
                    _ => return Err(format!("unknown build option: {option}").into()),
                };
                *destination = args
                    .next()
                    .ok_or_else(|| format!("missing value for {option}"))?
                    .into();
            }
            build::build(&root, &output, &cache)
        }
        Some("generate") => {
            let options: Vec<_> = args.collect();
            match options.as_slice() {
                [] => generate::generate(&root, false),
                [option] if option == "--check" => generate::generate(&root, true),
                _ => Err("usage: cargo xtask generate [--check]".into()),
            }
        }
        None | Some("--help" | "-h") => {
            println!(
                "cargo xtask build [--output PATH] [--cache PATH]\ncargo xtask generate [--check]"
            );
            Ok(())
        }
        Some(command) => Err(format!("unknown command: {command}").into()),
    }
}
