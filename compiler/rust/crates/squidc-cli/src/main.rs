mod app_id;
mod ble_push;
mod compile;
mod firmware_image;
mod http_upload;
mod package;
mod serial;
mod target;

use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use app_id::{fnv1a, generated_app_id, source_app_id, source_for_compile};
use clap::{error::ErrorKind, Args, Parser, Subcommand, ValueEnum};
use compile::{compile_path_to_sqbc, compile_source_to_sqbc, compile_target_id};
use package::{package_app_dir, read_stored_zip_entries};
use serde::Serialize;
use serde_json::{json, Value};
use serial::{
    candidate_ports, content_install_progress_line, detect_port, format_lines, format_raw_lines,
    format_state_bytes, OutputTail, SerialDevice,
};
use squid_device_protocol as protocol;
use squidc_core::{
    compile::{compile_path_with_profile, CompileResponse},
    formatter::format_source,
    profile::BuildProfile,
};

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
    Fmt(FmtArgs),
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
    Hardware {
        #[command(subcommand)]
        command: HardwareCommands,
    },
    Target {
        #[command(subcommand)]
        command: TargetCommands,
    },
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Self::Fmt(_) => "fmt",
            Self::Repl(_) => "repl",
            Self::Doctor(_) => "doctor",
            Self::App { .. } => "app",
            Self::Device { .. } => "device",
            Self::Protocol { .. } => "protocol",
            Self::Hardware { .. } => "hardware",
            Self::Target { .. } => "target",
        }
    }
}

#[derive(Args, Debug)]
struct FmtArgs {
    #[arg(long, help = "Check formatting without rewriting files")]
    check: bool,
    #[arg(
        long,
        help = "Read SquidScript source from stdin and write formatted source to stdout"
    )]
    stdin: bool,
    paths: Vec<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum AppCommands {
    Build(BuildArgs),
    Package(PackageArgs),
    Run(DeviceSourceArgs),
    Test(AppTestArgs),
    Install(AppInstallArgs),
    Push(AppPushArgs),
    Launch(AppLaunchArgs),
    List(DeviceOnlyArgs),
}

#[derive(Subcommand, Debug)]
enum DeviceCommands {
    Key(DeviceKeyArgs),
    DisplayWindowProbe(DeviceDisplayWindowProbeArgs),
    ContentPut(DeviceContentPutArgs),
    ContentCheck(DeviceContentCheckArgs),
    ContentDelete(DeviceContentDeleteArgs),
    FirmwareInfo(DeviceOnlyArgs),
    FirmwareUpdate(DeviceFirmwareUpdateArgs),
    Upload(DeviceUploadArgs),
    WifiProfile(DeviceWifiProfileArgs),
    RuntimeCap(DeviceRuntimeCapArgs),
    Reset(DeviceOnlyArgs),
    Output(DeviceOnlyArgs),
    State(DeviceOnlyArgs),
    Drawlog(DeviceOnlyArgs),
    DebugLog(DeviceOnlyArgs),
    Trace(DeviceOnlyArgs),
    Lifecycle(DeviceOnlyArgs),
    Errors(DeviceOnlyArgs),
    Resources(DeviceResourcesArgs),
    StorageFormat(DeviceOnlyArgs),
    Monitor(MonitorArgs),
}

#[derive(Subcommand, Debug)]
enum ProtocolCommands {
    Raw(ProtocolRawArgs),
}

#[derive(Subcommand, Debug)]
enum HardwareCommands {
    Test(HardwareTestArgs),
}

#[derive(Subcommand, Debug)]
enum TargetCommands {
    List(TargetListArgs),
    Inspect(TargetOnlyArgs),
    Build(TargetBuildArgs),
    Flash(TargetFlashArgs),
    Monitor(TargetMonitorArgs),
    Doctor(TargetDoctorArgs),
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
struct AppTestArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    check_target: bool,
    #[arg(long)]
    negative: bool,
    #[arg(long)]
    list: bool,
    input: PathBuf,
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
struct AppPushArgs {
    device: String,
    input: PathBuf,
}

#[derive(Args, Debug)]
struct DeviceUploadArgs {
    input: PathBuf,
    #[arg(long)]
    name: String,
    #[arg(long, value_enum)]
    transport: UploadTransportArg,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    device: Option<String>,
    #[arg(long)]
    port: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum UploadTransportArg {
    Http,
    Ble,
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
struct DeviceDisplayWindowProbeArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    pattern: String,
}

#[derive(Args, Debug)]
struct DeviceContentPutArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    input: PathBuf,
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args, Debug)]
struct DeviceContentCheckArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    name: String,
    #[arg(long)]
    size: u64,
    #[arg(long)]
    crc32: String,
}

#[derive(Args, Debug)]
struct DeviceContentDeleteArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    name: String,
}

#[derive(Args, Debug)]
struct DeviceWifiProfileArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    profile: String,
    #[arg(long)]
    ssid_env: String,
    #[arg(long)]
    password_env: String,
}

#[derive(Args, Debug)]
struct DeviceRuntimeCapArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    #[command(subcommand)]
    command: RuntimeCapCommands,
}

#[derive(Subcommand, Debug)]
enum RuntimeCapCommands {
    Get { key: Option<String> },
    Set { key: String, value: u16 },
    Clear { key: Option<String> },
}

#[derive(Args, Debug)]
struct DeviceOnlyArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
}

#[derive(Args, Debug)]
struct DeviceFirmwareUpdateArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    image: PathBuf,
}

#[derive(Args, Debug)]
struct DeviceResourcesArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    #[arg(long)]
    reset_heap_max: bool,
    #[arg(long, default_value_t = 1)]
    count: u32,
    #[arg(long, default_value_t = 0)]
    interval_ms: u64,
}

#[derive(Args, Debug)]
struct ProtocolRawArgs {
    #[command(flatten)]
    device: DeviceOnlyOptions,
    opcode: String,
    #[arg(long, default_value_t = 1)]
    seq: u32,
    #[arg(long = "string")]
    string: Vec<String>,
    #[arg(long = "bytes")]
    bytes: Vec<String>,
    #[arg(long = "bool")]
    r#bool: Vec<String>,
    #[arg(long = "u64")]
    u64: Vec<String>,
    #[arg(long = "u32")]
    u32: Vec<String>,
    #[arg(long = "i64")]
    i64: Vec<String>,
}

#[derive(Args, Debug)]
struct HardwareTestArgs {
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    skip_flash: bool,
    #[arg(long)]
    port: Option<String>,
    #[arg(long)]
    ble_device: Option<String>,
    #[arg(long)]
    host_wifi_iface: Option<String>,
    #[arg(long)]
    list: bool,
}

#[derive(Args, Debug)]
struct TargetListArgs {}

#[derive(Args, Debug)]
struct TargetOnlyArgs {
    #[arg(long)]
    target: Option<String>,
}

#[derive(Args, Debug)]
struct TargetBuildArgs {
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    stack_usage: bool,
    #[arg(long, value_enum, default_value_t = TargetPristineArg::Auto)]
    pristine: TargetPristineArg,
    #[arg(long)]
    print_plan: bool,
    #[arg(last = true)]
    west_args: Vec<String>,
}

#[derive(Args, Debug)]
struct TargetFlashArgs {
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    monitor_after_flash: bool,
    #[arg(long)]
    print_plan: bool,
    #[arg(last = true)]
    west_args: Vec<String>,
}

#[derive(Args, Debug)]
struct TargetMonitorArgs {
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    port: Option<String>,
    #[arg(long)]
    print_plan: bool,
    #[arg(last = true)]
    west_args: Vec<String>,
}

#[derive(Args, Debug)]
struct TargetDoctorArgs {
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    port: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum TargetPristineArg {
    Auto,
    Always,
    Never,
}

impl From<TargetPristineArg> for target::TargetPristine {
    fn from(value: TargetPristineArg) -> Self {
        match value {
            TargetPristineArg::Auto => target::TargetPristine::Auto,
            TargetPristineArg::Always => target::TargetPristine::Always,
            TargetPristineArg::Never => target::TargetPristine::Never,
        }
    }
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
        Commands::Fmt(args) => fmt_command_with_output(args, human),
        Commands::Repl(args) => repl(args, human),
        Commands::Doctor(args) => doctor(args, human),
        Commands::App { command } => match command {
            AppCommands::Build(args) => build(args),
            AppCommands::Package(args) => package_app(args),
            AppCommands::Run(args) => run_app_source(args, human),
            AppCommands::Test(args) => app_test(args, human),
            AppCommands::Install(args) => install_app(args, human),
            AppCommands::Push(args) => push_app_ble(args, human),
            AppCommands::Launch(args) => launch_app(args, human),
            AppCommands::List(args) => app_list(args, human),
        },
        Commands::Device { command } => match command {
            DeviceCommands::Key(args) => key(args, human),
            DeviceCommands::DisplayWindowProbe(args) => display_window_probe(args, human),
            DeviceCommands::ContentPut(args) => content_put(args, human),
            DeviceCommands::ContentCheck(args) => content_check(args, human),
            DeviceCommands::ContentDelete(args) => content_delete(args, human),
            DeviceCommands::FirmwareInfo(args) => firmware_info_command(args, human),
            DeviceCommands::FirmwareUpdate(args) => firmware_update_command(args, human),
            DeviceCommands::Upload(args) => device_upload(args, human),
            DeviceCommands::WifiProfile(args) => wifi_profile(args, human),
            DeviceCommands::RuntimeCap(args) => runtime_cap(args, human),
            DeviceCommands::Reset(args) => reset(args.device, human),
            DeviceCommands::Output(args) => device_output(args.device, human),
            DeviceCommands::State(args) => state(args.device, human),
            DeviceCommands::Drawlog(args) => drawlog(args.device, human),
            DeviceCommands::DebugLog(args) => debug_log(args.device, human),
            DeviceCommands::Trace(args) => trace(args.device, human),
            DeviceCommands::Lifecycle(args) => lifecycle(args.device, human),
            DeviceCommands::Errors(args) => errors(args.device, human),
            DeviceCommands::Resources(args) => resources(args, human),
            DeviceCommands::StorageFormat(args) => storage_format(args.device, human),
            DeviceCommands::Monitor(args) => monitor(args, json_mode),
        },
        Commands::Protocol { command } => match command {
            ProtocolCommands::Raw(args) => protocol_raw(args, human),
        },
        Commands::Hardware { command } => match command {
            HardwareCommands::Test(args) => hardware_test(args, human),
        },
        Commands::Target { command } => match command {
            TargetCommands::List(args) => target_list(args, human),
            TargetCommands::Inspect(args) => target_inspect(args, human),
            TargetCommands::Build(args) => target_build(args, human),
            TargetCommands::Flash(args) => target_flash(args, human),
            TargetCommands::Monitor(args) => target_monitor(args, human),
            TargetCommands::Doctor(args) => target_doctor(args, human),
        },
    }
}

fn fmt_command_with_output(args: FmtArgs, human: bool) -> Result<Value, String> {
    let result = fmt_command(args)?;
    if human {
        if let Some(formatted) = result.get("formattedStdin").and_then(Value::as_str) {
            print!("{formatted}");
        } else {
            println!(
                "formatted checked={} changed={}",
                result["checked"], result["changed"]
            );
        }
    }
    Ok(result)
}

