use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TargetDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub firmware: Option<Firmware>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl TargetDefinition {
    pub fn firmware(&self) -> Result<&Firmware, String> {
        self.firmware
            .as_ref()
            .ok_or_else(|| format!("target {} has no firmware metadata", self.id))
    }

    pub fn summary_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "status": self.status,
            "features": self.features,
            "path": self.path,
            "firmwarePackage": self.firmware.as_ref().map(|firmware| firmware.package.clone())
        })
    }

    pub fn inspect_json(&self, root: &Path) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "status": self.status,
            "features": self.features,
            "path": self.path,
            "firmware": self.firmware.as_ref().map(|firmware| firmware.resolved_json(root))
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Firmware {
    pub package: String,
    pub working_dir: PathBuf,
    pub target: String,
    pub chip: String,
    pub elf: PathBuf,
    pub ota_image: PathBuf,
    pub partition_table: PathBuf,
    pub bootloader: PathBuf,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub rustup_toolchain: Option<String>,
    #[serde(default = "default_ble_connection_watchdog_ms")]
    pub ble_connection_watchdog_ms: u64,
}

const fn default_ble_connection_watchdog_ms() -> u64 {
    30_000
}

impl Firmware {
    fn resolved_json(&self, root: &Path) -> Value {
        json!({
            "package": self.package,
            "workingDir": resolve_repo_path(root, &self.working_dir),
            "target": self.target,
            "chip": self.chip,
            "elf": resolve_repo_path(root, &self.elf),
            "otaImage": resolve_repo_path(root, &self.ota_image),
            "partitionTable": resolve_repo_path(root, &self.partition_table),
            "bootloader": resolve_repo_path(root, &self.bootloader),
            "features": self.features,
            "release": self.release,
            "rustupToolchain": self.rustup_toolchain,
            "bleConnectionWatchdogMs": self.ble_connection_watchdog_ms,
        })
    }
}

const ESP32C3_FLASH_BYTES: u64 = 16 * 1024 * 1024;
const OTA_SLOT_BYTES: u64 = 0x280000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PartitionEntry {
    name: String,
    kind: String,
    subtype: String,
    offset: u64,
    size: u64,
}

fn parse_partition_number(value: &str) -> Result<u64, String> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|_| format!("invalid partition number {value}"))
    } else {
        value
            .parse()
            .map_err(|_| format!("invalid partition number {value}"))
    }
}

fn validate_native_partition_table(path: &Path) -> Result<Vec<PartitionEntry>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        if fields.len() < 5 {
            return Err(format!(
                "{}:{}: partition row requires five fields",
                path.display(),
                index + 1
            ));
        }
        let entry = PartitionEntry {
            name: fields[0].to_string(),
            kind: fields[1].to_string(),
            subtype: fields[2].to_string(),
            offset: parse_partition_number(fields[3])?,
            size: parse_partition_number(fields[4])?,
        };
        if entry.offset % 0x1000 != 0 || entry.size == 0 || entry.size % 0x1000 != 0 {
            return Err(format!("partition {} is not 4 KiB aligned", entry.name));
        }
        if entry.offset.checked_add(entry.size).is_none()
            || entry.offset + entry.size > ESP32C3_FLASH_BYTES
        {
            return Err(format!("partition {} exceeds 16 MiB flash", entry.name));
        }
        entries.push(entry);
    }
    entries.sort_by_key(|entry| entry.offset);
    for pair in entries.windows(2) {
        if pair[0].offset + pair[0].size > pair[1].offset {
            return Err(format!(
                "partitions {} and {} overlap",
                pair[0].name, pair[1].name
            ));
        }
    }
    let required = [
        ("nvs", "data", "nvs", 0x9000, 0x5000),
        ("otadata", "data", "ota", 0xe000, 0x2000),
        ("app0", "app", "ota_0", 0x10000, OTA_SLOT_BYTES),
        ("app1", "app", "ota_1", 0x290000, OTA_SLOT_BYTES),
        ("squidscript", "data", "littlefs", 0x510000, 0xae0000),
        ("coredump", "data", "coredump", 0xff0000, 0x10000),
    ];
    for (name, kind, subtype, offset, size) in required {
        if !entries.iter().any(|entry| {
            entry.name == name
                && entry.kind == kind
                && entry.subtype == subtype
                && entry.offset == offset
                && entry.size == size
        }) {
            return Err(format!(
                "partition table is missing required {name} geometry"
            ));
        }
    }
    Ok(entries)
}

