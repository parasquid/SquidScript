mod app_id;
mod compile;
mod serial;

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use app_id::{generated_app_id, source_app_id, source_for_compile};
use compile::{compile_source_to_sqbc, compile_target_id};
use serial::{detect_port, OutputTail, SerialDevice};
use squidc_core::BuildProfile;

fn main() {
    if let Err(error) = run() {
        eprintln!("squidc: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("build") => build(&args[1..]),
        Some("repl") => repl(&args[1..]),
        Some("run") => run_app_source(&args[1..]),
        Some("install") => install_app(&args[1..]),
        Some("start") => start_app(&args[1..]),
        Some("key") => key(&args[1..]),
        Some("reset") => device_line_command(&args[1..], "RESET"),
        Some("send") => send(&args[1..]),
        Some("monitor") => monitor(&args[1..]),
        Some("output") => device_block_command(&args[1..], "OUTPUT.GET"),
        Some("state") => device_block_command(&args[1..], "STATE.GET"),
        Some("drawlog") => device_block_command(&args[1..], "DRAWLOG.GET"),
        _ => Err(
            "usage: squidc build|run|install|start|key|reset|send|monitor|output|state|drawlog|repl ..."
                .to_string(),
        ),
    }
}

fn build(args: &[String]) -> Result<(), String> {
    let mut input = None;
    let mut target = None;
    let mut check_target = false;
    let mut out = None;
    let mut profile = BuildProfile::Dev;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                index += 1;
                target = args.get(index).cloned();
            }
            "--check-target" => {
                check_target = true;
            }
            "--out" => {
                index += 1;
                out = args.get(index).map(PathBuf::from);
            }
            "--profile" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "missing --profile value".to_string())?;
                profile = BuildProfile::parse(value)
                    .ok_or_else(|| format!("unknown profile {value}; expected dev or release"))?;
            }
            value if input.is_none() => input = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument {value}")),
        }
        index += 1;
    }

    let input = input.ok_or_else(|| "missing input .squid path".to_string())?;
    let target = compile_target_id(target.as_deref(), check_target)?;
    let out = out.ok_or_else(|| "missing --out".to_string())?;
    let bytes = compile_source_to_sqbc(
        &fs::read_to_string(&input)
            .map_err(|error| format!("failed to read {}: {error}", input.display()))?,
        &target,
        profile,
    )?;
    fs::write(&out, bytes)
        .map_err(|error| format!("failed to write {}: {error}", out.display()))?;
    Ok(())
}

fn run_app_source(args: &[String]) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut input = None;
    parse_device_args(args, &mut options, &mut input, false)?;
    let input = input.ok_or_else(|| "missing input .squid path".to_string())?;
    let source = fs::read_to_string(&input)
        .map_err(|error| format!("failed to read {}: {error}", input.display()))?;
    let app_id = source_app_id(&source).unwrap_or_else(|| generated_app_id(&input, &source));
    let source = source_for_compile(&source, &app_id);
    let target = compile_target_id(options.target.as_deref(), options.check_target)?;
    let sqbc = compile_source_to_sqbc(&source, &target, options.profile)?;
    let port = options.resolve_port()?;
    let mut device = SerialDevice::open(&port)?;
    device.install_app("main", &sqbc)?;
    device.run_app("main")?;
    Ok(())
}

fn install_app(args: &[String]) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut input = None;
    parse_device_args(args, &mut options, &mut input, true)?;
    let input = input.ok_or_else(|| "missing input .squid or .sqbc path".to_string())?;
    let bytes = if input.extension().and_then(|value| value.to_str()) == Some("sqbc") {
        fs::read(&input).map_err(|error| format!("failed to read {}: {error}", input.display()))?
    } else {
        let source = fs::read_to_string(&input)
            .map_err(|error| format!("failed to read {}: {error}", input.display()))?;
        let app_id = options
            .app_id_override
            .clone()
            .or_else(|| source_app_id(&source))
            .unwrap_or_else(|| generated_app_id(&input, &source));
        let source = source_for_compile(&source, &app_id);
        let target = compile_target_id(options.target.as_deref(), options.check_target)?;
        compile_source_to_sqbc(&source, &target, options.profile)?
    };
    let app_id = if let Some(app_id) = options.app_id_override.as_ref() {
        app_id.clone()
    } else if input.extension().and_then(|value| value.to_str()) == Some("sqbc") {
        squidc_core::sqbc_v2::read_app_id(&bytes)
            .map_err(|error| error.message)?
            .ok_or_else(|| "SQBC has no app id metadata; pass --as <appId>".to_string())?
    } else {
        let source = fs::read_to_string(&input)
            .map_err(|error| format!("failed to read {}: {error}", input.display()))?;
        source_app_id(&source).unwrap_or_else(|| generated_app_id(&input, &source))
    };
    let port = options.resolve_port()?;
    let mut device = SerialDevice::open(&port)?;
    device.install_app(&app_id, &bytes)?;
    Ok(())
}