fn fmt_command(args: FmtArgs) -> Result<Value, String> {
    if args.stdin {
        if !args.paths.is_empty() {
            return Err("--stdin cannot be combined with file paths".to_string());
        }
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| format!("failed to read stdin: {error}"))?;
        let formatted = format_source(&source).map_err(|error| error.message)?;
        if args.check && formatted != source {
            return Err("stdin would reformat".to_string());
        }
        return Ok(json!({
            "checked": 1,
            "changed": usize::from(formatted != source),
            "formattedStdin": formatted
        }));
    }

    if args.paths.is_empty() {
        return Err("fmt requires at least one path or --stdin".to_string());
    }

    let files = collect_squid_files(&args.paths)?;
    let mut changed = Vec::new();
    for file in &files {
        let source = fs::read_to_string(file)
            .map_err(|error| format!("failed to read {}: {error}", file.display()))?;
        let formatted = format_source(&source)
            .map_err(|error| format!("failed to format {}: {}", file.display(), error.message))?;
        if formatted != source {
            changed.push(file.clone());
            if !args.check {
                fs::write(file, formatted)
                    .map_err(|error| format!("failed to write {}: {error}", file.display()))?;
            }
        }
    }

    if args.check && !changed.is_empty() {
        let files = changed
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("would reformat {files}"));
    }

    Ok(json!({
        "checked": files.len(),
        "changed": changed.len(),
        "files": files
    }))
}

fn collect_squid_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for path in paths {
        collect_squid_files_from_path(path, &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_squid_files_from_path(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.is_dir() {
        let entries = fs::read_dir(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("failed to read {} entry: {error}", path.display()))?;
            collect_squid_files_from_path(&entry.path(), files)?;
        }
    } else if path.extension().and_then(|value| value.to_str()) == Some("squid") {
        files.push(path.to_path_buf());
    }
    Ok(())
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
    let bytes = compile_path_to_sqbc(&args.input, &target, args.profile.into())?;
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
    let target = compile_target_id(args.device.target.as_deref(), args.device.check_target)?;
    let sqbc = if source_app_id(&source).is_some() {
        compile_path_to_sqbc(&args.input, &target, args.device.profile.into())?
    } else {
        let source = source_for_compile(&source, &app_id);
        compile_source_to_sqbc(&source, &target, args.device.profile.into())?
    };
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
    let app_id = squidc_core::sqbc::read_app_id(&main.bytes)
        .map_err(|error| error.message)?
        .ok_or_else(|| "SQBC has no app id metadata".to_string())?;

    let port = resolve_port(&args.device.device)?;
    let mut device = SerialDevice::open(&port)
        .map_err(|error| format!("open device for app install: {error}"))?;
    let mut response = String::new();
    for entry in &entries {
        response.push_str(
            &device
                .install_resource(&app_id, &entry.path, &entry.bytes)
                .map_err(|error| format!("install resource {}: {error}", entry.path))?,
        );
    }
    response.push_str(&device.install_app(&app_id, &main.bytes)?);
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

fn push_app_ble(args: AppPushArgs, human: bool) -> Result<Value, String> {
    let result = ble_push::push_sqbc(&args.device, &args.input)?;
    if human {
        println!(
            "BLE uploaded ext={} bytes={}",
            result.extension, result.bytes_sent
        );
    }
    Ok(json!({
        "device": args.device,
        "input": args.input,
        "extension": result.extension,
        "bytes": result.bytes_sent
    }))
}

fn device_upload(args: DeviceUploadArgs, human: bool) -> Result<Value, String> {
    if !is_safe_content_name(&args.name) {
        return Err(format!(
            "upload name must be a simple filename: {}",
            args.name
        ));
    }
    let started = Instant::now();
    let (transport, destination, bytes, resumed_bytes) = match args.transport {
        UploadTransportArg::Http => {
            let host = args
                .host
                .as_deref()
                .ok_or_else(|| "--host is required for --transport http".to_string())?;
            if args.device.is_some() {
                return Err("--device is only valid with --transport ble".to_string());
            }
            let port = args.port.unwrap_or(80);
            let result = http_upload::upload(host, port, &args.input, &args.name)?;
            (
                "http",
                format!("{host}:{port}"),
                result.bytes_sent,
                result.resumed_bytes,
            )
        }
        UploadTransportArg::Ble => {
            let device = args
                .device
                .as_deref()
                .ok_or_else(|| "--device is required for --transport ble".to_string())?;
            if args.host.is_some() {
                return Err("--host is only valid with --transport http".to_string());
            }
            if args.port.is_some() {
                return Err("--port is only valid with --transport http".to_string());
            }
            let result = ble_push::push_file(device, &args.input, &args.name)?;
            ("ble", device.to_string(), result.bytes_sent, 0)
        }
    };
    let elapsed = started.elapsed();
    let elapsed_seconds = elapsed.as_secs_f64();
    let bytes_per_second = if elapsed_seconds > 0.0 {
        bytes as f64 / elapsed_seconds
    } else {
        bytes as f64
    };
    if human {
        println!(
            "uploaded transport={transport} name={} bytes={bytes} elapsed={elapsed_seconds:.3}s bytes_per_second={bytes_per_second:.0}",
            args.name
        );
    }
    Ok(json!({
        "transport": transport,
        "destination": destination,
        "input": args.input,
        "name": args.name,
        "bytes": bytes,
        "resumedBytes": resumed_bytes,
        "elapsedSeconds": elapsed_seconds,
        "bytesPerSecond": bytes_per_second,
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
            None => squidc_core::sqbc::read_app_id(&bytes)
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
    let target = compile_target_id(options.target.as_deref(), options.check_target)?;
    let bytes = if source_app_id(&source).is_some() {
        compile_path_to_sqbc(input, &target, options.profile.into())?
    } else {
        let source = source_for_compile(&source, &app_id);
        compile_source_to_sqbc(&source, &target, options.profile.into())?
    };
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

fn app_list(args: DeviceOnlyArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let apps = device.app_list()?;
    if human {
        for app in &apps {
            println!("app={} sqbc_len={}", app.app_id, app.sqbc_len);
        }
    }
    Ok(json!({
        "port": port,
        "apps": apps.iter().map(|app| {
            json!({
                "appId": app.app_id,
                "sqbcLen": app.sqbc_len,
            })
        }).collect::<Vec<_>>()
    }))
}

fn content_put(args: DeviceContentPutArgs, human: bool) -> Result<Value, String> {
    let name = match args.name {
        Some(name) => name,
        None => args
            .input
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("input path has no UTF-8 filename: {}", args.input.display()))?
            .to_string(),
    };
    if !is_safe_content_name(&name) {
        return Err(format!("content name must be a simple filename: {name}"));
    }
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = if human {
        let mut last_progress = Instant::now() - Duration::from_secs(1);
        device.install_content_with_progress(&name, &args.input, |received, total, elapsed| {
            let now = Instant::now();
            if received == total || now.duration_since(last_progress) >= Duration::from_secs(1) {
                eprintln!(
                    "{}",
                    content_install_progress_line(&name, received, total, elapsed)
                );
                last_progress = now;
            }
        })?
    } else {
        device.install_content(&name, &args.input)?
    };
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "name": name,
        "input": args.input,
        "response": response
    }))
}

fn content_check(args: DeviceContentCheckArgs, human: bool) -> Result<Value, String> {
    if !is_safe_content_name(&args.name) {
        return Err(format!(
            "content name must be a simple filename: {}",
            args.name
        ));
    }
    let expected_crc32 = parse_crc32_arg(&args.crc32)?;
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let result = device.content_check(&args.name)?;
    if result.size != args.size {
        return Err(format!(
            "content {} size mismatch: expected {} got {}",
            args.name, args.size, result.size
        ));
    }
    if result.crc32 != expected_crc32 {
        return Err(format!(
            "content {} crc32 mismatch: expected {expected_crc32:08x} got {:08x}",
            args.name, result.crc32
        ));
    }
    if human {
        println!(
            "content-check {} size={} crc32={:08x}",
            result.name, result.size, result.crc32
        );
    }
    Ok(json!({
        "port": port,
        "name": result.name,
        "size": result.size,
        "crc32": format!("{:08x}", result.crc32),
    }))
}

fn content_delete(args: DeviceContentDeleteArgs, human: bool) -> Result<Value, String> {
    if !is_safe_content_name(&args.name) {
        return Err(format!(
            "content name must be a simple filename: {}",
            args.name
        ));
    }
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let name = device.content_delete(&args.name)?;
    if human {
        println!("content-deleted {name}");
    }
    Ok(json!({
        "port": port,
        "name": name,
    }))
}

fn firmware_info_command(args: DeviceOnlyArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let info = device.firmware_info()?;
    if human {
        println!(
            "firmware active={} size={} inactive={} size={} build={} boot={}",
            info.active_slot,
            info.active_slot_size,
            info.inactive_slot,
            info.inactive_slot_size,
            info.build_id,
            info.boot_state
        );
    }
    Ok(json!({
        "port": port,
        "activeSlot": info.active_slot,
        "activeSlotSize": info.active_slot_size,
        "inactiveSlot": info.inactive_slot,
        "inactiveSlotSize": info.inactive_slot_size,
        "buildId": info.build_id,
        "bootState": info.boot_state,
    }))
}

fn firmware_update_command(args: DeviceFirmwareUpdateArgs, human: bool) -> Result<Value, String> {
    let bytes = fs::read(&args.image)
        .map_err(|error| format!("failed to read {}: {error}", args.image.display()))?;
    let image = firmware_image::validate(&bytes)
        .map_err(|error| format!("invalid firmware image {}: {error}", args.image.display()))?;
    let build_id = image.build_id();
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let info = device.firmware_info()?;
    if image.image_len as u64 > info.inactive_slot_size {
        return Err(format!(
            "firmware image is {} bytes but inactive slot {} holds {} bytes",
            image.image_len, info.inactive_slot, info.inactive_slot_size
        ));
    }

    let mut status = device.firmware_update_status()?;
    let matches = status.expected_len == image.image_len as u64
        && status.expected_sha256.as_slice() == image.sha256
        && status.build_id == build_id
        && status.candidate_slot == info.inactive_slot;
    if status.state != "idle" && !matches {
        status = device.firmware_update_abort()?;
    }
    if status.state == "idle" {
        status = device.firmware_update_begin(image.image_len, &image.sha256, &build_id)?;
    }
    if status.expected_len != image.image_len as u64
        || status.expected_sha256.as_slice() != image.sha256
        || status.build_id != build_id
        || status.candidate_slot != info.inactive_slot
    {
        return Err("device did not retain the requested firmware update identity".to_string());
    }

    let mut offset = usize::try_from(status.durable_offset)
        .map_err(|_| "device durable firmware offset does not fit this host".to_string())?;
    if offset > bytes.len() {
        return Err(format!(
            "device durable firmware offset {offset} exceeds image length {}",
            bytes.len()
        ));
    }
    let resumed_from = offset;
    let chunk_bytes = device.firmware_update_chunk_bytes();
    let started = Instant::now();
    let mut last_progress = started - Duration::from_secs(1);
    while offset < bytes.len() {
        let end = offset.saturating_add(chunk_bytes).min(bytes.len());
        status = device.firmware_update_chunk(offset, &bytes[offset..end])?;
        let durable = usize::try_from(status.durable_offset)
            .map_err(|_| "device durable firmware offset does not fit this host".to_string())?;
        if durable != end {
            return Err(format!(
                "firmware chunk at {offset} ended at {end}, device confirmed {durable}"
            ));
        }
        offset = durable;
        if human && (offset == bytes.len() || last_progress.elapsed() >= Duration::from_secs(1)) {
            eprintln!(
                "{}",
                firmware_update_progress_line(
                    offset - resumed_from,
                    bytes.len() - resumed_from,
                    started.elapsed()
                )
            );
            last_progress = Instant::now();
        }
    }
    device.firmware_update_commit()?;
    if human {
        println!(
            "firmware-update image={} project={} version={} build={} slot={} bytes={} resumed={}",
            args.image.display(),
            image.project_name,
            image.version,
            build_id,
            info.inactive_slot,
            image.image_len,
            resumed_from
        );
    }
    Ok(json!({
        "port": port,
        "image": args.image,
        "project": image.project_name,
        "version": image.version,
        "buildId": build_id,
        "candidateSlot": info.inactive_slot,
        "bytes": image.image_len,
        "resumedFrom": resumed_from,
    }))
}

fn firmware_update_progress_line(received: usize, total: usize, elapsed: Duration) -> String {
    let percent = if total == 0 {
        100.0
    } else {
        received as f64 * 100.0 / total as f64
    };
    let seconds = elapsed.as_secs_f64().max(0.001);
    let rate = received as f64 / seconds;
    let eta = if rate > 0.0 {
        ((total.saturating_sub(received) as f64 / rate).ceil()) as u64
    } else {
        0
    };
    format!(
        "firmware {percent:.1}% {received}/{total} {:.1} KiB/s eta {eta}s",
        rate / 1024.0
    )
}

fn parse_crc32_arg(value: &str) -> Result<u64, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty() || value.len() > 8 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!("invalid crc32 value: {value}"));
    }
    u64::from_str_radix(value, 16).map_err(|error| format!("invalid crc32 value {value}: {error}"))
}