#[derive(Clone, Debug)]
pub struct TargetBuildPlanOptions {
    pub tool_args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TargetFlashPlanOptions {
    pub port: Option<String>,
    pub tool_args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TargetMonitorPlanOptions {
    pub port: Option<String>,
    pub tool_args: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandPlan {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

impl CommandPlan {
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.clone())
            .chain(self.args.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn as_json(&self) -> Value {
        json!({
            "program": self.program,
            "args": self.args,
            "cwd": self.cwd,
            "env": self.env.iter().map(|(key, value)| {
                json!({"key": key, "value": value})
            }).collect::<Vec<_>>(),
            "commandLine": self.command_line()
        })
    }
}

pub fn repo_root() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("targets").is_dir() && cwd.join("Cargo.toml").is_file() {
        return cwd;
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .find(|path| path.join("targets").is_dir() && path.join("Cargo.toml").is_file())
        .unwrap_or(manifest.as_path())
        .to_path_buf()
}

pub fn load_targets(root: &Path) -> Result<Vec<TargetDefinition>, String> {
    let targets_dir = root.join("targets");
    let mut targets = Vec::new();
    let entries = fs::read_dir(&targets_dir)
        .map_err(|error| format!("failed to read {}: {error}", targets_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read target entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json")
            || !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.ends_with(".target.json"))
        {
            continue;
        }
        let mut target: TargetDefinition = serde_json::from_str(
            &fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
        target.path = path;
        if let Some(firmware) = target.firmware.as_ref() {
            validate_native_partition_table(&resolve_repo_path(root, &firmware.partition_table))?;
        }
        targets.push(target);
    }
    targets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(targets)
}

pub fn load_target_by_id(root: &Path, id: &str) -> Result<TargetDefinition, String> {
    load_targets(root)?
        .into_iter()
        .find(|target| target.id == id)
        .ok_or_else(|| {
            format!("unknown target {id}; run `squidc target list` to see available targets")
        })
}

pub fn resolve_target_arg(
    root: &Path,
    target: Option<&str>,
    interactive: bool,
) -> Result<TargetDefinition, String> {
    if let Some(target) = target {
        return load_target_by_id(root, target);
    }
    if !interactive {
        return Err("target command requires --target in noninteractive sessions; run `squidc target list` and pass --target <target-id>".to_string());
    }
    choose_target_interactively(root)
}

pub fn plan_build_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetBuildPlanOptions,
) -> Result<CommandPlan, String> {
    plan_native_build_command(root, target, options)
}

pub fn plan_native_image_command(
    root: &Path,
    target: &TargetDefinition,
) -> Result<CommandPlan, String> {
    let firmware = target.firmware()?;
    validate_native_partition_table(&resolve_repo_path(root, &firmware.partition_table))?;
    Ok(CommandPlan {
        program: espflash_program(),
        args: vec![
            "save-image".to_string(),
            "--chip".to_string(),
            firmware.chip.clone(),
            "--flash-size".to_string(),
            "16mb".to_string(),
            "--partition-table".to_string(),
            resolve_repo_path(root, &firmware.partition_table)
                .display()
                .to_string(),
            "--target-app-partition".to_string(),
            "app0".to_string(),
            resolve_repo_path(root, &firmware.elf).display().to_string(),
            resolve_repo_path(root, &firmware.ota_image)
                .display()
                .to_string(),
        ],
        cwd: root.to_path_buf(),
        env: Vec::new(),
    })
}

pub fn validate_native_ota_image(root: &Path, target: &TargetDefinition) -> Result<u64, String> {
    let firmware = target.firmware()?;
    let path = resolve_repo_path(root, &firmware.ota_image);
    let size = fs::metadata(&path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
        .len();
    if size == 0 || size > OTA_SLOT_BYTES {
        return Err(format!(
            "OTA image {} is {size} bytes; app0 capacity is {OTA_SLOT_BYTES} bytes",
            path.display()
        ));
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    crate::firmware_image::validate(&bytes)
        .map_err(|error| format!("invalid native OTA image {}: {error}", path.display()))?;
    Ok(size)
}

fn plan_native_build_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetBuildPlanOptions,
) -> Result<CommandPlan, String> {
    let firmware = target.firmware()?;
    let mut args = vec![
        "build".to_string(),
        "-p".to_string(),
        firmware.package.clone(),
        "--target".to_string(),
        firmware.target.clone(),
    ];
    if !firmware.features.is_empty() {
        args.push("--features".to_string());
        args.push(firmware.features.join(","));
    }
    if firmware.release {
        args.push("--release".to_string());
    }
    args.extend(options.tool_args);

    let (program, args, mut env) = match firmware.rustup_toolchain.as_deref() {
        Some(toolchain) => {
            let mut rustup_args = vec![
                "run".to_string(),
                toolchain.to_string(),
                "cargo".to_string(),
            ];
            rustup_args.extend(args);
            (
                "rustup".to_string(),
                rustup_args,
                vec![
                    ("RUSTC".to_string(), rustup_tool_path(toolchain, "rustc")?),
                    (
                        "SQUIDSCRIPT_BLE_CONNECTION_WATCHDOG_MS".to_string(),
                        firmware.ble_connection_watchdog_ms.to_string(),
                    ),
                ],
            )
        }
        None => (
            "cargo".to_string(),
            args,
            vec![(
                "SQUIDSCRIPT_BLE_CONNECTION_WATCHDOG_MS".to_string(),
                firmware.ble_connection_watchdog_ms.to_string(),
            )],
        ),
    };
    if firmware
        .features
        .iter()
        .any(|feature| feature == "x4-flash-filesystem")
    {
        let compiler = env::var("SQUIDSCRIPT_RISCV_CC")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(detect_riscv_c_compiler);
        if let Some(compiler) = compiler {
            let target_key = firmware.target.replace('-', "_");
            env.push((format!("CC_{target_key}"), compiler));
            env.push((
                format!("CFLAGS_{target_key}"),
                "-march=rv32imc -mabi=ilp32".to_string(),
            ));
        }
    }

    Ok(CommandPlan {
        program,
        args,
        cwd: resolve_repo_path(root, &firmware.working_dir),
        env,
    })
}

fn detect_riscv_c_compiler() -> Option<String> {
    [
        "riscv32-esp-elf-gcc",
        "riscv32-unknown-elf-gcc",
        "riscv64-elf-gcc",
    ]
    .into_iter()
    .find(|candidate| {
        Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
    .map(str::to_string)
}

fn rustup_tool_path(toolchain: &str, tool: &str) -> Result<String, String> {
    let output = Command::new("rustup")
        .args(["which", "--toolchain", toolchain, tool])
        .output()
        .map_err(|error| format!("failed to query rustup {toolchain} {tool}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustup could not locate {tool} for toolchain {toolchain}"
        ));
    }
    String::from_utf8(output.stdout)
        .map(|path| path.trim().to_string())
        .map_err(|_| format!("rustup returned a non-UTF-8 path for {tool}"))
}

pub fn plan_flash_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetFlashPlanOptions,
) -> Result<CommandPlan, String> {
    plan_native_flash_command(root, target, options)
}

fn plan_native_flash_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetFlashPlanOptions,
) -> Result<CommandPlan, String> {
    let firmware = target.firmware()?;
    let mut args = vec![
        "flash".to_string(),
        "--chip".to_string(),
        firmware.chip.clone(),
        "--non-interactive".to_string(),
        "--flash-size".to_string(),
        "16mb".to_string(),
        "--partition-table".to_string(),
        resolve_repo_path(root, &firmware.partition_table)
            .display()
            .to_string(),
        "--bootloader".to_string(),
        resolve_repo_path(root, &firmware.bootloader)
            .display()
            .to_string(),
        "--target-app-partition".to_string(),
        "app0".to_string(),
        resolve_repo_path(root, &firmware.elf).display().to_string(),
    ];
    if let Some(port) = options.port {
        args.push("--port".to_string());
        args.push(port);
    }
    args.extend(options.tool_args);
    Ok(CommandPlan {
        program: espflash_program(),
        args,
        cwd: root.to_path_buf(),
        env: Vec::new(),
    })
}

pub fn plan_monitor_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetMonitorPlanOptions,
) -> Result<CommandPlan, String> {
    plan_native_monitor_command(root, target, options)
}

fn plan_native_monitor_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetMonitorPlanOptions,
) -> Result<CommandPlan, String> {
    let firmware = target.firmware()?;
    let mut args = vec![
        "monitor".to_string(),
        "--chip".to_string(),
        firmware.chip.clone(),
        "--non-interactive".to_string(),
    ];
    if let Some(port) = options.port {
        args.push("--port".to_string());
        args.push(port);
    }
    args.extend(options.tool_args);
    Ok(CommandPlan {
        program: espflash_program(),
        args,
        cwd: root.to_path_buf(),
        env: Vec::new(),
    })
}

pub fn run_plan(plan: &CommandPlan) -> Result<(), String> {
    let output = Command::new(&plan.program)
        .args(&plan.args)
        .envs(plan.env.iter().map(|(key, value)| (key, value)))
        .current_dir(&plan.cwd)
        .output()
        .map_err(|error| format!("failed to run {}: {error}", plan.program))?;
    io::stderr()
        .write_all(&output.stdout)
        .map_err(|error| format!("failed to write command stdout: {error}"))?;
    io::stderr()
        .write_all(&output.stderr)
        .map_err(|error| format!("failed to write command stderr: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} exited with status {}",
            plan.program,
            output.status.code().unwrap_or(-1)
        ))
    }
}

pub fn run_plan_streaming(plan: &CommandPlan) -> Result<(), String> {
    let status = Command::new(&plan.program)
        .args(&plan.args)
        .envs(plan.env.iter().map(|(key, value)| (key, value)))
        .current_dir(&plan.cwd)
        .status()
        .map_err(|error| format!("failed to run {}: {error}", plan.program))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} exited with status {}",
            plan.program,
            status.code().unwrap_or(-1)
        ))
    }
}