fn start_app(args: &[String]) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut app_id = None;
    parse_device_args(args, &mut options, &mut app_id, false)?;
    let app_id = app_id.ok_or_else(|| "missing app id".to_string())?;
    let port = options.resolve_port()?;
    let mut device = SerialDevice::open(&port)?;
    device.run_app(&app_id.to_string_lossy())?;
    Ok(())
}

fn key(args: &[String]) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut key = None;
    parse_device_args(args, &mut options, &mut key, false)?;
    let key = key.ok_or_else(|| "missing key".to_string())?;
    let port = options.resolve_port()?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_line(&format!("KEY {}", key.display()))?;
    print!("{response}");
    if !response.contains("OK key") {
        return Err(response.trim().to_string());
    }
    Ok(())
}

fn send(args: &[String]) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut line = None;
    parse_device_args(args, &mut options, &mut line, false)?;
    let line = line.ok_or_else(|| "missing line".to_string())?;
    device_line_command_with_options(options, &line.to_string_lossy())
}

fn monitor(args: &[String]) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut raw = false;
    let mut poll_ms = 500u64;
    let mut max_lines = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                index += 1;
                options.port = Some(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "missing --port value".to_string())?,
                );
            }
            "--raw" => raw = true,
            "--poll-ms" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "missing --poll-ms value".to_string())?;
                poll_ms = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --poll-ms value {value}"))?;
            }
            "--max-lines" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "missing --max-lines value".to_string())?;
                max_lines = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| format!("invalid --max-lines value {value}"))?,
                );
            }
            value => return Err(format!("unexpected argument {value}")),
        }
        index += 1;
    }

    let port = options.resolve_port()?;
    let mut device = SerialDevice::open(&port)?;
    eprintln!("squidc: monitoring {port}; press Ctrl+C to stop");
    if raw {
        monitor_raw(&mut device, max_lines)
    } else {
        monitor_output(&mut device, Duration::from_millis(poll_ms), max_lines)
    }
}

fn monitor_raw(device: &mut SerialDevice, max_lines: Option<usize>) -> Result<(), String> {
    let mut printed = 0usize;
    loop {
        let chunk = device.read_available_text()?;
        if !chunk.is_empty() {
            printed += chunk.lines().count().max(1);
            print!("{chunk}");
            io::stdout()
                .flush()
                .map_err(|error| format!("stdout flush failed: {error}"))?;
            if max_lines.is_some_and(|max| printed >= max) {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn monitor_output(
    device: &mut SerialDevice,
    poll_interval: Duration,
    max_lines: Option<usize>,
) -> Result<(), String> {
    let mut tail = OutputTail::new();
    let mut printed = 0usize;
    loop {
        let response = device.send_line("OUTPUT.GET")?;
        for line in tail.next_lines(&response) {
            println!("{line}");
            printed += 1;
            if max_lines.is_some_and(|max| printed >= max) {
                return Ok(());
            }
        }
        io::stdout()
            .flush()
            .map_err(|error| format!("stdout flush failed: {error}"))?;
        std::thread::sleep(poll_interval);
    }
}

fn device_line_command(args: &[String], command: &str) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut unexpected = None;
    parse_device_args(args, &mut options, &mut unexpected, false)?;
    if let Some(value) = unexpected {
        return Err(format!("unexpected argument {}", value.display()));
    }
    device_line_command_with_options(options, command)
}

fn device_line_command_with_options(options: DeviceOptions, command: &str) -> Result<(), String> {
    let port = options.resolve_port()?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_line(command)?;
    print!("{response}");
    Ok(())
}

fn device_block_command(args: &[String], command: &str) -> Result<(), String> {
    let mut options = DeviceOptions::default();
    let mut unexpected = None;
    parse_device_args(args, &mut options, &mut unexpected, false)?;
    if let Some(value) = unexpected {
        return Err(format!("unexpected argument {}", value.display()));
    }
    let port = options.resolve_port()?;
    let mut device = SerialDevice::open(&port)?;
    let response = device.send_line(command)?;
    print!("{response}");
    Ok(())
}

fn repl(args: &[String]) -> Result<(), String> {
    let mut target = None;
    let mut check_target = false;
    let mut port = env::var("ESPFLASH_PORT").ok();
    let mut script = None;
    let mut input_file = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                index += 1;
                target = args.get(index).cloned();
            }
            "--check-target" => {
                check_target = true;
            }
            "--port" => {
                index += 1;
                port = Some(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "missing --port value".to_string())?,
                );
            }
            "--script" => {
                index += 1;
                script = args.get(index).map(PathBuf::from);
            }
            value if input_file.is_none() => input_file = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument {value}")),
        }
        index += 1;
    }
    let target = compile_target_id(target.as_deref(), check_target)?;
    let port = match port {
        Some(port) => port,
        None => detect_port()?,
    };
    let script = script.ok_or_else(|| "missing --script".to_string())?;
    let script_text = fs::read_to_string(&script)
        .map_err(|error| format!("failed to read {}: {error}", script.display()))?;
    if script.extension().and_then(|value| value.to_str()) == Some("squid") {
        let mut session = ReplSession::new(target, port, script_text);
        return session.reload_base_source();
    }
    let base_source = match input_file {
        Some(path) => fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?,
        None => String::new(),
    };
    ReplSession::new(target, port, base_source).run_script(&script_text)
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
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReplMode {
    Event,
    Render,
}

