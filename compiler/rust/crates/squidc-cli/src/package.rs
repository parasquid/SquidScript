use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use squidc_core::{profile::BuildProfile, sqbc::read_app_id};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::compile::compile_path_to_sqbc;

pub struct PackageResult {
    pub out: PathBuf,
    pub app_id: String,
    pub entries: Vec<String>,
    pub bytes: usize,
}

pub struct ZipEntryData {
    pub path: String,
    pub bytes: Vec<u8>,
}

struct PackageEntry {
    path: String,
    bytes: Vec<u8>,
}

pub fn package_app_dir(
    app_dir: &Path,
    out: Option<&Path>,
    target: &str,
    profile: BuildProfile,
) -> Result<PackageResult, String> {
    if !app_dir.is_dir() {
        return Err(format!("app directory not found: {}", app_dir.display()));
    }
    let main = app_dir.join("main.squid");
    let sqbc = compile_path_to_sqbc(&main, target, profile)?;
    let app_id = read_app_id(&sqbc)
        .map_err(|error| error.message)?
        .ok_or_else(|| "compiled SQBC has no app id".to_string())?;
    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(format!("{app_id}.squid.zip")));

    let mut entries = vec![PackageEntry {
        path: "main.sqbc".to_string(),
        bytes: sqbc,
    }];
    collect_resource_entries(app_dir, app_dir, &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries.dedup_by(|a, b| a.path == b.path);

    let bytes = write_zip(&out, &entries)?;
    Ok(PackageResult {
        out,
        app_id,
        entries: entries.into_iter().map(|entry| entry.path).collect(),
        bytes,
    })
}

fn collect_resource_entries(
    app_dir: &Path,
    dir: &Path,
    entries: &mut Vec<PackageEntry>,
) -> Result<(), String> {
    let mut children = fs::read_dir(dir)
        .map_err(|error| format!("failed to read {}: {error}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read {}: {error}", dir.display()))?;
    children.sort_by_key(|entry| entry.path());
    for child in children {
        let path = child.path();
        let name = child.file_name();
        let Some(name) = name.to_str() else {
            return Err(format!("path is not UTF-8: {}", path.display()));
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_resource_entries(app_dir, &path, entries)?;
            continue;
        }
        let rel = path
            .strip_prefix(app_dir)
            .map_err(|_| format!("path escapes app directory: {}", path.display()))?;
        let rel = rel
            .to_str()
            .ok_or_else(|| format!("path is not UTF-8: {}", path.display()))?
            .replace('\\', "/");
        if rel == "main.sqbc"
            || rel == "source-map.json"
            || rel.ends_with(".squid")
            || rel.ends_with(".squid.zip")
        {
            continue;
        }
        normalize_package_entry_path(&rel)?;
        entries.push(PackageEntry {
            path: rel,
            bytes: fs::read(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?,
        });
    }
    Ok(())
}

pub fn normalize_package_entry_path(path: &str) -> Result<String, String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err(format!("invalid package entry path: {path}"));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.starts_with('.') {
            return Err(format!("invalid package entry path: {path}"));
        }
        parts.push(part);
    }
    let normalized = parts.join("/");
    if normalized.is_empty()
        || normalized == "sd"
        || normalized.starts_with("sd/")
        || normalized == "system"
        || normalized.starts_with("system/")
        || normalized.ends_with(".squid")
        || normalized == "source-map.json"
        || normalized.ends_with(".squid.zip")
    {
        return Err(format!("invalid package entry path: {path}"));
    }
    Ok(normalized)
}

fn write_zip(path: &Path, entries: &[PackageEntry]) -> Result<usize, String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let mut file = fs::File::create(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    let mut writer = ZipWriter::new(&mut file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for entry in entries {
        writer
            .start_file(&entry.path, options)
            .map_err(|error| format!("failed to write ZIP entry {}: {error}", entry.path))?;
        writer
            .write_all(&entry.bytes)
            .map_err(|error| format!("failed to write ZIP entry {}: {error}", entry.path))?;
    }
    writer
        .finish()
        .map_err(|error| format!("failed to finish ZIP {}: {error}", path.display()))?;
    file.metadata()
        .map(|metadata| metadata.len() as usize)
        .map_err(|error| format!("failed to stat {}: {error}", path.display()))
}

pub fn read_stored_zip_entries(bytes: &[u8]) -> Result<Vec<ZipEntryData>, String> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|error| format!("invalid ZIP: {error}"))?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("invalid ZIP entry {index}: {error}"))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|error| format!("failed to read ZIP entry {name}: {error}"))?;
        entries.push(ZipEntryData {
            path: normalize_package_entry_path(&name)?,
            bytes: data,
        });
    }
    Ok(entries)
}