fn is_safe_content_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && name.is_ascii()
        && name.len() <= squid_device_protocol::MAX_CONTENT_NAME_BYTES
}

#[derive(Clone, Debug, Serialize)]
struct AppTestCase {
    name: String,
    source: PathBuf,
    session: PathBuf,
}

#[derive(Clone, Debug)]
struct NegativeAppTest {
    source: PathBuf,
    expected_diagnostic: String,
}

fn app_test(args: AppTestArgs, human: bool) -> Result<Value, String> {
    if args.negative {
        return run_negative_app_tests(&args.input, args.list, human);
    }

    let tests = discover_app_tests(&args.input)?;
    if args.list {
        if human {
            for test in &tests {
                println!("app-test={} source={}", test.name, test.source.display());
            }
        }
        return Ok(json!({"tests": tests}));
    }

    let target = compile_target_id(args.target.as_deref(), args.check_target)?;
    let port = resolve_port(&args.device)?;
    let mut results = Vec::new();
    for (index, test) in tests.into_iter().enumerate() {
        let source = fs::read_to_string(&test.source)
            .map_err(|error| format!("failed to read {}: {error}", test.source.display()))?;
        let script = fs::read_to_string(&test.session)
            .map_err(|error| format!("failed to read {}: {error}", test.session.display()))?;
        ReplSession::new_with_app_id(
            target.clone(),
            port.clone(),
            source,
            human,
            app_test_session_app_id(&test.name, index),
        )
        .run_script(&script)?;
        if human {
            println!("app-test={} ok", test.name);
        }
        results.push(json!({
            "name": test.name,
            "source": test.source,
            "session": test.session,
            "status": "ok"
        }));
    }

    Ok(json!({
        "port": port,
        "target": target,
        "tests": results
    }))
}

fn discover_app_tests(path: &Path) -> Result<Vec<AppTestCase>, String> {
    discover_app_test_paths(path)?
        .into_iter()
        .map(|source| {
            let session = source
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("test.session");
            if !session.is_file() {
                return Err(format!(
                    "app test {} is missing sibling test.session",
                    source.display()
                ));
            }
            let name = test_name_for_source(&source);
            Ok(AppTestCase {
                name,
                source,
                session,
            })
        })
        .collect()
}

fn discover_app_test_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    collect_main_squid_paths(path, &mut paths)?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn collect_main_squid_paths(path: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.is_file() {
        if path.file_name().and_then(|value| value.to_str()) == Some("main.squid") {
            paths.push(path.to_path_buf());
        }
        return Ok(());
    }
    if path.join("main.squid").is_file() {
        paths.push(path.join("main.squid"));
        return Ok(());
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("failed to read {} entry: {error}", path.display()))?;
        let child = entry.path();
        if child.is_dir() && !is_hidden_path(&child) {
            collect_main_squid_paths(&child, paths)?;
        }
    }
    Ok(())
}

fn run_negative_app_tests(input: &Path, list_only: bool, human: bool) -> Result<Value, String> {
    let tests = discover_negative_app_tests(input)?;
    if list_only {
        if human {
            for test in &tests {
                println!(
                    "negative-app-test={} expected={}",
                    test.source.display(),
                    test.expected_diagnostic
                );
            }
        }
        return Ok(json!({
            "negative": true,
            "tests": tests.iter().map(|test| {
                json!({
                    "source": test.source,
                    "expectedDiagnostic": test.expected_diagnostic
                })
            }).collect::<Vec<_>>()
        }));
    }

    let mut results = Vec::new();
    for test in tests {
        let compiled = compile_path_with_profile(&test.source, "portable", BuildProfile::Dev);
        assert_negative_compile_result(&test, compiled)?;
        if human {
            println!(
                "negative-app-test={} expected={} ok",
                test.source.display(),
                test.expected_diagnostic
            );
        }
        results.push(json!({
            "source": test.source,
            "expectedDiagnostic": test.expected_diagnostic,
            "status": "ok"
        }));
    }

    Ok(json!({
        "negative": true,
        "tests": results
    }))
}

fn assert_negative_compile_result(
    test: &NegativeAppTest,
    compiled: CompileResponse,
) -> Result<(), String> {
    if compiled.ok {
        return Err(format!(
            "negative app test {} compiled successfully; expected {}",
            test.source.display(),
            test.expected_diagnostic
        ));
    }
    let expected = test.expected_diagnostic.trim();
    if compiled
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == expected || diagnostic.message.contains(expected))
    {
        return Ok(());
    }
    let diagnostics = compiled
        .diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("; ");
    Err(format!(
        "negative app test {} expected diagnostic {}, got {}",
        test.source.display(),
        expected,
        diagnostics
    ))
}

fn discover_negative_app_tests(path: &Path) -> Result<Vec<NegativeAppTest>, String> {
    let mut tests = Vec::new();
    collect_negative_app_tests(path, &mut tests)?;
    tests.sort_by(|left, right| left.source.cmp(&right.source));
    Ok(tests)
}

fn collect_negative_app_tests(path: &Path, tests: &mut Vec<NegativeAppTest>) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.is_file() {
        return Ok(());
    }
    let source = path.join("main.squid");
    let expected = path.join("expected.txt");
    if source.is_file() && expected.is_file() {
        let expected_diagnostic = fs::read_to_string(&expected)
            .map_err(|error| format!("failed to read {}: {error}", expected.display()))?
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .ok_or_else(|| format!("{} has no expected diagnostic", expected.display()))?
            .to_string();
        tests.push(NegativeAppTest {
            source,
            expected_diagnostic,
        });
        return Ok(());
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("failed to read {} entry: {error}", path.display()))?;
        let child = entry.path();
        if child.is_dir() && !is_hidden_path(&child) {
            collect_negative_app_tests(&child, tests)?;
        }
    }
    Ok(())
}

fn test_name_for_source(source: &Path) -> String {
    source
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("main")
        .to_string()
}

fn is_hidden_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn key(args: DeviceKeyArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_key(&args.key)?;
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "key": args.key,
        "response": response
    }))
}

fn display_window_probe(args: DeviceDisplayWindowProbeArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.display_window_probe(&args.pattern)?;
    if human {
        print!("{response}");
    }
    Ok(json!({
        "port": port,
        "pattern": args.pattern,
        "response": response
    }))
}

fn wifi_profile(args: DeviceWifiProfileArgs, human: bool) -> Result<Value, String> {
    let ssid = env::var(&args.ssid_env)
        .map_err(|_| format!("missing Wi-Fi SSID environment variable {}", args.ssid_env))?;
    let password = env::var(&args.password_env).map_err(|_| {
        format!(
            "missing Wi-Fi password environment variable {}",
            args.password_env
        )
    })?;
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    device.set_wifi_profile(&args.profile, &ssid, &password)?;
    if human {
        println!(
            "wifi-profile profile={} ssid_len={} password_len={}",
            args.profile,
            ssid.len(),
            password.len()
        );
    }
    Ok(json!({
        "port": port,
        "profile": args.profile,
        "ssidLen": ssid.len(),
        "passwordLen": password.len(),
    }))
}

fn runtime_cap(args: DeviceRuntimeCapArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    match args.command {
        RuntimeCapCommands::Get { key } => {
            let lines = device.runtime_cap_get(key.as_deref())?;
            if human {
                for line in &lines {
                    println!("{line}");
                }
            }
            Ok(json!({
                "port": port,
                "action": "get",
                "key": key,
                "lines": lines,
            }))
        }
        RuntimeCapCommands::Set { key, value } => {
            device.runtime_cap_set(&key, value)?;
            if human {
                println!("runtime-cap set {key}={value}");
            }
            Ok(json!({
                "port": port,
                "action": "set",
                "key": key,
                "value": value,
            }))
        }
        RuntimeCapCommands::Clear { key } => {
            device.runtime_cap_clear(key.as_deref())?;
            if human {
                if let Some(key) = &key {
                    println!("runtime-cap cleared {key}");
                } else {
                    println!("runtime-cap cleared");
                }
            }
            Ok(json!({
                "port": port,
                "action": "clear",
                "key": key,
            }))
        }
    }
}

fn protocol_raw(args: ProtocolRawArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut fields = Vec::new();
    for value in &args.string {
        fields.push(protocol::parse_field_arg("string", value)?);
    }
    for value in &args.bytes {
        fields.push(protocol::parse_field_arg("bytes", value)?);
    }
    for value in &args.r#bool {
        fields.push(protocol::parse_field_arg("bool", value)?);
    }
    for value in &args.u64 {
        fields.push(protocol::parse_field_arg("u64", value)?);
    }
    for value in &args.u32 {
        fields.push(protocol::parse_field_arg("u32", value)?);
    }
    for value in &args.i64 {
        fields.push(protocol::parse_field_arg("i64", value)?);
    }

    let opcode = protocol::Opcode::parse(&args.opcode)?;
    let frame = protocol::Frame::request(opcode, args.seq, fields);
    let bytes = protocol::encode_frame(&frame);
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_bytes_until_quiet(&bytes)?;
    if human {
        println!("{}", hex_string(&response));
    }
    Ok(json!({
        "port": port,
        "opcode": args.opcode,
        "sequence": args.seq,
        "requestHex": hex_string(&bytes),
        "responseHex": hex_string(&response)
    }))
}

fn target_list(_args: TargetListArgs, human: bool) -> Result<Value, String> {
    let root = target::repo_root();
    let targets = target::load_targets(&root)?;
    if human {
        for target in &targets {
            let zephyr = if target.zephyr.is_some() {
                " zephyr=true"
            } else {
                " zephyr=false"
            };
            println!(
                "target={} name=\"{}\" status={}{}",
                target.id,
                target.name,
                target.status.as_deref().unwrap_or(""),
                zephyr
            );
        }
    }
    Ok(json!({
        "targets": targets.iter().map(|target| target.summary_json()).collect::<Vec<_>>()
    }))
}