pub fn doctor_checks(root: &Path, target: &TargetDefinition, port: Option<&str>) -> Vec<Value> {
    let mut checks = Vec::new();
    checks.push(check_path("target-json", &target.path));
    if let Ok(firmware) = target.firmware() {
        checks.push(check_path(
            "firmware-working-dir",
            &resolve_repo_path(root, &firmware.working_dir),
        ));
        checks.push(check_path(
            "partition-table",
            &resolve_repo_path(root, &firmware.partition_table),
        ));
        checks.push(check_path(
            "bootloader",
            &resolve_repo_path(root, &firmware.bootloader),
        ));
        checks.push(check_command_with_env("rustup", &["--version"], &[]));
        checks.push(check_command_with_env(
            &espflash_program(),
            &["--version"],
            &[],
        ));
    }
    let candidates = match port {
        Some(port) => vec![port.to_string()],
        None => crate::serial::candidate_ports(),
    };
    checks.push(json!({
        "name": "serial-visibility",
        "status": if candidates.is_empty() { "warn" } else { "ok" },
        "message": if candidates.is_empty() { "no serial candidates visible" } else { "serial candidates visible" },
        "details": {"candidates": candidates}
    }));
    checks
}

fn choose_target_interactively(root: &Path) -> Result<TargetDefinition, String> {
    let targets = load_targets(root)?;
    if targets.is_empty() {
        return Err("no target JSON files found under targets/".to_string());
    }
    eprintln!("Select target:");
    for (index, target) in targets.iter().enumerate() {
        eprintln!("  {}. {} ({})", index + 1, target.id, target.name);
    }
    eprint!("Target number: ");
    io::stderr()
        .flush()
        .map_err(|error| format!("failed to flush prompt: {error}"))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("failed to read target selection: {error}"))?;
    let selection = input
        .trim()
        .parse::<usize>()
        .map_err(|_| "invalid target selection".to_string())?;
    targets
        .get(selection.saturating_sub(1))
        .cloned()
        .ok_or_else(|| "target selection out of range".to_string())
}

