mod app_id;
mod compile;
mod package;
mod serial;

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use app_id::{generated_app_id, source_app_id, source_for_compile};
use clap::{error::ErrorKind, Args, Parser, Subcommand, ValueEnum};
use compile::{compile_source_to_sqbc, compile_target_id};
use package::{package_app_dir, read_stored_zip_entries};
use serde::Serialize;
use serde_json::{json, Value};
use serial::{candidate_ports, detect_port, OutputTail, SerialDevice};
use squidc_core::profile::BuildProfile;

fn main() {
    let raw_args = env::args().collect::<Vec<_>>();
    let wants_json = raw_args.iter().any(|arg| arg == "--json");
    let cli = match Cli::try_parse_from(raw_args) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            std::process::exit(0);
        }
        Err(error) => {
            if wants_json {
                write_json_error("parse", error.to_string());
            } else {
                eprint!("{error}");
            }
            std::process::exit(2);
        }
    };
    let json = cli.json;
    let command = cli.command;
    let command_name = command.name();
    match run(command, !json, json) {
        Ok(data) => {
            if json {
                write_json_success(command_name, data);
            }
        }
        Err(error) => {
            if json {
                write_json_error(command_name, error);
            } else {
                eprintln!("squidc: {error}");
            }
            std::process::exit(1);
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "squidc",
    version,
    about = "SquidScript compiler and reference firmware CLI"
)]
struct Cli {
    #[arg(long, global = true, help = "Emit stable JSON envelope output")]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Build(BuildArgs),
    Package(PackageArgs),
    Run(DeviceSourceArgs),
    Repl(ReplArgs),
    Doctor(DoctorArgs),
    App {
        #[command(subcommand)]
        command: AppCommands,
    },
    Device {
        #[command(subcommand)]
        command: DeviceCommands,
    },
    Protocol {
        #[command(subcommand)]
        command: ProtocolCommands,
    },
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Self::Build(_) => "build",
            Self::Package(_) => "package",
            Self::Run(_) => "run",
            Self::Repl(_) => "repl",
            Self::Doctor(_) => "doctor",
            Self::App { .. } => "app",
            Self::Device { .. } => "device",
            Self::Protocol { .. } => "protocol",
        }
    }
}

#[derive(Subcommand, Debug)]
enum AppCommands {
    Install(AppInstallArgs),
    Launch(AppLaunchArgs),
    List(DeviceOnlyArgs),
}

#[derive(Subcommand, Debug)]
enum DeviceCommands {
    Key(DeviceKeyArgs),
    Reset(DeviceOnlyArgs),
    Output(DeviceOnlyArgs),
    State(DeviceOnlyArgs),
    Drawlog(DeviceOnlyArgs),
    Trace(DeviceOnlyArgs),
    Errors(DeviceOnlyArgs),
    Resources(DeviceOnlyArgs),
    Monitor(MonitorArgs),
}

#[derive(Subcommand, Debug)]
enum ProtocolCommands {
    Raw(ProtocolRawArgs),
}

#[derive(Args, Debug)]
struct BuildArgs {
    input: PathBuf,
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    check_target: bool,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, value_enum, default_value_t = ProfileArg::Dev)]
    profile: ProfileArg,
}

#[derive(Args, Debug)]
struct PackageArgs {
    input: PathBuf,
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    check_target: bool,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = ProfileArg::Dev)]
    profile: ProfileArg,
}

#[derive(Args, Debug)]
struct DeviceSourceArgs {
    #[command(flatten)]
    device: DeviceOptions,
    input: PathBuf,
}

#[derive(Args, Debug)]
struct AppInstallArgs {
    #[command(flatten)]
    device: DeviceOptions,
    #[arg(long = "as")]
    app_id_override: Option<String>,
    input: PathBuf,
}

#[derive(Args, Debug)]
struct AppLaunchArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    app_id: String,
}

#[derive(Args, Debug)]
struct DeviceKeyArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    key: String,
}

#[derive(Args, Debug)]
struct DeviceOnlyArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
}

#[derive(Args, Debug)]
struct ProtocolRawArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    line: String,
}

#[derive(Args, Debug)]
struct MonitorArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    #[arg(long)]
    raw: bool,
    #[arg(long, default_value_t = 500)]
    poll_ms: u64,
    #[arg(long)]
    max_lines: Option<usize>,
}

#[derive(Args, Debug)]
struct ReplArgs {
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    check_target: bool,
    #[arg(long)]
    port: Option<String>,
    #[arg(long)]
    script: PathBuf,
    input_file: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct DoctorArgs {
    #[arg(long)]
    port: Option<String>,
}

#[derive(Args, Clone, Debug)]
struct DeviceOptions {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    check_target: bool,
    #[arg(long, value_enum, default_value_t = ProfileArg::Dev)]
    profile: ProfileArg,
}

#[derive(Args, Clone, Debug)]
struct DeviceOnlyOptions {
    #[arg(long)]
    port: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProfileArg {
    Dev,
    Release,
}

impl From<ProfileArg> for BuildProfile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::Dev => BuildProfile::Dev,
            ProfileArg::Release => BuildProfile::Release,
        }
    }
}

