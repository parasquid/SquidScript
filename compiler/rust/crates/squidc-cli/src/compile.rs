use std::{fs, path::Path};

use squidc_core::{
    compile::{compile_with_profile, CompileRequest},
    profile::{BuildProfile, PORTABLE_TARGET_ID},
    sqbc_v2::encode_sqbc_v2_with_profile,
};

pub fn compile_target_id(target: Option<&str>, check_target: bool) -> Result<String, String> {
    if check_target {
        let target = target.ok_or_else(|| "--check-target requires --target".to_string())?;
        return Ok(target_id_from_arg(target));
    }
    Ok(PORTABLE_TARGET_ID.to_string())
}

fn target_id_from_arg(target: &str) -> String {
    let path = Path::new(target);
    if !path.exists() {
        return target.to_string();
    }
    let Ok(text) = fs::read_to_string(path) else {
        return target.to_string();
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("\"id\"") {
            if let Some((_, value)) = rest.split_once(':') {
                return value
                    .trim()
                    .trim_end_matches(',')
                    .trim_matches('"')
                    .to_string();
            }
        }
    }
    target.to_string()
}

pub fn compile_source_to_sqbc(
    source: &str,
    target: &str,
    profile: BuildProfile,
) -> Result<Vec<u8>, String> {
    let compiled = compile_with_profile(
        CompileRequest {
            source: source.to_string(),
            target_id: target.to_string(),
        },
        profile,
    );
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
    encode_sqbc_v2_with_profile(&ir, profile).map_err(|error| error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqbc_app_id_metadata_round_trips() {
        let source = r#"app "metadata-demo"
state {}
event.on("app.start") {}
screen("main") {}
"#;
        let bytes = compile_source_to_sqbc(source, PORTABLE_TARGET_ID, BuildProfile::Dev).unwrap();
        assert_eq!(
            squidc_core::sqbc_v2::read_app_id(&bytes)
                .unwrap()
                .as_deref(),
            Some("metadata-demo")
        );
    }
}