fn target_inspect(args: TargetOnlyArgs, human: bool) -> Result<Value, String> {
    let root = target::repo_root();
    let target = target::resolve_target_arg(
        &root,
        args.target.as_deref(),
        target::stdin_is_interactive(),
    )?;
    let data = target.inspect_json(&root);
    if human {
        println!("target={}", target.id);
        println!("name={}", target.name);
        println!("status={}", target.status.as_deref().unwrap_or(""));
        if let Some(zephyr) = &target.zephyr {
            println!("zephyr.board={}", zephyr.board);
            println!("zephyr.buildDir={}", root.join(&zephyr.build_dir).display());
            println!("zephyr.overlay={}", root.join(&zephyr.overlay).display());
            println!(
                "zephyr.fallbackSource={}",
                root.join(&zephyr.fallback_source).display()
            );
            println!(
                "zephyr.targetKconfig={}",
                root.join(&zephyr.target_kconfig).display()
            );
        } else {
            println!("zephyr.supported=false");
        }
        if let Some(native) = &target.native {
            println!("native.elf={}", root.join(&native.elf).display());
            println!("native.otaImage={}", root.join(&native.ota_image).display());
            println!(
                "native.partitionTable={}",
                root.join(&native.partition_table).display()
            );
        }
    }
    Ok(data)
}

fn target_build(args: TargetBuildArgs, human: bool) -> Result<Value, String> {
    let root = target::repo_root();
    let target_def = target::resolve_target_arg(
        &root,
        args.target.as_deref(),
        target::stdin_is_interactive(),
    )?;
    let backend = target::TargetBackend::Native;
    let plan = target::plan_build_command(
        &root,
        &target_def,
        target::TargetBuildPlanOptions {
            backend,
            stack_usage: args.stack_usage,
            pristine: args.pristine.into(),
            west_args: args.west_args,
        },
    )?;
    let image_plan = target::plan_native_image_command(&root, &target_def)?;
    if args.print_plan {
        if human {
            print_command_plan(&plan);
            print_command_plan(&image_plan);
        }
        return Ok(json!({
            "target": target_def.summary_json(),
            "plans": {"build": plan.as_json(), "otaImage": image_plan.as_json()}
        }));
    }
    if human {
        eprintln!("building target {}", target_def.id);
    }
    target::run_plan(&plan)?;
    target::run_plan(&image_plan)?;
    let ota_image_bytes = target::validate_native_ota_image(&root, &target_def)?;
    Ok(json!({
        "target": target_def.summary_json(),
        "plans": {"build": plan.as_json(), "otaImage": image_plan.as_json()},
        "artifacts": {
            "elf": root.join(&target_def.native()?.elf),
            "otaImage": root.join(&target_def.native()?.ota_image),
            "partitionTable": root.join(&target_def.native()?.partition_table),
            "otaImageBytes": ota_image_bytes
        }
    }))
}

fn target_flash(args: TargetFlashArgs, human: bool) -> Result<Value, String> {
    if !human && args.monitor_after_flash && !args.print_plan {
        return Err("target flash --json cannot stream monitor output; use --print-plan or omit --monitor-after-flash".to_string());
    }
    let root = target::repo_root();
    let target_def = target::resolve_target_arg(
        &root,
        args.target.as_deref(),
        target::stdin_is_interactive(),
    )?;
    let backend = target::TargetBackend::Native;
    let build_plan = target::plan_build_command(
        &root,
        &target_def,
        target::TargetBuildPlanOptions {
            backend,
            stack_usage: false,
            pristine: target::TargetPristine::Auto,
            west_args: Vec::new(),
        },
    )?;
    let flash_plan = target::plan_flash_command(
        &root,
        &target_def,
        target::TargetFlashPlanOptions {
            backend,
            west_args: args.west_args,
        },
    )?;
    let image_plan = target::plan_native_image_command(&root, &target_def)?;
    let monitor_plan = if args.monitor_after_flash {
        let port = if args.print_plan {
            None
        } else {
            Some(detect_port()?)
        };
        Some(target::plan_monitor_command(
            &root,
            &target_def,
            target::TargetMonitorPlanOptions {
                backend,
                port,
                west_args: Vec::new(),
            },
        )?)
    } else {
        None
    };
    if args.print_plan {
        if human {
            print_command_plan(&build_plan);
            print_command_plan(&image_plan);
            print_command_plan(&flash_plan);
            if let Some(plan) = &monitor_plan {
                print_command_plan(plan);
            }
        }
        return Ok(json!({
            "target": target_def.summary_json(),
            "plans": {
                "build": build_plan.as_json(),
                "otaImage": image_plan.as_json(),
                "flash": flash_plan.as_json(),
                "monitor": monitor_plan.as_ref().map(|plan| plan.as_json())
            }
        }));
    }
    if human {
        eprintln!("building target {}", target_def.id);
    }
    target::run_plan(&build_plan)?;
    target::run_plan(&image_plan)?;
    target::validate_native_ota_image(&root, &target_def)?;
    if human {
        eprintln!("flashing target {}", target_def.id);
    }
    target::run_plan(&flash_plan)?;
    if let Some(plan) = &monitor_plan {
        target::run_plan_streaming(plan)?;
    }
    Ok(json!({
        "target": target_def.summary_json(),
        "plans": {
            "build": build_plan.as_json(),
            "otaImage": image_plan.as_json(),
            "flash": flash_plan.as_json(),
            "monitor": monitor_plan.as_ref().map(|plan| plan.as_json())
        }
    }))
}

#[derive(Clone, Copy, Debug, Serialize)]
struct HardwareTestCheck {
    name: &'static str,
    script: Option<&'static str>,
}

fn hardware_test(args: HardwareTestArgs, human: bool) -> Result<Value, String> {
    let root = target::repo_root();
    let target_def = target::resolve_target_arg(
        &root,
        args.target.as_deref(),
        target::stdin_is_interactive(),
    )?;
    let checks = hardware_test_checks_for_target(&target_def);
    if args.list {
        if human {
            for check in &checks {
                println!("hardware-test={}", check.name);
            }
        }
        return Ok(json!({
            "target": target_def.summary_json(),
            "checks": checks
        }));
    }

    if !args.skip_flash {
        target_flash(
            TargetFlashArgs {
                target: Some(target_def.id.clone()),
                monitor_after_flash: false,
                print_plan: false,
                west_args: Vec::new(),
            },
            human,
        )?;
        wait_for_hardware_test_device_reset(args.port.as_deref())?;
    }

    let mut results = Vec::new();
    for check in &checks {
        if human {
            eprintln!("running hardware test {}", check.name);
        }
        match check.name {
            "portable-app-tests" => {
                app_test(
                    AppTestArgs {
                        device: DeviceOnlyOptions {
                            port: args.port.clone(),
                        },
                        target: Some(target_def.id.clone()),
                        check_target: false,
                        negative: false,
                        list: false,
                        input: root.join("examples/app-tests/portable"),
                    },
                    human,
                )?;
            }
            _ => run_hardware_script(&root, check, &target_def, &args)?,
        }
        results.push(json!({"name": check.name, "status": "ok"}));
    }

    Ok(json!({
        "target": target_def.summary_json(),
        "checks": results
    }))
}

fn wait_for_hardware_test_device_reset(port: Option<&str>) -> Result<(), String> {
    const ATTEMPTS: usize = 8;
    const DELAY: Duration = Duration::from_secs(2);

    let mut last_error = None;
    for attempt in 1..=ATTEMPTS {
        let result = match port {
            Some(port) => {
                SerialDevice::open(port).and_then(|mut device| device.reset().map(|_| ()))
            }
            None => detect_port().and_then(|port| {
                SerialDevice::open(&port).and_then(|mut device| device.reset().map(|_| ()))
            }),
        };
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                let retryable = hardware_test_reset_error_is_retryable(&error);
                last_error = Some(error);
                if !retryable || attempt == ATTEMPTS {
                    break;
                }
                std::thread::sleep(DELAY);
            }
        }
    }
    Err(format!(
        "firmware did not become ready for hardware tests after flash: {}",
        last_error.unwrap_or_else(|| "no response".to_string())
    ))
}

fn hardware_test_reset_error_is_retryable(error: &str) -> bool {
    error.contains("(-116)")
        || error.contains("(ETIMEDOUT)")
        || error.contains("busy (-16)")
        || error.contains("firmware did not become ready")
        || error.contains("BadMagic")
        || error.contains("TruncatedHeader")
        || error.contains("LengthMismatch")
        || error.contains("no SquidScript firmware serial target found")
}

fn hardware_test_checks_for_target(target: &target::TargetDefinition) -> Vec<HardwareTestCheck> {
    let has = |feature: &str| target.features.iter().any(|value| value == feature);
    let mut checks = Vec::new();
    if has("squidscript.serial-install") || has("squidscript.bytecode.v2.reference") {
        checks.push(HardwareTestCheck {
            name: "portable-app-tests",
            script: None,
        });
    }
    if has("service.ble.file-transfer") {
        checks.push(HardwareTestCheck {
            name: "ble-file-transfer-install",
            script: Some("scripts/zephyr-test-ble-file-transfer.sh"),
        });
        checks.push(HardwareTestCheck {
            name: "ble-installed-receiver",
            script: Some("scripts/zephyr-test-ble-installed-receiver.sh"),
        });
        checks.push(HardwareTestCheck {
            name: "ble-reconnect",
            script: Some("scripts/zephyr-test-ble-reconnect.sh"),
        });
    }
    let has_station_wifi = has("service.wifi.connect") && has("service.wifi.disconnect");
    let has_ap_wifi = has("service.wifi.startAP") && has("service.wifi.stopAP");
    if has("service.ble.file-transfer") && has_station_wifi {
        checks.push(HardwareTestCheck {
            name: "radio-concurrency",
            script: Some("scripts/zephyr-test-radio-concurrency.sh"),
        });
    }
    if has_station_wifi && has_ap_wifi {
        checks.push(HardwareTestCheck {
            name: "ap-after-station",
            script: Some("scripts/zephyr-test-ap-after-station.sh"),
        });
    }
    if target.id == "xteink-x4" && has("service.ble.file-transfer") && has_ap_wifi {
        checks.push(HardwareTestCheck {
            name: "transfer-regression",
            script: Some("scripts/xteink-x4-test-transfer-regression.sh"),
        });
    }
    checks
}