fn run(command: Commands, human: bool, json_mode: bool) -> Result<Value, String> {
    match command {
        Commands::Build(args) => build(args),
        Commands::Package(args) => package_app(args),
        Commands::Run(args) => run_app_source(args, human),
        Commands::Repl(args) => repl(args, human),
        Commands::Doctor(args) => doctor(args, human),
        Commands::App { command } => match command {
            AppCommands::Install(args) => install_app(args, human),
            AppCommands::Launch(args) => launch_app(args, human),
            AppCommands::List(args) => device_block_command(args.device, "APP.LIST", "apps", human),
        },
        Commands::Device { command } => match command {
            DeviceCommands::Key(args) => key(args, human),
            DeviceCommands::Reset(args) => {
                device_line_command(args.device, "RESET", "reset", human)
            }
            DeviceCommands::Output(args) => {
                device_block_command(args.device, "OUTPUT.GET", "output", human)
            }
            DeviceCommands::State(args) => {
                device_block_command(args.device, "STATE.GET", "state", human)
            }
            DeviceCommands::Drawlog(args) => {
                device_block_command(args.device, "DRAWLOG.GET", "drawlog", human)
            }
            DeviceCommands::Trace(args) => {
                device_block_command(args.device, "TRACE.GET", "trace", human)
            }
            DeviceCommands::Errors(args) => {
                device_block_command(args.device, "ERRORS.GET", "errors", human)
            }
            DeviceCommands::Resources(args) => resources(args, human),
            DeviceCommands::Monitor(args) => monitor(args, json_mode),
        },
        Commands::Protocol { command } => match command {
            ProtocolCommands::Raw(args) => protocol_raw(args, human),
        },
    }
}

fn package_app(args: PackageArgs) -> Result<Value, String> {
    let target = compile_target_id(args.target.as_deref(), args.check_target)?;
    let result = package_app_dir(
        &args.input,
        args.out.as_deref(),
        &target,
        args.profile.into(),
    )?;
    Ok(json!({
        "input": args.input,
        "out": result.out,
        "appId": result.app_id,
        "target": target,
        "files": result.entries,
        "bytes": result.bytes
    }))
}

fn build(args: BuildArgs) -> Result<Value, String> {
    let target = compile_target_id(args.target.as_deref(), args.check_target)?;
    let bytes = compile_source_to_sqbc(
        &fs::read_to_string(&args.input)
            .map_err(|error| format!("failed to read {}: {error}", args.input.display()))?,
        &target,
        args.profile.into(),
    )?;
    if let Some(parent) = args
        .out
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&args.out, &bytes)
        .map_err(|error| format!("failed to write {}: {error}", args.out.display()))?;
    Ok(json!({
        "input": args.input,
        "out": args.out,
        "target": target,
        "bytes": bytes.len()
    }))
}

fn run_app_source(args: DeviceSourceArgs, human: bool) -> Result<Value, String> {
    let source = fs::read_to_string(&args.input)
        .map_err(|error| format!("failed to read {}: {error}", args.input.display()))?;
    let app_id = source_app_id(&source).unwrap_or_else(|| generated_app_id(&args.input, &source));
    let source = source_for_compile(&source, &app_id);
    let target = compile_target_id(args.device.target.as_deref(), args.device.check_target)?;
    let sqbc = compile_source_to_sqbc(&source, &target, args.device.profile.into())?;
    let port = resolve_port(&args.device.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.run_temp_app(&app_id, &sqbc)?;
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "mode": "temp",
        "sourceAppId": app_id,
        "target": target,
        "bytes": sqbc.len(),
        "response": response
    }))
}

fn install_app(args: AppInstallArgs, human: bool) -> Result<Value, String> {
    if args
        .input
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.ends_with(".squid.zip"))
    {
        return install_app_package(args, human);
    }
    let (bytes, app_id) =
        read_installable_app(&args.input, args.app_id_override.as_deref(), &args.device)?;
    let port = resolve_port(&args.device.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.install_app(&app_id, &bytes)?;
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "appId": app_id,
        "bytes": bytes.len(),
        "response": response
    }))
}

fn install_app_package(args: AppInstallArgs, human: bool) -> Result<Value, String> {
    if args.app_id_override.is_some() {
        return Err("--as is not supported when installing .squid.zip packages".to_string());
    }
    let bytes = fs::read(&args.input)
        .map_err(|error| format!("failed to read {}: {error}", args.input.display()))?;
    let mut entries = read_stored_zip_entries(&bytes)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let main_index = entries
        .iter()
        .position(|entry| entry.path == "main.sqbc")
        .ok_or_else(|| "package is missing main.sqbc".to_string())?;
    let main = entries.remove(main_index);
    let app_id = squidc_core::sqbc_v2::read_app_id(&main.bytes)
        .map_err(|error| error.message)?
        .ok_or_else(|| "SQBC has no app id metadata".to_string())?;

    let port = resolve_port(&args.device.device)?;
    let mut device = SerialDevice::open(&port)?;
    let mut response = device.install_app(&app_id, &main.bytes)?;
    for entry in &entries {
        response.push_str(&device.install_resource(&app_id, &entry.path, &entry.bytes)?);
    }
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "appId": app_id,
        "bytes": bytes.len(),
        "files": entries.len() + 1,
        "response": response
    }))
}