impl ReplSession {
    fn new(target: String, port: String, base_source: String) -> Self {
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
                self.serial_text(&["send", "RESET"])?;
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
        let source = self.generated_source();
        let sqbc = compile_source_to_sqbc(&source, &self.target, self.profile)?;
        let sqbc_path = self.temp_dir.join("repl.sqbc");
        fs::write(&sqbc_path, sqbc)
            .map_err(|error| format!("failed to write {}: {error}", sqbc_path.display()))?;

        let state_before = self.serial_text_allow_fail(&["state"]).unwrap_or_default();
        let state_path = self.temp_dir.join("state.txt");
        fs::write(&state_path, state_payload(&state_before))
            .map_err(|error| format!("failed to write {}: {error}", state_path.display()))?;

        self.serial_text(&["install", path_str(&sqbc_path)?])?;
        self.serial_text(&["load"])?;
        if fs::metadata(&state_path).map(|m| m.len()).unwrap_or(0) > 0 {
            self.serial_text(&["state-import", path_str(&state_path)?])?;
        }

        match self.mode {
            ReplMode::Event => {
                self.serial_text(&["run", "repl"])?;
                self.last_output = self.serial_text(&["output"])?;
            }
            ReplMode::Render => {
                self.serial_text(&["run", "app.start"])?;
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

        self.serial_text(&["install", path_str(&sqbc_path)?])?;
        self.serial_text(&["load"])?;
        self.serial_text(&["run", "app.start"])?;
        self.last_output = self.serial_text(&["output"])?;
        self.last_drawlog = self.serial_text(&["drawlog"])?;
        self.last_state = self.serial_text(&["state"])?;
        Ok(())
    }

    fn generated_source(&self) -> String {
        let mut source = format!("app \"repl-session\"\n\n{}", self.state_block);
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
            ["install", path] => {
                let bytes =
                    fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
                device.install_legacy(&bytes)?;
                String::new()
            }
            ["load"] => device.send_line("LOAD")?,
            ["run", event] => device.run_event(event)?,
            ["state-import", path] => {
                let bytes =
                    fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
                device.import_state(&bytes)?;
                String::new()
            }
            ["state"] => device.send_line("STATE.GET")?,
            ["output"] => device.send_line("OUTPUT.GET")?,
            ["drawlog"] => device.send_line("DRAWLOG.GET")?,
            ["key", key] => device.send_line(&format!("KEY {key}"))?,
            ["send", line] => device.send_line(line)?,
            _ => return Err(format!("unsupported repl serial command: {args:?}")),
        };
        print!("{output}");
        Ok(output)
    }

    fn serial_text_allow_fail(&self, args: &[&str]) -> Result<String, String> {
        self.serial_text(args).or_else(|_| Ok(String::new()))
    }
}

struct DeviceOptions {
    port: Option<String>,
    target: Option<String>,
    check_target: bool,
    profile: BuildProfile,
    app_id_override: Option<String>,
}

impl Default for DeviceOptions {
    fn default() -> Self {
        Self {
            port: None,
            target: None,
            check_target: false,
            profile: BuildProfile::Dev,
            app_id_override: None,
        }
    }
}

impl DeviceOptions {
    fn resolve_port(&self) -> Result<String, String> {
        match &self.port {
            Some(port) => Ok(port.clone()),
            None => detect_port(),
        }
    }
}

fn parse_device_args(
    args: &[String],
    options: &mut DeviceOptions,
    positional: &mut Option<PathBuf>,
    allow_as: bool,
) -> Result<(), String> {
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                index += 1;
                options.port = Some(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "missing --port value".to_string())?,
                );
            }
            "--target" => {
                index += 1;
                options.target = args.get(index).cloned();
            }
            "--check-target" => {
                options.check_target = true;
            }
            "--profile" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "missing --profile value".to_string())?;
                options.profile = BuildProfile::parse(value)
                    .ok_or_else(|| format!("unknown profile {value}; expected dev or release"))?;
            }
            "--as" if allow_as => {
                index += 1;
                options.app_id_override = Some(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "missing --as value".to_string())?,
                );
            }
            value if positional.is_none() => *positional = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument {value}")),
        }
        index += 1;
    }
    Ok(())
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

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not UTF-8: {}", path.display()))
}