pub fn stdin_is_interactive() -> bool {
    io::stdin().is_terminal()
}

fn espflash_program() -> String {
    let path = env::var_os("PATH").unwrap_or_default();
    for dir in env::split_paths(&path) {
        let candidate = dir.join("espflash");
        if candidate.is_file() {
            return candidate.display().to_string();
        }
    }
    if let Some(home) = env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".cargo/bin/espflash");
        if candidate.is_file() {
            return candidate.display().to_string();
        }
    }
    "espflash".to_string()
}

fn check_path(name: &str, path: &Path) -> Value {
    json!({
        "name": name,
        "status": if path.exists() { "ok" } else { "fail" },
        "message": if path.exists() {
            format!("{} exists", path.display())
        } else {
            format!("{} is missing", path.display())
        },
        "details": {"path": path}
    })
}

fn check_command_with_env(name: &str, args: &[&str], envs: &[(String, String)]) -> Value {
    match Command::new(name)
        .args(args)
        .envs(envs.iter().map(|(key, value)| (key, value)))
        .output()
    {
        Ok(output) if output.status.success() => json!({
            "name": name,
            "status": "ok",
            "message": String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or("available"),
            "details": {}
        }),
        Ok(output) => json!({
            "name": name,
            "status": "fail",
            "message": String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("command failed"),
            "details": {"status": output.status.code()}
        }),
        Err(error) => json!({
            "name": name,
            "status": "fail",
            "message": format!("missing command: {error}"),
            "details": {}
        }),
    }
}