fn read_installable_app(
    input: &Path,
    override_id: Option<&str>,
    options: &DeviceOptions,
) -> Result<(Vec<u8>, String), String> {
    if input.extension().and_then(|value| value.to_str()) == Some("sqbc") {
        let bytes = fs::read(input)
            .map_err(|error| format!("failed to read {}: {error}", input.display()))?;
        let app_id = match override_id {
            Some(app_id) => app_id.to_string(),
            None => squidc_core::sqbc_v2::read_app_id(&bytes)
                .map_err(|error| error.message)?
                .ok_or_else(|| "SQBC has no app id metadata; pass --as <appId>".to_string())?,
        };
        return Ok((bytes, app_id));
    }

    let source = fs::read_to_string(input)
        .map_err(|error| format!("failed to read {}: {error}", input.display()))?;
    let app_id = override_id
        .map(ToOwned::to_owned)
        .or_else(|| source_app_id(&source))
        .unwrap_or_else(|| generated_app_id(input, &source));
    let source = source_for_compile(&source, &app_id);
    let target = compile_target_id(options.target.as_deref(), options.check_target)?;
    let bytes = compile_source_to_sqbc(&source, &target, options.profile.into())?;
    Ok((bytes, app_id))
}

fn launch_app(args: AppLaunchArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.run_app(&args.app_id)?;
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "appId": args.app_id,
        "response": response
    }))
}

fn key(args: DeviceKeyArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_line(&format!("KEY {}", args.key))?;
    if human {
        print!("{response}");
    }
    if !response.contains("OK key") {
        return Err(response.trim().to_string());
    }
    Ok(json!({
        "port": port,
        "key": args.key,
        "response": response
    }))
}

fn protocol_raw(args: ProtocolRawArgs, human: bool) -> Result<Value, String> {
    device_line_command(args.device, &args.line, "protocol.raw", human)
}

fn resources(args: DeviceOnlyArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_line("RESOURCES.GET")?;
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "resources": parse_key_value_block(&response, "RESOURCES"),
        "response": response
    }))
}

fn monitor(args: MonitorArgs, json_mode: bool) -> Result<Value, String> {
    if json_mode && args.max_lines.is_none() {
        return Err("device monitor --json requires --max-lines".to_string());
    }
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    if !json_mode {
        eprintln!("squidc: monitoring {port}; press Ctrl+C to stop");
    }
    let lines = if args.raw {
        monitor_raw(&mut device, args.max_lines, json_mode)?
    } else {
        monitor_output(
            &mut device,
            Duration::from_millis(args.poll_ms),
            args.max_lines,
            json_mode,
        )?
    };
    Ok(json!({
        "port": port,
        "raw": args.raw,
        "lines": lines
    }))
}

fn monitor_raw(
    device: &mut SerialDevice,
    max_lines: Option<usize>,
    collect_only: bool,
) -> Result<Vec<String>, String> {
    let mut printed = 0usize;
    let mut lines = Vec::new();
    loop {
        let chunk = device.read_available_text()?;
        if !chunk.is_empty() {
            let chunk_lines = chunk.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
            printed += chunk_lines.len().max(1);
            lines.extend(chunk_lines);
            if !collect_only {
                print!("{chunk}");
                io::stdout()
                    .flush()
                    .map_err(|error| format!("stdout flush failed: {error}"))?;
            }
            if max_lines.is_some_and(|max| printed >= max) {
                return Ok(lines);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn monitor_output(
    device: &mut SerialDevice,
    poll_interval: Duration,
    max_lines: Option<usize>,
    collect_only: bool,
) -> Result<Vec<String>, String> {
    let mut tail = OutputTail::new();
    let mut printed = 0usize;
    let mut lines = Vec::new();
    loop {
        let response = device.send_line("OUTPUT.GET")?;
        for line in tail.next_lines(&response) {
            if !collect_only {
                println!("{line}");
            }
            lines.push(line);
            printed += 1;
            if max_lines.is_some_and(|max| printed >= max) {
                return Ok(lines);
            }
        }
        if !collect_only {
            io::stdout()
                .flush()
                .map_err(|error| format!("stdout flush failed: {error}"))?;
        }
        std::thread::sleep(poll_interval);
    }
}

fn device_line_command(
    options: DeviceOnlyOptions,
    command: &str,
    label: &str,
    human: bool,
) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_line(command)?;
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "command": label,
        "response": response
    }))
}

fn device_block_command(
    options: DeviceOnlyOptions,
    command: &str,
    label: &str,
    human: bool,
) -> Result<Value, String> {
    device_line_command(options, command, label, human)
}

fn repl(args: ReplArgs, human: bool) -> Result<Value, String> {
    let target = compile_target_id(args.target.as_deref(), args.check_target)?;
    let port = match args.port.or_else(|| env::var("ESPFLASH_PORT").ok()) {
        Some(port) => port,
        None => detect_port()?,
    };
    let script_text = fs::read_to_string(&args.script)
        .map_err(|error| format!("failed to read {}: {error}", args.script.display()))?;
    if args.script.extension().and_then(|value| value.to_str()) == Some("squid") {
        let mut session = ReplSession::new(target, port.clone(), script_text, human);
        session.reload_base_source()?;
        return Ok(json!({"port": port, "mode": "reload"}));
    }
    let base_source = match args.input_file {
        Some(path) => fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?,
        None => String::new(),
    };
    ReplSession::new(target, port.clone(), base_source, human).run_script(&script_text)?;
    Ok(json!({"port": port, "script": args.script}))
}