fn run_hardware_script(
    root: &Path,
    check: &HardwareTestCheck,
    target_def: &target::TargetDefinition,
    args: &HardwareTestArgs,
) -> Result<(), String> {
    let script = check
        .script
        .ok_or_else(|| format!("hardware test {} has no runner", check.name))?;
    let mut command = Command::new(root.join(script));
    command.current_dir(root);
    command.arg("--target").arg(&target_def.id);
    command.arg("--skip-flash");
    match check.name {
        "ble-file-transfer-install" => {
            if let Some(port) = &args.port {
                command.arg("--port").arg(port);
            }
            if let Some(device) = &args.ble_device {
                command.arg("--device").arg(device);
            }
        }
        "ble-installed-receiver" => {
            if let Some(port) = &args.port {
                command.arg("--port").arg(port);
            }
            if let Some(device) = &args.ble_device {
                command.arg("--device").arg(device);
            }
        }
        "ble-reconnect" => {
            if let Some(device) = &args.ble_device {
                command.arg("--device").arg(device);
            }
        }
        "radio-concurrency" => {
            if let Some(device) = &args.ble_device {
                command.arg("--device").arg(device);
            }
            if let Some(iface) = &args.host_wifi_iface {
                command.arg("--host-wifi-iface").arg(iface);
            }
        }
        "ap-after-station" => {
            if let Some(iface) = &args.host_wifi_iface {
                command.env("HOST_WIFI_IFACE", iface);
            }
        }
        "transfer-regression" => {
            if let Some(port) = &args.port {
                command.arg("--port").arg(port);
            }
            if let Some(device) = &args.ble_device {
                command.arg("--device").arg(device);
            }
            if let Some(iface) = &args.host_wifi_iface {
                command.arg("--host-wifi-iface").arg(iface);
            }
        }
        _ => {}
    }
    let status = command
        .status()
        .map_err(|error| format!("failed to run {script}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "hardware test {} failed with status {}",
            check.name,
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ))
    }
}

fn target_monitor(args: TargetMonitorArgs, human: bool) -> Result<Value, String> {
    if !human && !args.print_plan {
        return Err(
            "target monitor --json requires --print-plan because monitor output is a stream"
                .to_string(),
        );
    }
    let root = target::repo_root();
    let target_def = target::resolve_target_arg(
        &root,
        args.target.as_deref(),
        target::stdin_is_interactive(),
    )?;
    let backend = target::TargetBackend::Native;
    let plan = target::plan_monitor_command(
        &root,
        &target_def,
        target::TargetMonitorPlanOptions {
            backend,
            port: match args.port {
                Some(port) => Some(port),
                None if args.print_plan => None,
                None => Some(detect_port()?),
            },
            west_args: args.west_args,
        },
    )?;
    if args.print_plan {
        if human {
            print_command_plan(&plan);
        }
        return Ok(json!({"target": target_def.summary_json(), "plan": plan.as_json()}));
    }
    target::run_plan_streaming(&plan)?;
    Ok(json!({"target": target_def.summary_json(), "plan": plan.as_json()}))
}

fn target_doctor(args: TargetDoctorArgs, human: bool) -> Result<Value, String> {
    let root = target::repo_root();
    let target_def = target::resolve_target_arg(
        &root,
        args.target.as_deref(),
        target::stdin_is_interactive(),
    )?;
    let checks = target::doctor_checks(&root, &target_def, args.port.as_deref());
    let failed = checks
        .iter()
        .any(|check| check["status"].as_str() == Some("fail"));
    let warning = checks
        .iter()
        .any(|check| check["status"].as_str() == Some("warn"));
    let summary = if failed {
        "fail"
    } else if warning {
        "warn"
    } else {
        "ok"
    };
    if human {
        for check in &checks {
            println!(
                "[{}] {}: {}",
                check["status"].as_str().unwrap_or("unknown"),
                check["name"].as_str().unwrap_or("check"),
                check["message"].as_str().unwrap_or("")
            );
        }
    }
    Ok(json!({
        "target": target_def.summary_json(),
        "summary": summary,
        "checks": checks
    }))
}

fn print_command_plan(plan: &target::CommandPlan) {
    println!("cwd={}", plan.cwd.display());
    for (key, value) in &plan.env {
        println!("env.{key}={value}");
    }
    println!("command={}", plan.command_line());
}

fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

fn resources(args: DeviceResourcesArgs, human: bool) -> Result<Value, String> {
    let port = resolve_port(&args.device)?;
    let mut device = SerialDevice::open(&port)?;
    let count = args.count.max(1);
    let mut samples = Vec::new();

    for index in 0..count {
        let epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system time error: {error}"))?
            .as_millis();
        let resources = device.resource_values(args.reset_heap_max && index == 0)?;
        if human {
            if count > 1 {
                if index > 0 {
                    println!();
                }
                println!("sample_epoch_ms={epoch_ms}");
            }
            for (key, value) in &resources {
                println!("{key}={value}");
            }
        }
        samples.push(json!({
            "epoch_ms": epoch_ms,
            "resources": resources.iter().map(|(key, value)| {
                json!({"key": key, "value": value})
            }).collect::<Vec<_>>()
        }));
        if index + 1 < count && args.interval_ms > 0 {
            std::thread::sleep(Duration::from_millis(args.interval_ms));
        }
    }
    let resources = samples
        .last()
        .and_then(|sample| sample.get("resources"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    Ok(json!({
        "port": port,
        "resources": resources,
        "samples": samples,
    }))
}

fn reset(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.reset()?;
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "reset", "response": response}))
}

fn storage_format(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.storage_format()?;
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "storage-format", "response": response}))
}

fn device_output(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let lines = device.output_lines()?;
    if human {
        for line in &lines {
            println!("output={line}");
        }
    }
    Ok(json!({
        "port": port,
        "command": "output",
        "lines": lines
    }))
}

fn state(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let bytes = device.state_bytes()?;
    let response = format_state_bytes(&bytes);
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "state", "stateHex": hex_string(&bytes)}))
}

fn trace(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let lines = device.trace_lines()?;
    let response = format_lines("trace", &lines);
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "trace", "lines": lines}))
}

fn lifecycle(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let lines = device.lifecycle_lines()?;
    let response = format_lines("lifecycle", &lines);
    if human {
        print!("{response}");
    }
    let details = lifecycle_details(&lines);
    Ok(json!({
        "port": port,
        "command": "lifecycle",
        "lines": lines,
        "active": details["active"].clone(),
        "processStack": details["processStack"].clone(),
        "armedStack": details["armedStack"].clone()
    }))
}

fn lifecycle_details(lines: &[String]) -> Value {
    let mut active = None;
    let mut process_stack = Vec::new();
    let mut armed_stack = Vec::new();

    for line in lines {
        if let Some(value) = line.strip_prefix("active=") {
            active = Some(value.to_string());
            continue;
        }
        if let Some((_, value)) = indexed_lifecycle_value(line, "process_stack") {
            process_stack.push(value.to_string());
            continue;
        }
        if let Some((_, value)) = indexed_lifecycle_value(line, "armed_stack") {
            if value.is_empty() {
                continue;
            }
            let (app_id, event) = value.split_once(' ').unwrap_or((value, ""));
            armed_stack.push(json!({
                "appId": app_id,
                "event": event
            }));
        }
    }

    json!({
        "active": active,
        "processStack": process_stack,
        "armedStack": armed_stack
    })
}

fn indexed_lifecycle_value<'a>(line: &'a str, prefix: &str) -> Option<(usize, &'a str)> {
    let rest = line.strip_prefix(prefix)?.strip_prefix('[')?;
    let (index, rest) = rest.split_once("]=")?;
    Some((index.parse().ok()?, rest))
}

fn drawlog(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let lines = device.drawlog_lines()?;
    let response = format_raw_lines(&lines);
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "drawlog", "lines": lines}))
}

fn debug_log(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let lines = device.debug_log_lines()?;
    let response = format_raw_lines(&lines);
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "debug-log", "lines": lines}))
}

fn errors(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let lines = device.error_lines()?;
    let response = format_lines("error", &lines);
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "errors", "lines": lines}))
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
        let output = device.output_lines()?;
        for line in tail.next_lines(&output) {
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
                    "note": "riscv64-elf-gcc is used with -march=rv32imc -mabi=ilp32 by `squidc target build --target esp32c3-super-mini`"
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
    session_app_id: String,
    profile: BuildProfile,
    mode: ReplMode,
    base_source: String,
    state_block: String,
    snippet: String,
    last_state: String,
    last_output: String,
    last_drawlog: String,
    last_trace: String,
    output_tail: OutputTail,
    temp_dir: PathBuf,
    echo: bool,
}

fn app_test_session_app_id(test_name: &str, index: usize) -> String {
    format!(
        "app-test-{:x}-{:08x}-{index:x}",
        std::process::id(),
        fnv1a(test_name.as_bytes())
    )
}

fn repl_session_start_command(app_id: &str) -> [&str; 2] {
    ["launch-app", app_id]
}

fn repl_session_format_new_output(tail: &mut OutputTail, lines: &[String]) -> String {
    tail.next_lines(lines)
        .into_iter()
        .map(|line| format!("{line}\n"))
        .collect()
}