fn resolve_repo_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_defaults_ble_connection_watchdog_when_omitted() {
        let firmware: Firmware = serde_json::from_value(json!({
            "package": "firmware",
            "workingDir": ".",
            "target": "riscv32imc-unknown-none-elf",
            "chip": "esp32c3",
            "elf": "target/firmware",
            "otaImage": "target/firmware.bin",
            "partitionTable": "targets/partitions/test.csv",
            "bootloader": "firmware/bootloader.bin"
        }))
        .unwrap();

        assert_eq!(firmware.ble_connection_watchdog_ms, 30_000);
    }

    #[test]
    fn x4_partition_table_matches_ota_geometry() {
        let root = repo_root();
        let entries =
            validate_native_partition_table(&root.join("targets/partitions/xteink-x4.csv"))
                .unwrap();

        assert_eq!(entries.len(), 6);
        let app_slots = entries
            .iter()
            .filter(|entry| entry.kind == "app")
            .collect::<Vec<_>>();
        assert_eq!(app_slots.len(), 2);
        assert_eq!(app_slots[0].size, app_slots[1].size);
        assert_eq!(app_slots[0].size, OTA_SLOT_BYTES);
        assert_eq!(
            entries.last().unwrap().offset + entries.last().unwrap().size,
            ESP32C3_FLASH_BYTES
        );
    }

    #[test]
    fn command_check_uses_injected_environment_path() {
        let scratch = repo_root().join("target/squidc-cli-test-doctor-env");
        let bin = scratch.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("test-tool");
        fs::write(&tool, "#!/bin/sh\nprintf 'tool 1.2.3\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let check = check_command_with_env(
            "test-tool",
            &["--version"],
            &[("PATH".to_string(), bin.display().to_string())],
        );

        assert_eq!(check["status"].as_str(), Some("ok"));
        assert_eq!(check["message"].as_str(), Some("tool 1.2.3"));
    }
}