#[derive(Serialize)]
struct DoctorCheck {
    name: &'static str,
    status: &'static str,
    message: String,
    details: Value,
}

fn doctor(args: DoctorArgs, human: bool) -> Result<Value, String> {
    let mut checks = Vec::new();
    checks.push(command_check("cargo", &["--version"], true));
    checks.push(command_check("rustc", &["--version"], true));
    checks.push(rustup_check());
    checks.push(rust_target_check("riscv32imc-unknown-none-elf"));
    checks.push(riscv_c_toolchain_check());
    checks.push(espflash_check());
    checks.push(espflash_ports_check());
    checks.push(command_check("riscv64-elf-size", &["--version"], false));
    checks.push(script_check("scripts/c3-supermini-test-hardware.sh"));
    checks.push(serial_visibility_check(args.port.as_deref()));
    checks.push(firmware_probe_check(args.port.as_deref()));

    let failed = checks.iter().any(|check| check.status == "fail");
    let warning = checks.iter().any(|check| check.status == "warn");
    let summary = if failed {
        "fail"
    } else if warning {
        "warn"
    } else {
        "ok"
    };

    if human {
        for check in &checks {
            println!("[{}] {}: {}", check.status, check.name, check.message);
        }
    }

    Ok(json!({
        "summary": summary,
        "sandboxNote": "Hardware target tests and serial/flashing commands must run outside the Codex sandbox.",
        "checks": checks
    }))
}

fn command_check(name: &'static str, args: &[&str], required: bool) -> DoctorCheck {
    match Command::new(name).args(args).output() {
        Ok(output) if output.status.success() => DoctorCheck {
            name,
            status: "ok",
            message: String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("available")
                .to_string(),
            details: json!({"required": required}),
        },
        Ok(output) => DoctorCheck {
            name,
            status: if required { "fail" } else { "warn" },
            message: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("command failed")
                .to_string(),
            details: json!({"required": required, "status": output.status.code()}),
        },
        Err(error) => DoctorCheck {
            name,
            status: if required { "fail" } else { "warn" },
            message: if required {
                format!("missing required command: {error}")
            } else {
                format!("optional command missing: {error}")
            },
            details: json!({"required": required}),
        },
    }
}

fn rustup_check() -> DoctorCheck {
    match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(output) if output.status.success() => DoctorCheck {
            name: "rustup",
            status: "ok",
            message: "rustup can list installed targets".to_string(),
            details: json!({"required": true}),
        },
        Ok(output) if String::from_utf8_lossy(&output.stdout).starts_with("rustup ") => {
            DoctorCheck {
                name: "rustup",
                status: "warn",
                message:
                    "rustup is available, but version probing is unstable in this host context"
                        .to_string(),
                details: json!({"required": true, "status": output.status.code()}),
            }
        }
        Ok(output) => DoctorCheck {
            name: "rustup",
            status: "fail",
            message: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("rustup failed")
                .to_string(),
            details: json!({"required": true, "status": output.status.code()}),
        },
        Err(error) => DoctorCheck {
            name: "rustup",
            status: "fail",
            message: format!("missing required command: {error}"),
            details: json!({"required": true}),
        },
    }
}

fn espflash_check() -> DoctorCheck {
    match espflash_path().and_then(|path| {
        Command::new(&path)
            .arg("--version")
            .output()
            .ok()
            .map(|output| (path, output))
    }) {
        Some((path, output)) if output.status.success() => DoctorCheck {
            name: "espflash",
            status: "ok",
            message: String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("available")
                .to_string(),
            details: json!({"path": path}),
        },
        Some((path, output)) => DoctorCheck {
            name: "espflash",
            status: "fail",
            message: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("espflash failed")
                .to_string(),
            details: json!({"path": path, "status": output.status.code()}),
        },
        None => DoctorCheck {
            name: "espflash",
            status: "fail",
            message: "missing required command; install espflash or add ~/.cargo/bin to PATH"
                .to_string(),
            details: json!({"required": true}),
        },
    }
}

fn espflash_ports_check() -> DoctorCheck {
    let Some(path) = espflash_path() else {
        return DoctorCheck {
            name: "espflash-ports",
            status: "warn",
            message: "skipped because espflash is unavailable".to_string(),
            details: json!({}),
        };
    };
    match Command::new(&path)
        .args(["list-ports", "--list-all-ports"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let ports = stdout
                .lines()
                .filter(|line| line.trim_start().starts_with("/dev/"))
                .map(|line| line.trim().to_string())
                .collect::<Vec<_>>();
            DoctorCheck {
                name: "espflash-ports",
                status: if ports.is_empty() { "warn" } else { "ok" },
                message: if ports.is_empty() {
                    "espflash saw no serial ports".to_string()
                } else {
                    format!("espflash ports: {}", ports.join(", "))
                },
                details: json!({"path": path, "ports": ports}),
            }
        }
        Ok(output) => DoctorCheck {
            name: "espflash-ports",
            status: "warn",
            message: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("espflash list-ports failed")
                .to_string(),
            details: json!({"path": path, "status": output.status.code()}),
        },
        Err(error) => DoctorCheck {
            name: "espflash-ports",
            status: "warn",
            message: format!("failed to run espflash list-ports: {error}"),
            details: json!({"path": path}),
        },
    }
}

