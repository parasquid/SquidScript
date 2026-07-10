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
    pub firmware: Option<TargetFirmware>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip)]
    pub zephyr: Option<ZephyrFirmware>,
    #[serde(skip)]
    pub native: Option<NativeFirmware>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl TargetDefinition {
    pub fn zephyr(&self) -> Result<&ZephyrFirmware, String> {
        self.zephyr
            .as_ref()
            .ok_or_else(|| format!("target {} has no Zephyr firmware metadata", self.id))
    }

    pub fn native(&self) -> Result<&NativeFirmware, String> {
        self.native
            .as_ref()
            .ok_or_else(|| format!("target {} has no native firmware metadata", self.id))
    }

    pub fn summary_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "status": self.status,
            "features": self.features,
            "path": self.path,
            "zephyrSupported": self.zephyr.is_some(),
            "zephyrBoard": self.zephyr.as_ref().map(|zephyr| zephyr.board.clone()),
            "nativeSupported": self.native.is_some(),
            "nativePackage": self.native.as_ref().map(|native| native.package.clone())
        })
    }

    pub fn inspect_json(&self, root: &Path) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "status": self.status,
            "features": self.features,
            "path": self.path,
            "zephyr": self.zephyr.as_ref().map(|zephyr| zephyr.resolved_json(root)),
            "native": self.native.as_ref().map(|native| native.resolved_json(root))
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TargetFirmware {
    #[serde(default)]
    pub zephyr: Option<ZephyrFirmware>,
    #[serde(default)]
    pub native: Option<NativeFirmware>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZephyrFirmware {
    pub board: String,
    pub build_dir: PathBuf,
    pub overlay: PathBuf,
    pub fallback_source: PathBuf,
    pub target_kconfig: PathBuf,
    #[serde(default)]
    pub runtime_limits: Option<PathBuf>,
}

impl ZephyrFirmware {
    fn resolved_json(&self, root: &Path) -> Value {
        json!({
            "board": self.board,
            "buildDir": resolve_repo_path(root, &self.build_dir),
            "overlay": resolve_repo_path(root, &self.overlay),
            "fallbackSource": resolve_repo_path(root, &self.fallback_source),
            "targetKconfig": resolve_repo_path(root, &self.target_kconfig),
            "runtimeLimits": self.runtime_limits.as_ref().map(|path| resolve_repo_path(root, path)),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFirmware {
    pub package: String,
    pub working_dir: PathBuf,
    pub target: String,
    pub chip: String,
    pub elf: PathBuf,
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

impl NativeFirmware {
    fn resolved_json(&self, root: &Path) -> Value {
        json!({
            "package": self.package,
            "workingDir": resolve_repo_path(root, &self.working_dir),
            "target": self.target,
            "chip": self.chip,
            "elf": resolve_repo_path(root, &self.elf),
            "features": self.features,
            "release": self.release,
            "rustupToolchain": self.rustup_toolchain,
            "bleConnectionWatchdogMs": self.ble_connection_watchdog_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[allow(dead_code)]
pub enum TargetBackend {
    Zephyr,
    Native,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetPristine {
    Auto,
    Always,
    Never,
}

impl TargetPristine {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TargetBuildPlanOptions {
    pub backend: TargetBackend,
    pub stack_usage: bool,
    pub pristine: TargetPristine,
    pub west_args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TargetFlashPlanOptions {
    pub backend: TargetBackend,
    pub west_args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TargetMonitorPlanOptions {
    pub backend: TargetBackend,
    pub port: Option<String>,
    pub west_args: Vec<String>,
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
        target.zephyr = target
            .firmware
            .as_ref()
            .and_then(|firmware| firmware.zephyr.clone());
        target.native = target
            .firmware
            .as_ref()
            .and_then(|firmware| firmware.native.clone());
        target.path = path;
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
    match options.backend {
        TargetBackend::Zephyr => plan_zephyr_build_command(root, target, options),
        TargetBackend::Native => plan_native_build_command(root, target, options),
    }
}

fn plan_zephyr_build_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetBuildPlanOptions,
) -> Result<CommandPlan, String> {
    let zephyr = target.zephyr()?;
    let build_dir = resolve_repo_path(root, &zephyr.build_dir);
    let overlay = resolve_repo_path(root, &zephyr.overlay);
    let fallback = resolve_repo_path(root, &zephyr.fallback_source);
    let target_json = target.path.clone();
    let kconfig = resolve_repo_path(root, &zephyr.target_kconfig);
    let extra_conf = match env::var("ZEPHYR_EXTRA_CONF_FILE") {
        Ok(extra) if !extra.is_empty() => format!("{};{}", kconfig.display(), extra),
        _ => kconfig.display().to_string(),
    };

    let mut args = vec![
        "build".to_string(),
        "--build-dir".to_string(),
        build_dir.display().to_string(),
        "--board".to_string(),
        zephyr.board.clone(),
        "--pristine".to_string(),
        options.pristine.as_str().to_string(),
        root.join("firmware/zephyr").display().to_string(),
    ];
    args.extend(options.west_args);
    args.push("--".to_string());
    args.push(format!("-DDTC_OVERLAY_FILE={}", overlay.display()));
    args.push(format!(
        "-DSQUID_ZEPHYR_TARGET_JSON={}",
        target_json.display()
    ));
    args.push(format!(
        "-DSQUID_ZEPHYR_TARGET_OVERLAY={}",
        overlay.display()
    ));
    args.push(format!(
        "-DSQUID_ZEPHYR_FALLBACK_SOURCE={}",
        fallback.display()
    ));
    args.push(format!("-DEXTRA_CONF_FILE={extra_conf}"));
    if options.stack_usage {
        args.push("-DSQUID_ZEPHYR_STACK_USAGE=ON".to_string());
    }

    Ok(CommandPlan {
        program: "west".to_string(),
        args,
        cwd: root.to_path_buf(),
        env: zephyr_env(root, target, options.stack_usage)?,
    })
}

fn plan_native_build_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetBuildPlanOptions,
) -> Result<CommandPlan, String> {
    let native = target.native()?;
    if options.stack_usage {
        return Err(
            "target build --stack-usage is only supported for the Zephyr backend".to_string(),
        );
    }
    if options.pristine != TargetPristine::Auto {
        return Err("target build --pristine is only supported for the Zephyr backend".to_string());
    }
    let mut args = vec![
        "build".to_string(),
        "-p".to_string(),
        native.package.clone(),
        "--target".to_string(),
        native.target.clone(),
    ];
    if !native.features.is_empty() {
        args.push("--features".to_string());
        args.push(native.features.join(","));
    }
    if native.release {
        args.push("--release".to_string());
    }
    args.extend(options.west_args);

    let (program, args, env) = match native.rustup_toolchain.as_deref() {
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
                        native.ble_connection_watchdog_ms.to_string(),
                    ),
                ],
            )
        }
        None => (
            "cargo".to_string(),
            args,
            vec![(
                "SQUIDSCRIPT_BLE_CONNECTION_WATCHDOG_MS".to_string(),
                native.ble_connection_watchdog_ms.to_string(),
            )],
        ),
    };

    Ok(CommandPlan {
        program,
        args,
        cwd: resolve_repo_path(root, &native.working_dir),
        env,
    })
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
    match options.backend {
        TargetBackend::Zephyr => plan_zephyr_flash_command(root, target, options),
        TargetBackend::Native => plan_native_flash_command(root, target, options),
    }
}

fn plan_zephyr_flash_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetFlashPlanOptions,
) -> Result<CommandPlan, String> {
    let zephyr = target.zephyr()?;
    let mut args = vec![
        "flash".to_string(),
        "--build-dir".to_string(),
        resolve_repo_path(root, &zephyr.build_dir)
            .display()
            .to_string(),
    ];
    args.extend(options.west_args);
    Ok(CommandPlan {
        program: "west".to_string(),
        args,
        cwd: root.to_path_buf(),
        env: zephyr_env(root, target, false)?,
    })
}

fn plan_native_flash_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetFlashPlanOptions,
) -> Result<CommandPlan, String> {
    let native = target.native()?;
    let mut args = vec![
        "flash".to_string(),
        "--chip".to_string(),
        native.chip.clone(),
        "--non-interactive".to_string(),
        resolve_repo_path(root, &native.elf).display().to_string(),
    ];
    args.extend(options.west_args);
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
    match options.backend {
        TargetBackend::Zephyr => plan_zephyr_monitor_command(root, target, options),
        TargetBackend::Native => plan_native_monitor_command(root, target, options),
    }
}

fn plan_zephyr_monitor_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetMonitorPlanOptions,
) -> Result<CommandPlan, String> {
    let zephyr = target.zephyr()?;
    let port = options.port.unwrap_or_else(|| "<auto-detect>".to_string());
    let mut args = vec![
        "espressif".to_string(),
        "monitor".to_string(),
        "-p".to_string(),
        port,
        "-e".to_string(),
        "zephyr/zephyr.elf".to_string(),
    ];
    args.extend(options.west_args);
    Ok(CommandPlan {
        program: "west".to_string(),
        args,
        cwd: resolve_repo_path(root, &zephyr.build_dir),
        env: zephyr_env(root, target, false)?,
    })
}

fn plan_native_monitor_command(
    root: &Path,
    target: &TargetDefinition,
    options: TargetMonitorPlanOptions,
) -> Result<CommandPlan, String> {
    let native = target.native()?;
    let mut args = vec![
        "monitor".to_string(),
        "--chip".to_string(),
        native.chip.clone(),
        "--non-interactive".to_string(),
    ];
    if let Some(port) = options.port {
        args.push("--port".to_string());
        args.push(port);
    }
    args.extend(options.west_args);
    Ok(CommandPlan {
        program: espflash_program(),
        args,
        cwd: root.to_path_buf(),
        env: Vec::new(),
    })
}

#[allow(dead_code)]
pub fn ensure_target_kconfig(root: &Path, target: &TargetDefinition) -> Result<PathBuf, String> {
    let zephyr = target.zephyr()?;
    let out = resolve_repo_path(root, &zephyr.target_kconfig);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let script = root.join("scripts/generate-zephyr-target-kconfig.py");
    let output = Command::new(&script)
        .arg(&target.path)
        .arg(&out)
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run {}: {error}", script.display()))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .next()
            .unwrap_or("target Kconfig generation failed")
            .to_string());
    }
    Ok(out)
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
    match target.zephyr() {
        Ok(zephyr) => {
            checks.push(check_path(
                "zephyr-overlay",
                &resolve_repo_path(root, &zephyr.overlay),
            ));
            checks.push(check_path(
                "fallback-source",
                &resolve_repo_path(root, &zephyr.fallback_source),
            ));
            let envs = zephyr_env(root, target, false).unwrap_or_default();
            checks.push(check_command_with_env("west", &["--version"], &envs));
            checks.push(check_path("firmware-dir", &root.join("firmware/zephyr")));
            let candidates = match port {
                Some(port) => vec![port.to_string()],
                None => crate::serial::candidate_ports(),
            };
            checks.push(json!({
                "name": "serial-visibility",
                "status": if candidates.is_empty() { "warn" } else { "ok" },
                "message": if candidates.is_empty() {
                    "no serial candidates visible"
                } else {
                    "serial candidates visible"
                },
                "details": {"candidates": candidates}
            }));
        }
        Err(error) => checks.push(json!({
            "name": "zephyr-metadata",
            "status": "fail",
            "message": error,
            "details": {}
        })),
    }
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

fn zephyr_env(
    root: &Path,
    target: &TargetDefinition,
    stack_usage: bool,
) -> Result<Vec<(String, String)>, String> {
    let zephyr = target.zephyr()?;
    let zephyr_home = env::var("SQUID_ZEPHYR_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target/zephyr"));
    let mut path = format!(
        "{}:{}",
        zephyr_home.join("venv/bin").display(),
        env::var("PATH").unwrap_or_default()
    );
    if path.ends_with(':') {
        path.pop();
    }
    let mut envs = vec![
        (
            "SQUID_ZEPHYR_HOME".to_string(),
            zephyr_home.display().to_string(),
        ),
        ("PATH".to_string(), path),
        ("ZEPHYR_BOARD".to_string(), zephyr.board.clone()),
        (
            "ZEPHYR_BUILD_DIR".to_string(),
            resolve_repo_path(root, &zephyr.build_dir)
                .display()
                .to_string(),
        ),
        (
            "SQUID_ZEPHYR_TARGET_JSON".to_string(),
            target.path.display().to_string(),
        ),
        (
            "SQUID_ZEPHYR_TARGET_OVERLAY".to_string(),
            resolve_repo_path(root, &zephyr.overlay)
                .display()
                .to_string(),
        ),
        (
            "SQUID_ZEPHYR_FALLBACK_SOURCE".to_string(),
            resolve_repo_path(root, &zephyr.fallback_source)
                .display()
                .to_string(),
        ),
    ];
    let zephyr_base = zephyr_home.join("workspace/zephyr");
    if zephyr_base.is_dir() {
        envs.push(("ZEPHYR_BASE".to_string(), zephyr_base.display().to_string()));
    }
    if stack_usage {
        envs.push(("SQUID_ZEPHYR_STACK_USAGE".to_string(), "1".to_string()));
    }
    if env::var_os("ZEPHYR_SDK_INSTALL_DIR").is_none() {
        if let Some(sdk) = find_zephyr_sdk(root, &zephyr_home) {
            envs.push((
                "ZEPHYR_SDK_INSTALL_DIR".to_string(),
                sdk.display().to_string(),
            ));
        }
    }
    Ok(envs)
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

fn find_zephyr_sdk(root: &Path, zephyr_home: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(zephyr_home.to_path_buf());
    candidates.push(zephyr_home.join("sdk"));
    if let Some(home) = env::var_os("HOME") {
        candidates.push(PathBuf::from(home));
    }
    candidates.push(PathBuf::from("/opt"));
    candidates.push(root.join("target/zephyr"));

    for base in candidates {
        let Ok(entries) = fs::read_dir(base) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.starts_with("zephyr-sdk-"))
            {
                return Some(path);
            }
        }
    }
    None
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
    fn native_firmware_defaults_ble_connection_watchdog_when_omitted() {
        let firmware: NativeFirmware = serde_json::from_value(json!({
            "package": "firmware",
            "workingDir": ".",
            "target": "riscv32imc-unknown-none-elf",
            "chip": "esp32c3",
            "elf": "target/firmware"
        }))
        .unwrap();

        assert_eq!(firmware.ble_connection_watchdog_ms, 30_000);
    }

    #[test]
    fn command_check_uses_injected_environment_path() {
        let scratch = repo_root().join("target/squidc-cli-test-doctor-env");
        let bin = scratch.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let west = bin.join("west");
        fs::write(&west, "#!/bin/sh\nprintf 'west 1.2.3\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&west, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let check = check_command_with_env(
            "west",
            &["--version"],
            &[("PATH".to_string(), bin.display().to_string())],
        );

        assert_eq!(check["status"].as_str(), Some("ok"));
        assert_eq!(check["message"].as_str(), Some("west 1.2.3"));
    }
}
