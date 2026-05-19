use std::{env, fs, path::PathBuf, process};

use squidc_core::{compile, sqbc_v2::encode_sqbc_v2, CompileRequest};

fn main() {
    if let Err(error) = run() {
        eprintln!("squidc: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) != Some("build") {
        return Err(
            "usage: squidc build <input.squid> --target <target-id> --out <main.sqbc>".to_string(),
        );
    }

    let mut input = None;
    let mut target = None;
    let mut out = None;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                index += 1;
                target = args.get(index).cloned();
            }
            "--out" => {
                index += 1;
                out = args.get(index).map(PathBuf::from);
            }
            value if input.is_none() => input = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument {value}")),
        }
        index += 1;
    }

    let input = input.ok_or_else(|| "missing input .squid path".to_string())?;
    let target = target.ok_or_else(|| "missing --target".to_string())?;
    let out = out.ok_or_else(|| "missing --out".to_string())?;
    let source = fs::read_to_string(&input)
        .map_err(|error| format!("failed to read {}: {error}", input.display()))?;
    let compiled = compile(CompileRequest {
        source,
        target_id: target,
    });
    if !compiled.ok {
        for diagnostic in compiled.diagnostics {
            eprintln!(
                "{}:{}..{}: {}",
                diagnostic.code, diagnostic.span.start, diagnostic.span.end, diagnostic.message
            );
        }
        return Err("compile failed".to_string());
    }

    let ir = compiled
        .ir
        .ok_or_else(|| "compiler returned no IR".to_string())?;
    let bytes = encode_sqbc_v2(&ir).map_err(|error| error.message)?;
    fs::write(&out, bytes)
        .map_err(|error| format!("failed to write {}: {error}", out.display()))?;
    Ok(())
}