fn espflash_path() -> Option<PathBuf> {
    if Command::new("espflash").arg("--version").output().is_ok() {
        return Some(PathBuf::from("espflash"));
    }
    let home_path = env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cargo/bin/espflash"));
    home_path.filter(|path| path.exists())
}

fn rust_target_check(target: &'static str) -> DoctorCheck {
    match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let installed = String::from_utf8_lossy(&output.stdout);
            let present = installed.lines().any(|line| line.trim() == target);
            DoctorCheck {
                name: "rust-target",
                status: if present { "ok" } else { "fail" },
                message: if present {
                    format!("{target} installed")
                } else {
                    format!("{target} missing; run rustup target add {target}")
                },
                details: json!({"target": target}),
            }
        }
        Ok(output) => DoctorCheck {
            name: "rust-target",
            status: "fail",
            message: String::from_utf8_lossy(&output.stderr).to_string(),
            details: json!({"target": target}),
        },
        Err(error) => DoctorCheck {
            name: "rust-target",
            status: "fail",
            message: format!("failed to run rustup: {error}"),
            details: json!({"target": target}),
        },
    }
}

fn riscv_c_toolchain_check() -> DoctorCheck {
    let mut candidates = vec![
        PathBuf::from("riscv32-unknown-elf-gcc"),
        PathBuf::from("riscv64-elf-gcc"),
    ];
    if let Ok(output) = Command::new("brew")
        .args(["--prefix", "riscv64-elf-gcc"])
        .output()
    {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !prefix.is_empty() {
                candidates.push(PathBuf::from(prefix).join("bin/riscv64-elf-gcc"));
            }
        }
    }
    for candidate in candidates {
        if Command::new(&candidate).arg("--version").output().is_ok() {
            return DoctorCheck {
                name: "riscv-c-toolchain",
                status: "ok",
                message: format!("{} is available for LittleFS C build", candidate.display()),
                details: json!({
                    "required": true,
                    "path": candidate,
                    "note": "riscv64-elf-gcc is used with -march=rv32imc -mabi=ilp32 by scripts/c3-supermini-build.sh"
                }),
            };
        }
    }
    DoctorCheck {
        name: "riscv-c-toolchain",
        status: "fail",
        message: "missing RISC-V ELF GCC required by littlefs2-sys; install riscv32-unknown-elf-gcc, put riscv64-elf-gcc on PATH, or install Homebrew riscv64-elf-gcc".to_string(),
        details: json!({"required": true}),
    }
}

fn script_check(path: &'static str) -> DoctorCheck {
    let exists = Path::new(path).exists();
    DoctorCheck {
        name: "hardware-test-script",
        status: if exists { "ok" } else { "fail" },
        message: if exists {
            format!("{path} exists")
        } else {
            format!("{path} is missing")
        },
        details: json!({"path": path}),
    }
}

fn serial_visibility_check(port: Option<&str>) -> DoctorCheck {
    let candidates = match port {
        Some(port) => vec![port.to_string()],
        None => candidate_ports(),
    };
    DoctorCheck {
        name: "serial-visibility",
        status: if candidates.is_empty() { "warn" } else { "ok" },
        message: if candidates.is_empty() {
            "no serial candidates visible; run hardware checks outside the Codex sandbox and set ESPFLASH_PORT if needed".to_string()
        } else {
            format!("visible candidates: {}", candidates.join(", "))
        },
        details: json!({"candidates": candidates}),
    }
}

fn firmware_probe_check(port: Option<&str>) -> DoctorCheck {
    let candidates = match port {
        Some(port) => vec![port.to_string()],
        None => candidate_ports(),
    };
    if candidates.len() != 1 {
        return DoctorCheck {
            name: "firmware-hello",
            status: "warn",
            message: "skipped; pass --port or expose exactly one serial candidate".to_string(),
            details: json!({"candidates": candidates}),
        };
    }
    let port = &candidates[0];
    match SerialDevice::probe(port) {
        Ok(true) => DoctorCheck {
            name: "firmware-hello",
            status: "ok",
            message: format!("{port} responded to HELLO"),
            details: json!({"port": port}),
        },
        Ok(false) => DoctorCheck {
            name: "firmware-hello",
            status: "warn",
            message: format!("{port} did not look like SquidScript firmware"),
            details: json!({"port": port}),
        },
        Err(error) => DoctorCheck {
            name: "firmware-hello",
            status: "warn",
            message: error,
            details: json!({"port": port}),
        },
    }
}

fn resolve_port(options: &DeviceOnlyOptions) -> Result<String, String> {
    match &options.port {
        Some(port) => Ok(port.clone()),
        None => detect_port(),
    }
}

fn parse_key_value_block(response: &str, name: &str) -> Value {
    let begin = format!("BEGIN {name}");
    let end = format!("END {name}");
    let mut in_block = false;
    let mut values = serde_json::Map::new();
    for line in response.lines() {
        if line == begin {
            in_block = true;
            continue;
        }
        if line == end {
            break;
        }
        if !in_block {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .parse::<u64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::from(value));
        values.insert(key.to_string(), value);
    }
    Value::Object(values)
}