fn repl_session_updated_output(previous: &str, next: &str) -> String {
    if next.is_empty() {
        previous.to_string()
    } else {
        next.to_string()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReplMode {
    Event,
    Render,
}

impl ReplSession {
    fn new(target: String, port: String, base_source: String, echo: bool) -> Self {
        Self::new_with_app_id(target, port, base_source, echo, "repl-session".to_string())
    }

    fn new_with_app_id(
        target: String,
        port: String,
        base_source: String,
        echo: bool,
        session_app_id: String,
    ) -> Self {
        let state_block =
            extract_state_block(&base_source).unwrap_or_else(|| "state {}\n".to_string());
        Self {
            target,
            port,
            session_app_id,
            profile: BuildProfile::Dev,
            mode: ReplMode::Event,
            base_source,
            state_block,
            snippet: String::new(),
            last_state: String::new(),
            last_output: String::new(),
            last_drawlog: String::new(),
            last_trace: String::new(),
            output_tail: OutputTail::new(),
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
                let output = self.output_text()?;
                self.last_output = repl_session_updated_output(&self.last_output, &output);
                Ok(())
            }
            ":drawlog" => {
                self.flush_snippet()?;
                self.last_drawlog = self.serial_text(&["drawlog"])?;
                Ok(())
            }
            ":trace" => {
                self.flush_snippet()?;
                self.last_trace = self.serial_text(&["trace"])?;
                Ok(())
            }
            ":key" => {
                self.flush_snippet()?;
                let key = parts.next().ok_or_else(|| "missing key".to_string())?;
                self.serial_text(&["key", key])?;
                self.last_output = self.output_text()?;
                Ok(())
            }
            ":reset" => {
                self.flush_snippet()?;
                self.serial_text(&["reset"])?;
                self.output_tail = OutputTail::new();
                Ok(())
            }
            ":reload" => {
                self.flush_snippet()?;
                self.reload_base_source()
            }
            ":expect-state" => self.expect_contains("state", command, &self.last_state),
            ":expect-output" => self.expect_contains("output", command, &self.last_output),
            ":expect-draw" => self.expect_contains("drawlog", command, &self.last_drawlog),
            ":expect-trace" => self.expect_contains("trace", command, &self.last_trace),
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

        self.serial_text(&["install-app", &self.session_app_id, path_str(&sqbc_path)?])?;
        self.discard_output()?;
        self.serial_text(&repl_session_start_command(&self.session_app_id))?;
        if fs::metadata(&state_path).map(|m| m.len()).unwrap_or(0) > 0 {
            self.serial_text(&["state-import", path_str(&state_path)?])?;
        }

        match self.mode {
            ReplMode::Event => {
                self.serial_text(&["run-app-event", &self.session_app_id, "repl"])?;
                self.last_output = self.output_text()?;
            }
            ReplMode::Render => {
                self.serial_text(&["run-app-event", &self.session_app_id, "app.start"])?;
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

        self.serial_text(&["install-app", &self.session_app_id, path_str(&sqbc_path)?])?;
        self.discard_output()?;
        self.serial_text(&repl_session_start_command(&self.session_app_id))?;
        self.last_output = self.output_text()?;
        self.last_drawlog = self.serial_text(&["drawlog"])?;
        self.last_state = self.serial_text(&["state"])?;
        Ok(())
    }

    fn generated_source_with_state(&self, state_output: &str) -> String {
        let mut source = format!(
            "app \"{}\"\n\n{}",
            self.session_app_id,
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
            ["launch-app", app_id] => device.run_app(app_id)?,
            ["run-app-event", app_id, event] => device.run_app_event(app_id, event)?,
            ["state-import", path] => {
                let bytes =
                    fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
                device.import_state(&bytes)?
            }
            ["state"] => format_state_bytes(&device.state_bytes()?),
            ["output"] => format_lines("output", &device.output_lines()?),
            ["trace"] => format_lines("trace", &device.trace_lines()?),
            ["drawlog"] => format_raw_lines(&device.drawlog_lines()?),
            ["resources"] => device
                .resource_values(false)?
                .into_iter()
                .map(|(key, value)| format!("{key}={value}\n"))
                .collect::<String>(),
            ["key", key] => device.send_key(key)?,
            ["reset"] => device.reset()?,
            _ => return Err(format!("unsupported repl serial command: {args:?}")),
        };
        if self.echo {
            print!("{output}");
        }
        Ok(output)
    }

    fn output_text(&mut self) -> Result<String, String> {
        let mut device = SerialDevice::open(&self.port)?;
        let lines = device.output_lines()?;
        let output = repl_session_format_new_output(&mut self.output_tail, &lines);
        if self.echo {
            print!("{output}");
        }
        Ok(output)
    }

    fn discard_output(&mut self) -> Result<(), String> {
        let mut device = SerialDevice::open(&self.port)?;
        let lines = device.output_lines()?;
        let _ = repl_session_format_new_output(&mut self.output_tail, &lines);
        Ok(())
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
    if let Some(hex) = state_output
        .lines()
        .find_map(|line| line.strip_prefix("state=").map(str::trim))
    {
        return decode_hex_bytes(hex).unwrap_or_default();
    }
    let mut out = String::new();
    for line in state_output.lines() {
        if line.starts_with("exited=") {
            continue;
        }
        if line.contains('=') {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.into_bytes()
}

fn decode_hex_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for index in (0..hex.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&hex[index..index + 2], 16).ok()?);
    }
    Some(bytes)
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
    fn parses_grouped_app_build_command() {
        let cli = Cli::try_parse_from([
            "squidc",
            "app",
            "build",
            "examples/blinky-supermini/main.squid",
            "--out",
            "target/blinky.sqbc",
        ])
        .unwrap();
        let Commands::App {
            command: AppCommands::Build(args),
        } = cli.command
        else {
            panic!("expected app build");
        };
        assert_eq!(
            args.input,
            PathBuf::from("examples/blinky-supermini/main.squid")
        );
        assert_eq!(args.out, PathBuf::from("target/blinky.sqbc"));
    }

    #[test]
    fn parses_grouped_app_package_command_with_default_output() {
        let cli =
            Cli::try_parse_from(["squidc", "app", "package", "examples/binbook-reader"]).unwrap();
        let Commands::App {
            command: AppCommands::Package(args),
        } = cli.command
        else {
            panic!("expected app package");
        };
        assert_eq!(args.input, PathBuf::from("examples/binbook-reader"));
        assert_eq!(args.out, None);
    }

    #[test]
    fn parses_grouped_app_run_command() {
        let cli = Cli::try_parse_from([
            "squidc",
            "app",
            "run",
            "examples/blinky-supermini/main.squid",
        ])
        .unwrap();
        let Commands::App {
            command: AppCommands::Run(args),
        } = cli.command
        else {
            panic!("expected app run");
        };
        assert_eq!(
            args.input,
            PathBuf::from("examples/blinky-supermini/main.squid")
        );
    }

    #[test]
    fn parses_app_test_positive_and_negative_commands() {
        let positive =
            Cli::try_parse_from(["squidc", "app", "test", "examples/app-tests/portable"]).unwrap();
        let Commands::App {
            command: AppCommands::Test(args),
        } = positive.command
        else {
            panic!("expected app test");
        };
        assert_eq!(args.input, PathBuf::from("examples/app-tests/portable"));
        assert!(!args.negative);
        assert!(!args.list);

        let negative = Cli::try_parse_from([
            "squidc",
            "app",
            "test",
            "--negative",
            "tests/squidscript/negative",
        ])
        .unwrap();
        let Commands::App {
            command: AppCommands::Test(args),
        } = negative.command
        else {
            panic!("expected app test --negative");
        };
        assert_eq!(args.input, PathBuf::from("tests/squidscript/negative"));
        assert!(args.negative);
    }

    #[test]
    fn repl_session_foregrounds_installed_base_app_with_launch() {
        assert_eq!(
            repl_session_start_command("test-session"),
            ["launch-app", "test-session"]
        );
    }

    #[test]
    fn repl_session_output_tail_ignores_prelaunch_output() {
        let mut tail = OutputTail::new();
        let stale = vec!["view menu 0".to_string(), "view menu 1".to_string()];
        let _ = repl_session_format_new_output(&mut tail, &stale);

        let current = vec![
            "view menu 0".to_string(),
            "view menu 1".to_string(),
            "view library 0".to_string(),
        ];
        assert_eq!(
            repl_session_format_new_output(&mut tail, &current),
            "output=view library 0\n"
        );
    }

    #[test]
    fn app_test_session_app_id_is_unique_and_safe() {
        let first = app_test_session_app_id("xteink/binbook-reader-selection", 0);
        let second = app_test_session_app_id("xteink/binbook-reader-selection", 1);
        assert_ne!(first, second);
        assert!(first.starts_with("app-test-"));
        assert!(first.len() < 40);
        assert!(first
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'));
    }

    #[test]
    fn repl_session_preserves_last_output_when_no_new_lines_arrive() {
        assert_eq!(
            repl_session_updated_output("output=view library 1\n", ""),
            "output=view library 1\n"
        );
        assert_eq!(
            repl_session_updated_output("output=view library 1\n", "output=view reader 0\n"),
            "output=view reader 0\n"
        );
    }

    #[test]
    fn parses_generic_content_put_and_check_commands() {
        let put = Cli::try_parse_from([
            "squidc",
            "device",
            "content-put",
            "target/transfer-smoke.dat",
            "--name",
            "transfer-smoke.dat",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::ContentPut(put_args),
        } = put.command
        else {
            panic!("expected device content-put");
        };
        assert_eq!(put_args.input, PathBuf::from("target/transfer-smoke.dat"));
        assert_eq!(put_args.name.as_deref(), Some("transfer-smoke.dat"));

        let check = Cli::try_parse_from([
            "squidc",
            "device",
            "content-check",
            "transfer-smoke.dat",
            "--size",
            "8192",
            "--crc32",
            "1234abcd",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::ContentCheck(check_args),
        } = check.command
        else {
            panic!("expected device content-check");
        };
        assert_eq!(check_args.name, "transfer-smoke.dat");
        assert_eq!(check_args.size, 8192);
        assert_eq!(check_args.crc32, "1234abcd");

        let http = Cli::try_parse_from([
            "squidc",
            "device",
            "upload",
            "target/transfer-smoke.dat",
            "--name",
            "transfer-smoke.dat",
            "--transport",
            "http",
            "--host",
            "192.168.4.1",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::Upload(http_args),
        } = http.command
        else {
            panic!("expected HTTP device upload");
        };
        assert_eq!(http_args.transport, UploadTransportArg::Http);
        assert_eq!(http_args.host.as_deref(), Some("192.168.4.1"));
        assert_eq!(http_args.port, None);

        let ble = Cli::try_parse_from([
            "squidc",
            "device",
            "upload",
            "target/transfer-smoke.dat",
            "--name",
            "transfer-smoke.dat",
            "--transport",
            "ble",
            "--device",
            "SquidScript-X4",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::Upload(ble_args),
        } = ble.command
        else {
            panic!("expected BLE device upload");
        };
        assert_eq!(ble_args.transport, UploadTransportArg::Ble);
        assert_eq!(ble_args.device.as_deref(), Some("SquidScript-X4"));
    }

    #[test]
    fn parses_firmware_info_and_update_commands() {
        let info =
            Cli::try_parse_from(["squidc", "device", "firmware-info", "--port", "/dev/test"])
                .unwrap();
        let Commands::Device {
            command: DeviceCommands::FirmwareInfo(args),
        } = info.command
        else {
            panic!("expected firmware info");
        };
        assert_eq!(args.device.port.as_deref(), Some("/dev/test"));

        let update = Cli::try_parse_from([
            "squidc",
            "device",
            "firmware-update",
            "firmware.bin",
            "--port",
            "/dev/test",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::FirmwareUpdate(args),
        } = update.command
        else {
            panic!("expected firmware update");
        };
        assert_eq!(args.image, PathBuf::from("firmware.bin"));
        assert_eq!(args.device.port.as_deref(), Some("/dev/test"));
    }

    #[test]
    fn device_upload_requires_transport_destination_and_safe_name() {
        let longest_name = format!("{}.binbook", "a".repeat(113));
        let too_long_name = format!("{}.binbook", "a".repeat(114));
        assert_eq!(
            longest_name.len(),
            squid_device_protocol::MAX_CONTENT_NAME_BYTES
        );
        assert!(is_safe_content_name(&longest_name));
        assert!(!is_safe_content_name(&too_long_name));
        assert!(!is_safe_content_name("cafe\u{301}.binbook"));

        let missing_host = device_upload(
            DeviceUploadArgs {
                input: PathBuf::from("missing.binbook"),
                name: "book.binbook".to_string(),
                transport: UploadTransportArg::Http,
                host: None,
                device: None,
                port: None,
            },
            false,
        )
        .unwrap_err();
        assert_eq!(missing_host, "--host is required for --transport http");

        let missing_device = device_upload(
            DeviceUploadArgs {
                input: PathBuf::from("missing.binbook"),
                name: "book.binbook".to_string(),
                transport: UploadTransportArg::Ble,
                host: None,
                device: None,
                port: None,
            },
            false,
        )
        .unwrap_err();
        assert_eq!(missing_device, "--device is required for --transport ble");

        let unsafe_name = device_upload(
            DeviceUploadArgs {
                input: PathBuf::from("missing.binbook"),
                name: "../book.binbook".to_string(),
                transport: UploadTransportArg::Http,
                host: Some("127.0.0.1".to_string()),
                device: None,
                port: None,
            },
            false,
        )
        .unwrap_err();
        assert!(unsafe_name.contains("simple filename"));
    }

    #[test]
    fn parses_display_window_probe_command() {
        let cli =
            Cli::try_parse_from(["squidc", "device", "display-window-probe", "corners"]).unwrap();
        let Commands::Device {
            command: DeviceCommands::DisplayWindowProbe(args),
        } = cli.command
        else {
            panic!("expected device display-window-probe");
        };
        assert_eq!(args.pattern, "corners");
    }

    #[test]
    fn parses_hardware_test_command() {
        let cli = Cli::try_parse_from([
            "squidc",
            "hardware",
            "test",
            "--target",
            "xiao-esp32c3-gdeq0426t82-sd",
            "--skip-flash",
            "--port",
            "/dev/ttyACM0",
            "--ble-device",
            "SquidScript",
            "--host-wifi-iface",
            "wlan0",
        ])
        .unwrap();
        let Commands::Hardware {
            command: HardwareCommands::Test(args),
        } = cli.command
        else {
            panic!("expected hardware test");
        };
        assert_eq!(args.target.as_deref(), Some("xiao-esp32c3-gdeq0426t82-sd"));
        assert!(args.skip_flash);
        assert_eq!(args.port.as_deref(), Some("/dev/ttyACM0"));
        assert_eq!(args.ble_device.as_deref(), Some("SquidScript"));
        assert_eq!(args.host_wifi_iface.as_deref(), Some("wlan0"));
    }

    #[test]
    fn xiao_hardware_test_selection_includes_current_supported_checks_only() {
        let root = target::repo_root();
        let target = target::load_target_by_id(&root, "xiao-esp32c3-gdeq0426t82-sd").unwrap();
        let checks = hardware_test_checks_for_target(&target);
        let names = checks.iter().map(|check| check.name).collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "portable-app-tests",
                "ble-file-transfer-install",
                "ble-installed-receiver",
                "ble-reconnect",
                "radio-concurrency",
                "ap-after-station",
            ]
        );
        assert!(!names.contains(&"display-drawlog"));
        assert!(!names.contains(&"sd-card"));
    }

    #[test]
    fn hardware_test_post_flash_reset_retries_readiness_failures_only() {
        assert!(hardware_test_reset_error_is_retryable(
            "command failed (-116)"
        ));
        assert!(hardware_test_reset_error_is_retryable("busy (-16)"));
        assert!(hardware_test_reset_error_is_retryable(
            "firmware did not become ready for protocol commands: BadMagic"
        ));
        assert!(hardware_test_reset_error_is_retryable(
            "invalid hello frame: TruncatedHeader"
        ));
        assert!(hardware_test_reset_error_is_retryable(
            "no SquidScript firmware serial target found"
        ));
        assert!(!hardware_test_reset_error_is_retryable(
            "command failed (-5)"
        ));
        assert!(!hardware_test_reset_error_is_retryable(
            "failed to configure /dev/ttyACM0 with stty"
        ));
    }

    #[test]
    fn app_test_discovers_small_example_directories() {
        let root = unique_test_dir("squidc-app-tests");
        let suite = root.join("portable");
        fs::create_dir_all(suite.join("state-counter")).unwrap();
        fs::create_dir_all(suite.join("timer-event")).unwrap();
        fs::write(
            suite.join("state-counter").join("main.squid"),
            "app \"state-counter\"\n",
        )
        .unwrap();
        fs::write(
            suite.join("timer-event").join("main.squid"),
            "app \"timer-event\"\n",
        )
        .unwrap();

        let tests = discover_app_test_paths(&suite).unwrap();

        assert_eq!(
            tests,
            vec![
                suite.join("state-counter").join("main.squid"),
                suite.join("timer-event").join("main.squid"),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn negative_app_test_fixture_reads_expected_diagnostic() {
        let root = unique_test_dir("squidc-negative-tests");
        let fixture = root.join("undeclared-variable");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(
            fixture.join("main.squid"),
            "app \"bad\"\ndebug.print(missing)\n",
        )
        .unwrap();
        fs::write(fixture.join("expected.txt"), "E_UNDECLARED_VARIABLE\n").unwrap();

        let tests = discover_negative_app_tests(&root).unwrap();

        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].source, fixture.join("main.squid"));
        assert_eq!(tests[0].expected_diagnostic, "E_UNDECLARED_VARIABLE");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_removed_top_level_app_commands() {
        for command in ["build", "package", "run"] {
            assert!(
                Cli::try_parse_from(["squidc", command, "examples/blinky-supermini/main.squid"])
                    .is_err(),
                "{command} should only exist under squidc app"
            );
        }
    }

    #[test]
    fn parses_target_build_command_with_print_plan_and_forwarded_args() {
        let cli = Cli::try_parse_from([
            "squidc",
            "target",
            "build",
            "--target",
            "esp32c3-super-mini",
            "--stack-usage",
            "--pristine",
            "always",
            "--print-plan",
            "--",
            "-DOVERLAY_CONFIG=extra.conf",
        ])
        .unwrap();
        let Commands::Target {
            command: TargetCommands::Build(args),
        } = cli.command
        else {
            panic!("expected target build");
        };
        assert_eq!(args.target.as_deref(), Some("esp32c3-super-mini"));
        assert!(args.stack_usage);
        assert!(args.print_plan);
        assert_eq!(args.pristine, TargetPristineArg::Always);
        assert_eq!(args.west_args, vec!["-DOVERLAY_CONFIG=extra.conf"]);
    }

    #[test]
    fn target_build_has_no_backend_selector() {
        let cli = Cli::try_parse_from([
            "squidc",
            "target",
            "build",
            "--target",
            "xteink-x4",
            "--print-plan",
        ])
        .unwrap();
        let Commands::Target {
            command: TargetCommands::Build(args),
        } = cli.command
        else {
            panic!("expected target build");
        };
        assert_eq!(args.target.as_deref(), Some("xteink-x4"));
        assert!(args.print_plan);
    }

    #[test]
    fn parses_target_flash_monitor_doctor_and_inspect_commands() {
        let flash = Cli::try_parse_from([
            "squidc",
            "target",
            "flash",
            "--target",
            "esp32c3-super-mini",
            "--monitor-after-flash",
            "--",
            "--runner",
            "esp32",
        ])
        .unwrap();
        let Commands::Target {
            command: TargetCommands::Flash(args),
        } = flash.command
        else {
            panic!("expected target flash");
        };
        assert_eq!(args.target.as_deref(), Some("esp32c3-super-mini"));
        assert!(args.monitor_after_flash);
        assert_eq!(args.west_args, vec!["--runner", "esp32"]);

        let monitor = Cli::try_parse_from([
            "squidc",
            "target",
            "monitor",
            "--target",
            "esp32c3-super-mini",
            "--port",
            "/dev/ttyACM0",
        ])
        .unwrap();
        let Commands::Target {
            command: TargetCommands::Monitor(args),
        } = monitor.command
        else {
            panic!("expected target monitor");
        };
        assert_eq!(args.target.as_deref(), Some("esp32c3-super-mini"));
        assert_eq!(args.port.as_deref(), Some("/dev/ttyACM0"));

        let doctor = Cli::try_parse_from([
            "squidc",
            "target",
            "doctor",
            "--target",
            "esp32c3-super-mini",
        ])
        .unwrap();
        assert!(matches!(
            doctor.command,
            Commands::Target {
                command: TargetCommands::Doctor(_)
            }
        ));

        let inspect = Cli::try_parse_from([
            "squidc",
            "target",
            "inspect",
            "--target",
            "esp32c3-super-mini",
        ])
        .unwrap();
        assert!(matches!(
            inspect.command,
            Commands::Target {
                command: TargetCommands::Inspect(_)
            }
        ));
    }

    #[test]
    fn loads_zephyr_target_metadata_and_plans_build_command() {
        let root = target::repo_root();
        let target = target::load_target_by_id(&root, "esp32c3-super-mini").unwrap();
        assert_eq!(target.id, "esp32c3-super-mini");
        let zephyr = target.zephyr.as_ref().expect("super mini zephyr metadata");
        assert_eq!(zephyr.board, "esp32c3_supermini");

        let plan = target::plan_build_command(
            &root,
            &target,
            target::TargetBuildPlanOptions {
                backend: target::TargetBackend::Zephyr,
                stack_usage: true,
                pristine: target::TargetPristine::Always,
                west_args: vec!["-DOVERLAY_CONFIG=extra.conf".to_string()],
            },
        )
        .unwrap();
        assert_eq!(plan.program, "west");
        assert!(plan.args.starts_with(&[
            "build".to_string(),
            "--build-dir".to_string(),
            root.join("build/zephyr/c3-supermini").display().to_string(),
            "--board".to_string(),
            "esp32c3_supermini".to_string(),
            "--pristine".to_string(),
            "always".to_string(),
        ]));
        assert!(plan
            .env
            .iter()
            .any(|(key, value)| { key == "SQUID_ZEPHYR_STACK_USAGE" && value == "1" }));
        assert!(plan.env.iter().any(|(key, value)| {
            key == "SQUID_ZEPHYR_TARGET_JSON"
                && value.ends_with("targets/esp32c3-super-mini.target.json")
        }));
        assert!(plan
            .args
            .contains(&"-DOVERLAY_CONFIG=extra.conf".to_string()));
    }

    #[test]
    fn target_without_zephyr_metadata_is_unsupported_for_build() {
        let root = target::repo_root();
        let mut target = target::load_target_by_id(&root, "xteink-x4").unwrap();
        target.zephyr = None;
        let error = target::plan_build_command(
            &root,
            &target,
            target::TargetBuildPlanOptions {
                backend: target::TargetBackend::Zephyr,
                stack_usage: false,
                pristine: target::TargetPristine::Auto,
                west_args: Vec::new(),
            },
        )
        .unwrap_err();
        assert!(error.contains("has no Zephyr firmware metadata"));
    }

    #[test]
    fn loads_native_target_metadata_and_plans_build_command() {
        let root = target::repo_root();
        let target = target::load_target_by_id(&root, "xteink-x4").unwrap();
        let native = target.native.as_ref().expect("x4 native metadata");
        assert_eq!(native.package, "squidscript-fw-x4");
        assert_eq!(native.chip, "esp32c3");

        let plan = target::plan_build_command(
            &root,
            &target,
            target::TargetBuildPlanOptions {
                backend: target::TargetBackend::Native,
                stack_usage: false,
                pristine: target::TargetPristine::Auto,
                west_args: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(plan.program, "rustup");
        assert_eq!(plan.cwd, root.join("firmware/native"));
        assert!(plan.args.starts_with(&[
            "run".to_string(),
            "nightly".to_string(),
            "cargo".to_string(),
            "build".to_string(),
            "-p".to_string(),
            "squidscript-fw-x4".to_string(),
        ]));
        assert!(plan.args.contains(&"--features".to_string()));
        let features = plan
            .args
            .windows(2)
            .find_map(|window| (window[0] == "--features").then_some(window[1].as_str()))
            .expect("native feature argument");
        assert!(features.contains("firmware-bin"));
        assert!(features.contains("x4-binbook"));
        assert!(features.contains("native-radio-services"));
        assert!(!plan.env.iter().any(|(key, _)| key == "RUSTUP_TOOLCHAIN"));
        assert!(plan.env.iter().any(|(key, value)| {
            key == "RUSTC" && value.contains("toolchains/nightly") && value.ends_with("/rustc")
        }));
    }

    #[test]
    fn plans_native_flash_command_with_espflash() {
        let root = target::repo_root();
        let target = target::load_target_by_id(&root, "xteink-x4").unwrap();

        let plan = target::plan_flash_command(
            &root,
            &target,
            target::TargetFlashPlanOptions {
                backend: target::TargetBackend::Native,
                west_args: Vec::new(),
            },
        )
        .unwrap();
        assert!(plan.program.ends_with("espflash"));
        assert!(plan.args.starts_with(&[
            "flash".to_string(),
            "--chip".to_string(),
            "esp32c3".to_string(),
            "--non-interactive".to_string(),
        ]));
        assert!(plan.args.iter().any(|arg| {
            arg.ends_with("target/riscv32imc-unknown-none-elf/debug/squidscript-fw-x4")
        }));
        assert!(plan.args.windows(2).any(|args| {
            args[0] == "--partition-table" && args[1].ends_with("targets/partitions/xteink-x4.csv")
        }));
        assert!(plan
            .args
            .windows(2)
            .any(|args| args == ["--target-app-partition", "app0"]));
    }

    #[test]
    fn plans_distinct_native_ota_image_artifact() {
        let root = target::repo_root();
        let target = target::load_target_by_id(&root, "xteink-x4").unwrap();
        let plan = target::plan_native_image_command(&root, &target).unwrap();

        assert!(plan.program.ends_with("espflash"));
        assert_eq!(plan.args.first().map(String::as_str), Some("save-image"));
        assert!(plan
            .args
            .iter()
            .any(|arg| arg.ends_with("squidscript-fw-x4")));
        assert!(plan
            .args
            .iter()
            .any(|arg| arg.ends_with("squidscript-fw-x4-ota.bin")));
        assert!(plan
            .args
            .iter()
            .any(|arg| arg.ends_with("targets/partitions/xteink-x4.csv")));
    }

    #[test]
    fn native_build_plan_exports_configured_ble_connection_watchdog() {
        let root = target::repo_root();
        let target = target::load_target_by_id(&root, "xteink-x4").unwrap();
        let plan = target::plan_build_command(
            &root,
            &target,
            target::TargetBuildPlanOptions {
                backend: target::TargetBackend::Native,
                stack_usage: false,
                pristine: target::TargetPristine::Auto,
                west_args: Vec::new(),
            },
        )
        .unwrap();
        assert!(plan.env.iter().any(|(key, value)| {
            key == "SQUIDSCRIPT_BLE_CONNECTION_WATCHDOG_MS" && value == "30000"
        }));
    }

    #[test]
    fn native_flash_filesystem_build_uses_explicit_riscv_compiler() {
        let root = target::repo_root();
        let target = target::load_target_by_id(&root, "xteink-x4").unwrap();
        std::env::set_var("SQUIDSCRIPT_RISCV_CC", "/toolchain/riscv-gcc");
        let plan = target::plan_build_command(
            &root,
            &target,
            target::TargetBuildPlanOptions {
                backend: target::TargetBackend::Native,
                stack_usage: false,
                pristine: target::TargetPristine::Auto,
                west_args: Vec::new(),
            },
        )
        .unwrap();
        std::env::remove_var("SQUIDSCRIPT_RISCV_CC");

        assert!(plan.env.iter().any(|(key, value)| {
            key == "CC_riscv32imc_unknown_none_elf" && value == "/toolchain/riscv-gcc"
        }));
        assert!(plan.env.iter().any(|(key, value)| {
            key == "CFLAGS_riscv32imc_unknown_none_elf" && value == "-march=rv32imc -mabi=ilp32"
        }));
    }

    #[test]
    fn missing_target_fails_noninteractive() {
        let root = target::repo_root();
        let error = target::resolve_target_arg(&root, None, false).unwrap_err();
        assert!(error.contains("pass --target"));
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
import ui from "lib/ui.squid"
state {}
event.on("app.start") {
  ui.helper()
}
screen("main") {}
"#,
        )
        .unwrap();
        fs::write(
            app_dir.join("lib").join("ui.squid"),
            "export function helper() {}\n",
        )
        .unwrap();
        fs::write(app_dir.join("static").join("index.html"), "<h1>Demo</h1>").unwrap();
        fs::write(app_dir.join("README.md"), "# Package demo\n").unwrap();
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

    #[test]
    fn package_app_dir_rejects_unknown_top_level_declaration() {
        let root = unique_test_dir("squidc-package");
        let app_dir = root.join("app");
        fs::create_dir_all(app_dir.join("lib")).unwrap();
        fs::write(
            app_dir.join("main.squid"),
            r#"app "package-demo"
capability "demo"
state {}
screen("main") {}
"#,
        )
        .unwrap();
        fs::write(
            app_dir.join("lib").join("ui.squid"),
            "function helper() {}\n",
        )
        .unwrap();

        let result = package_app_dir(&app_dir, None, "portable", BuildProfile::Dev);

        assert!(result.is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn app_install_source_with_override_still_resolves_imports_for_declared_app() {
        let root = unique_test_dir("squidc-install-imports");
        let app_dir = root.join("app");
        fs::create_dir_all(app_dir.join("lib")).unwrap();
        fs::write(
            app_dir.join("main.squid"),
            r#"app "declared-app"
import helper from "lib/helper.squid"
state {}
event.on("app.start") {
  helper.ready()
}
"#,
        )
        .unwrap();
        fs::write(
            app_dir.join("lib").join("helper.squid"),
            r#"export function ready() {
  debug.print("ready")
}
"#,
        )
        .unwrap();
        let options = DeviceOptions {
            device: DeviceOnlyOptions { port: None },
            target: None,
            check_target: false,
            profile: ProfileArg::Dev,
        };

        let (bytes, app_id) =
            read_installable_app(&app_dir.join("main.squid"), Some("override-app"), &options)
                .unwrap();

        assert_eq!(app_id, "override-app");
        assert!(!bytes.is_empty());
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
    fn parses_protocol_raw_framed_command() {
        let cli = Cli::try_parse_from([
            "squidc",
            "protocol",
            "raw",
            "hello",
            "--seq",
            "7",
            "--string",
            "target=esp32c3-supermini",
            "--bool",
            "diagnostic=true",
            "--u32",
            "capacity=824",
        ])
        .unwrap();
        let Commands::Protocol {
            command: ProtocolCommands::Raw(args),
        } = cli.command
        else {
            panic!("expected protocol raw");
        };
        assert_eq!(args.opcode, "hello");
        assert_eq!(args.seq, 7);
        assert_eq!(args.string, vec!["target=esp32c3-supermini".to_string()]);
        assert_eq!(args.r#bool, vec!["diagnostic=true".to_string()]);
        assert_eq!(args.u32, vec!["capacity=824".to_string()]);
    }

    #[test]
    fn parses_device_resources_command_and_resource_block() {
        let cli = Cli::try_parse_from([
            "squidc",
            "device",
            "resources",
            "--reset-heap-max",
            "--count",
            "20",
            "--interval-ms",
            "100",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::Resources(args),
        } = cli.command
        else {
            panic!("expected device resources");
        };
        assert!(args.reset_heap_max);
        assert_eq!(args.count, 20);
        assert_eq!(args.interval_ms, 100);
    }

    #[test]
    fn parses_device_storage_format_command() {
        let cli = Cli::try_parse_from(["squidc", "device", "storage-format"]).unwrap();
        let Commands::Device {
            command: DeviceCommands::StorageFormat(_),
        } = cli.command
        else {
            panic!("expected device storage-format");
        };
    }

    #[test]
    fn parses_device_wifi_profile_env_command_without_secret_values() {
        let cli = Cli::try_parse_from([
            "squidc",
            "device",
            "wifi-profile",
            "dev",
            "--ssid-env",
            "SQUID_WIFI_STATION_SSID",
            "--password-env",
            "SQUID_WIFI_STATION_PASSWORD",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::WifiProfile(args),
        } = cli.command
        else {
            panic!("expected device wifi-profile");
        };
        assert_eq!(args.profile, "dev");
        assert_eq!(args.ssid_env, "SQUID_WIFI_STATION_SSID");
        assert_eq!(args.password_env, "SQUID_WIFI_STATION_PASSWORD");
    }

    #[test]
    fn parses_device_runtime_cap_commands() {
        let cli = Cli::try_parse_from([
            "squidc",
            "device",
            "runtime-cap",
            "set",
            "vm_runtime.timer_max",
            "2",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::RuntimeCap(args),
        } = cli.command
        else {
            panic!("expected device runtime-cap");
        };
        match args.command {
            RuntimeCapCommands::Set { key, value } => {
                assert_eq!(key, "vm_runtime.timer_max");
                assert_eq!(value, 2);
            }
            _ => panic!("expected runtime-cap set"),
        }

        let cli = Cli::try_parse_from([
            "squidc",
            "device",
            "runtime-cap",
            "clear",
            "vm_runtime.timer_max",
        ])
        .unwrap();
        let Commands::Device {
            command: DeviceCommands::RuntimeCap(args),
        } = cli.command
        else {
            panic!("expected device runtime-cap");
        };
        match args.command {
            RuntimeCapCommands::Clear { key } => {
                assert_eq!(key.as_deref(), Some("vm_runtime.timer_max"));
            }
            _ => panic!("expected runtime-cap clear"),
        }
    }

    #[test]
    fn parses_fmt_commands() {
        let cli = Cli::try_parse_from(["squidc", "fmt", "--check", "examples"]).unwrap();
        let Commands::Fmt(args) = cli.command else {
            panic!("expected fmt command");
        };
        assert!(args.check);
        assert!(!args.stdin);
        assert_eq!(args.paths, vec![PathBuf::from("examples")]);

        let cli = Cli::try_parse_from(["squidc", "fmt", "--stdin"]).unwrap();
        let Commands::Fmt(args) = cli.command else {
            panic!("expected fmt command");
        };
        assert!(args.stdin);
        assert!(args.paths.is_empty());
    }

    #[test]
    fn parses_app_push_command() {
        let cli = Cli::try_parse_from(["squidc", "app", "push", "SquidScript", "target/app.sqbc"])
            .unwrap();
        let Commands::App {
            command: AppCommands::Push(args),
        } = cli.command
        else {
            panic!("expected app push");
        };
        assert_eq!(args.device, "SquidScript");
        assert_eq!(args.input, PathBuf::from("target/app.sqbc"));
    }

    #[test]
    fn fmt_rewrites_squid_files_in_place() {
        let root = unique_test_dir("squidc-fmt");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("main.squid");
        fs::write(&source, "app   \"demo\"\nstate{count:int=0,}\n").unwrap();

        let result = fmt_command(FmtArgs {
            check: false,
            stdin: false,
            paths: vec![source.clone()],
        })
        .unwrap();

        assert_eq!(result["changed"], 1);
        assert_eq!(
            fs::read_to_string(&source).unwrap(),
            "app \"demo\"\n\nstate {\n  count: int = 0\n}\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fmt_check_reports_unformatted_files_without_rewriting() {
        let root = unique_test_dir("squidc-fmt-check");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("main.squid");
        fs::write(&source, "app   \"demo\"\n").unwrap();

        let error = fmt_command(FmtArgs {
            check: true,
            stdin: false,
            paths: vec![source.clone()],
        })
        .unwrap_err();

        assert!(error.contains("would reformat"));
        assert_eq!(fs::read_to_string(&source).unwrap(), "app   \"demo\"\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_json_parses_process_and_armed_stacks() {
        let lines = vec![
            "active=break-reminder".to_string(),
            "process_stack[0]=main".to_string(),
            "process_stack[1]=reader-clock".to_string(),
            "armed_stack[0]=break-reminder timer.break".to_string(),
        ];

        assert_eq!(
            lifecycle_details(&lines),
            json!({
                "active": "break-reminder",
                "processStack": ["main", "reader-clock"],
                "armedStack": [
                    {"appId": "break-reminder", "event": "timer.break"}
                ],
            })
        );
    }

    #[test]
    fn repl_state_block_uses_previous_state_values() {
        let state_block = "state {\n  count: int = 0\n  label: string = \"old\"\n}\n";
        let state_output =
            "state=636f756e743d320a6c6162656c3d226e6577220a6578697465643d66616c73650a\n";

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
