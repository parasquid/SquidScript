use std::path::Path;

pub fn source_app_id(source: &str) -> Option<String> {
    squidc_core::parse(source)
        .ast
        .app
        .map(|app| app.id)
        .filter(|id| !id.is_empty())
}

pub fn source_for_compile(source: &str, app_id: &str) -> String {
    if source_app_id(source).is_some() {
        source.to_string()
    } else {
        format!("app \"{}\"\n\n{}", app_id, source)
    }
}

pub fn generated_app_id(path: &Path, source: &str) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("app");
    format!("{}-{:08x}", sanitize_app_id(stem), fnv1a(source.as_bytes()))
}

pub fn sanitize_app_id(value: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            previous_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if !previous_dash {
            previous_dash = true;
            Some('-')
        } else {
            None
        };
        if let Some(ch) = next {
            out.push(ch);
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "app".to_string()
    } else {
        out
    }
}

pub fn fnv1a(data: &[u8]) -> u32 {
    let mut value = 0x811c9dc5u32;
    for byte in data {
        value ^= u32::from(*byte);
        value = value.wrapping_mul(0x0100_0193);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_app_id_is_stable_and_sanitized() {
        let id = generated_app_id(Path::new("My App!.squid"), "state {}\n");
        assert_eq!(id, "my-app-eb429b88");
        assert_eq!(
            generated_app_id(Path::new("My App!.squid"), "state {}\n"),
            id
        );
    }

    #[test]
    fn source_app_id_reads_declared_app() {
        assert_eq!(
            source_app_id("app \"sample-app\"\nstate {}\n").as_deref(),
            Some("sample-app")
        );
        assert_eq!(source_app_id("state {}\n"), None);
    }

    #[test]
    fn source_for_compile_injects_missing_app_decl() {
        let source = source_for_compile("state {}\nscreen(\"main\") {}\n", "main-a1");
        assert!(source.starts_with("app \"main-a1\""));
        assert_eq!(
            source_for_compile("app \"named\"\nstate {}\n", "ignored"),
            "app \"named\"\nstate {}\n"
        );
    }
}