fn write_json_success(command: &str, data: Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "command": command,
            "data": data,
            "warnings": [],
            "errors": []
        }))
        .unwrap()
    );
}

fn write_json_error(command: &str, error: String) {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": false,
            "command": command,
            "data": null,
            "warnings": [],
            "errors": [{"code": "SQUIDC_ERROR", "message": error}]
        }))
        .unwrap()
    );
}

struct ReplSession {
    target: String,
    port: String,
    profile: BuildProfile,
    mode: ReplMode,
    base_source: String,
    state_block: String,
    snippet: String,
    last_state: String,
    last_output: String,
    last_drawlog: String,
    temp_dir: PathBuf,
    echo: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReplMode {
    Event,
    Render,
}

impl ReplSession {
    fn new(target: String, port: String, base_source: String, echo: bool) -> Self {
        let state_block =
            extract_state_block(&base_source).unwrap_or_else(|| "state {}\n".to_string());
        Self {
            target,
            port,
            profile: BuildProfile::Dev,
            mode: ReplMode::Event,
            base_source,
            state_block,
            snippet: String::new(),
            last_state: String::new(),
            last_output: String::new(),
            last_drawlog: String::new(),
            temp_dir: PathBuf::from("target/repl"),
            echo,
        }
    }

    fn run_script(&mut self, script: &str) -> Result<(), String> {
        let mut state_capture = false;
        for raw_line in script.lines() {
            let line = raw_line.trim_end();
            if state_capture {
                self.state_block.push_str(line);
                self.state_block.push('\n');
                if line.trim() == "}" {
                    state_capture = false;
                }
                continue;
            }
            if line.trim_start().starts_with("state ") || line.trim_start() == "state{" {
                self.flush_snippet()?;
                self.state_block.clear();
                self.state_block.push_str(line);
                self.state_block.push('\n');
                if !line.contains('}') {
                    state_capture = true;
                }
                continue;
            }
            if line.trim_start().starts_with(':') {
                self.handle_command(line.trim_start())?;
            } else if !line.trim().is_empty() {
                self.snippet.push_str(line);
                self.snippet.push('\n');
            }
        }
        self.flush_snippet()
    }

    fn handle_command(&mut self, command: &str) -> Result<(), String> {
        let mut parts = command.split_whitespace();
        match parts.next().unwrap_or("") {
            ":mode" => {
                self.flush_snippet()?;
                self.mode = match parts.next() {
                    Some("event") => ReplMode::Event,
                    Some("render") => ReplMode::Render,
                    other => return Err(format!("unknown mode {other:?}")),
                };
                Ok(())
            }
            ":profile" => {
                self.flush_snippet()?;
                let value = parts.next().ok_or_else(|| "missing profile".to_string())?;
                self.profile = BuildProfile::parse(value)
                    .ok_or_else(|| format!("unknown profile {value}; expected dev or release"))?;
                Ok(())
            }
            ":state" => {
                self.flush_snippet()?;
                self.last_state = self.serial_text(&["state"])?;
                Ok(())
            }
            ":output" => {
                self.flush_snippet()?;
                self.last_output = self.serial_text(&["output"])?;
                Ok(())
            }
            ":drawlog" => {
                self.flush_snippet()?;
                self.last_drawlog = self.serial_text(&["drawlog"])?;
                Ok(())
            }
            ":key" => {
                self.flush_snippet()?;
                let key = parts.next().ok_or_else(|| "missing key".to_string())?;
                self.serial_text(&["key", key])?;
                Ok(())
            }
            ":reset" => {
                self.flush_snippet()?;
                self.serial_text(&["raw", "RESET"])?;
                Ok(())
            }
            ":reload" => {
                self.flush_snippet()?;
                self.reload_base_source()
            }
            ":expect-state" => self.expect_contains("state", command, &self.last_state),
            ":expect-output" => self.expect_contains("output", command, &self.last_output),
            ":expect-draw" => self.expect_contains("drawlog", command, &self.last_drawlog),
            ":quit" => Ok(()),
            other => Err(format!("unknown repl command {other}")),
        }
    }

    fn expect_contains(&self, label: &str, command: &str, haystack: &str) -> Result<(), String> {
        let expected = command
            .split_once(' ')
            .map(|(_, value)| value.trim().trim_matches('"'))
            .ok_or_else(|| format!("missing expectation for {label}"))?;
        if haystack.contains(expected) {
            Ok(())
        } else {
            Err(format!(
                "expected {label} to contain {expected:?}, got {haystack:?}"
            ))
        }
    }

    fn flush_snippet(&mut self) -> Result<(), String> {
        if self.snippet.trim().is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.temp_dir)
            .map_err(|error| format!("failed to create {}: {error}", self.temp_dir.display()))?;
        let state_before = self.serial_text_allow_fail(&["state"]).unwrap_or_default();
        let source = self.generated_source_with_state(&state_before);
        let sqbc = compile_source_to_sqbc(&source, &self.target, self.profile)?;
        let sqbc_path = self.temp_dir.join("repl.sqbc");
        fs::write(&sqbc_path, sqbc)
            .map_err(|error| format!("failed to write {}: {error}", sqbc_path.display()))?;

        let state_path = self.temp_dir.join("state.txt");
        fs::write(&state_path, state_payload(&state_before))
            .map_err(|error| format!("failed to write {}: {error}", state_path.display()))?;

        self.serial_text(&["install-app", "repl-session", path_str(&sqbc_path)?])?;
        self.serial_text(&["run-app-event", "repl-session", "app.start"])?;
        if fs::metadata(&state_path).map(|m| m.len()).unwrap_or(0) > 0 {
            self.serial_text(&["state-import", path_str(&state_path)?])?;
        }

        match self.mode {
            ReplMode::Event => {
                self.serial_text(&["run-app-event", "repl-session", "repl"])?;
                self.last_output = self.serial_text(&["output"])?;
            }
            ReplMode::Render => {
                self.serial_text(&["run-app-event", "repl-session", "app.start"])?;
                self.last_drawlog = self.serial_text(&["drawlog"])?;
            }
        }
        self.last_state = self.serial_text(&["state"])?;
        self.snippet.clear();
        Ok(())
    }

    fn reload_base_source(&mut self) -> Result<(), String> {
        if self.base_source.trim().is_empty() {
            return Err(":reload requires an input .squid file".to_string());
        }
        fs::create_dir_all(&self.temp_dir)
            .map_err(|error| format!("failed to create {}: {error}", self.temp_dir.display()))?;
        let sqbc = compile_source_to_sqbc(&self.base_source, &self.target, self.profile)?;
        let sqbc_path = self.temp_dir.join("repl-base.sqbc");
        fs::write(&sqbc_path, sqbc)
            .map_err(|error| format!("failed to write {}: {error}", sqbc_path.display()))?;

        self.serial_text(&["install-app", "repl-session", path_str(&sqbc_path)?])?;
        self.serial_text(&["run-app-event", "repl-session", "app.start"])?;
        self.last_output = self.serial_text(&["output"])?;
        self.last_drawlog = self.serial_text(&["drawlog"])?;
        self.last_state = self.serial_text(&["state"])?;
        Ok(())
    }

    fn generated_source_with_state(&self, state_output: &str) -> String {
        let mut source = format!(
            "app \"repl-session\"\n\n{}",
            state_block_with_values(&self.state_block, state_output)
        );
        match self.mode {
            ReplMode::Event => {
                source.push_str("\nevent.on(\"app.start\") {}\n\nevent.on(\"repl\") {\n");
                source.push_str(&self.snippet);
                source.push_str("}\n\nscreen(\"main\") {}\n");
            }
            ReplMode::Render => {
                source.push_str(
                    "\nevent.on(\"app.start\") {\n  screen.open(\"__repl\")\n}\n\nscreen(\"__repl\") {\n",
                );
                source.push_str(&self.snippet);
                source.push_str("}\n");
            }
        }
        source
    }

    fn serial_text(&self, args: &[&str]) -> Result<String, String> {
        let mut device = SerialDevice::open(&self.port)?;
        let output = match args {
            ["install-app", app_id, path] => {
                let bytes =
                    fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
                device.install_app(app_id, &bytes)?
            }
            ["run-app-event", app_id, event] => device.run_app_event(app_id, event)?,
            ["state-import", path] => {
                let bytes =
                    fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
                device.import_state(&bytes)?
            }
            ["state"] => device.send_line("STATE.GET")?,
            ["output"] => device.send_line("OUTPUT.GET")?,
            ["drawlog"] => device.send_line("DRAWLOG.GET")?,
            ["resources"] => device.send_line("RESOURCES.GET")?,
            ["key", key] => device.send_line(&format!("KEY {key}"))?,
            ["raw", line] => device.send_line(line)?,
            _ => return Err(format!("unsupported repl serial command: {args:?}")),
        };
        if self.echo {
            print!("{output}");
        }
        Ok(output)
    }

    fn serial_text_allow_fail(&self, args: &[&str]) -> Result<String, String> {
        self.serial_text(args).or_else(|_| Ok(String::new()))
    }
}

fn extract_state_block(source: &str) -> Option<String> {
    let mut out = String::new();
    let mut capture = false;
    for line in source.lines() {
        if line.trim_start().starts_with("state ") {
            capture = true;
        }
        if capture {
            out.push_str(line);
            out.push('\n');
            if line.trim() == "}" {
                return Some(out);
            }
        }
    }
    None
}

fn state_payload(state_output: &str) -> Vec<u8> {
    let mut out = String::new();
    for line in state_output.lines() {
        if line.starts_with("BEGIN ")
            || line.starts_with("END ")
            || line.starts_with("OK ")
            || line.starts_with("exited=")
        {
            continue;
        }
        if line.contains('=') {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.into_bytes()
}

fn state_block_with_values(state_block: &str, state_output: &str) -> String {
    let payload = String::from_utf8_lossy(&state_payload(state_output)).to_string();
    let mut values = Vec::new();
    for line in payload.lines() {
        if let Some((name, value)) = line.split_once('=') {
            values.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    if values.is_empty() {
        return state_block.to_string();
    }

    let mut out = String::new();
    for line in state_block.lines() {
        let trimmed = line.trim_start();
        if let Some((name, rest)) = trimmed.split_once(':') {
            if let Some((_, value)) = values.iter().find(|(existing, _)| existing == name.trim()) {
                let indent_len = line.len().saturating_sub(trimmed.len());
                out.push_str(&line[..indent_len]);
                out.push_str(name.trim());
                if let Some((type_part, _)) = rest.split_once('=') {
                    out.push(':');
                    out.push_str(type_part);
                    out.push_str("= ");
                } else {
                    out.push_str(": ");
                }
                out.push_str(value);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grouped_app_install_command() {
        let cli = Cli::try_parse_from([
            "squidc",
            "app",
            "install",
            "--as",
            "main",
            "examples/blinky-supermini/main.squid",
        ])
        .unwrap();
        let Commands::App {
            command: AppCommands::Install(args),
        } = cli.command
        else {
            panic!("expected app install");
        };
        assert_eq!(args.app_id_override.as_deref(), Some("main"));
        assert_eq!(
            args.input,
            PathBuf::from("examples/blinky-supermini/main.squid")
        );
    }

    #[test]
    fn parses_package_command_with_default_output() {
        let cli = Cli::try_parse_from(["squidc", "package", "examples/binbook-reader"]).unwrap();
        let Commands::Package(args) = cli.command else {
            panic!("expected package");
        };
        assert_eq!(args.input, PathBuf::from("examples/binbook-reader"));
        assert_eq!(args.out, None);
    }

    #[test]
    fn package_app_dir_writes_sqbc_and_runtime_files_without_source_or_dotfiles() {
        let root = unique_test_dir("squidc-package");
        let app_dir = root.join("app");
        fs::create_dir_all(app_dir.join("static")).unwrap();
        fs::create_dir_all(app_dir.join("lib")).unwrap();
        fs::create_dir_all(app_dir.join(".git")).unwrap();
        fs::write(
            app_dir.join("main.squid"),
            r#"app "package-demo"
include "lib/ui.squid"
state {}
event.on("app.start") {}
screen("main") {}
"#,
        )
        .unwrap();
        fs::write(
            app_dir.join("lib").join("ui.squid"),
            "function helper() {}\n",
        )
        .unwrap();
        fs::write(app_dir.join("static").join("index.html"), "<h1>Demo</h1>").unwrap();
        fs::write(app_dir.join(".env"), "SECRET=1").unwrap();
        fs::write(app_dir.join(".git").join("HEAD"), "ref: main").unwrap();
        fs::write(app_dir.join("old.squid.zip"), "old").unwrap();
        let out = root.join("demo.squid.zip");

        let result = package_app_dir(&app_dir, Some(&out), "portable", BuildProfile::Dev).unwrap();
        let entries = package::read_stored_zip_entries(&fs::read(&out).unwrap())
            .unwrap()
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();

        assert_eq!(result.app_id, "package-demo");
        assert_eq!(entries, vec!["main.sqbc", "static/index.html"]);
        assert!(result.entries.contains(&"main.sqbc".to_string()));
        assert!(result.entries.contains(&"static/index.html".to_string()));
        fs::remove_dir_all(root).unwrap();
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    #[test]
    fn parses_device_monitor_json_shape_requirement_inputs() {
        let cli =
            Cli::try_parse_from(["squidc", "--json", "device", "monitor", "--max-lines", "4"])
                .unwrap();
        assert!(cli.json);
        let Commands::Device {
            command: DeviceCommands::Monitor(args),
        } = cli.command
        else {
            panic!("expected device monitor");
        };
        assert_eq!(args.max_lines, Some(4));
    }

    #[test]
    fn parses_protocol_raw_command() {
        let cli = Cli::try_parse_from(["squidc", "protocol", "raw", "APP.LIST"]).unwrap();
        let Commands::Protocol {
            command: ProtocolCommands::Raw(args),
        } = cli.command
        else {
            panic!("expected protocol raw");
        };
        assert_eq!(args.line, "APP.LIST");
    }

    #[test]
    fn parses_device_resources_command_and_resource_block() {
        let cli = Cli::try_parse_from(["squidc", "device", "resources"]).unwrap();
        let Commands::Device {
            command: DeviceCommands::Resources(_),
        } = cli.command
        else {
            panic!("expected device resources");
        };
        let parsed = parse_key_value_block(
            "BEGIN RESOURCES\nmemory_available_bytes=299136\napp_storage_available_bytes=2031616\nEND RESOURCES\nOK RESOURCES.GET\n",
            "RESOURCES",
        );
        assert_eq!(parsed["memory_available_bytes"], 299136u64);
        assert_eq!(parsed["app_storage_available_bytes"], 2031616u64);
    }

    #[test]
    fn repl_state_block_uses_previous_state_values() {
        let state_block = "state {\n  count: int = 0\n  label: string = \"old\"\n}\n";
        let state_output = "BEGIN STATE\ncount=2\nlabel=\"new\"\nexited=false\nEND STATE\n";

        let merged = state_block_with_values(state_block, state_output);

        assert!(merged.contains("count: int = 2"));
        assert!(merged.contains("label: string = \"new\""));
        assert!(!merged.contains("exited"));
    }

    #[test]
    fn json_monitor_requires_max_lines() {
        let cli = Cli::try_parse_from(["squidc", "--json", "device", "monitor"]).unwrap();
        let Commands::Device {
            command: DeviceCommands::Monitor(args),
        } = cli.command
        else {
            panic!("expected device monitor");
        };
        assert!(monitor(args, true).unwrap_err().contains("--max-lines"));
    }
}
