
use core::fmt;

use crate::{
    bytecode::*, chunk::*, error::*, host::*, limits::*, program::*, reader::*, strings::*,
    value::*, vm::*,
};

#[derive(Default)]
struct Trace {
    events: Vec<String>,
}

impl TraceSink for Trace {
    fn trace(&mut self, message: &str) {
        self.events.push(message.to_string());
    }
}

#[derive(Default)]
struct GpioTrace {
    events: Vec<String>,
    led: bool,
}

impl TraceSink for GpioTrace {
    fn trace(&mut self, message: &str) {
        self.events.push(message.to_string());
    }

    fn hardware_gpio_write(&mut self, name: &str, value: bool) -> Result<(), VmError> {
        self.events.push(format!("write {name}={value}"));
        self.led = value;
        Ok(())
    }

    fn hardware_gpio_toggle(&mut self, name: &str) -> Result<(), VmError> {
        self.events.push(format!("toggle {name}"));
        self.led = !self.led;
        Ok(())
    }

    fn hardware_gpio_read(&mut self, name: &str) -> Result<bool, VmError> {
        self.events.push(format!("read {name}"));
        Ok(self.led)
    }

    fn service_indicator_write(&mut self, value: bool) -> Result<(), VmError> {
        self.events.push(format!("indicator write {value}"));
        self.led = value;
        Ok(())
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        self.events.push("indicator toggle".to_string());
        self.led = !self.led;
        Ok(())
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        self.events.push("indicator read".to_string());
        Ok(self.led)
    }
}

#[derive(Default)]
struct RuntimeTrace {
    events: Vec<String>,
}

impl TraceSink for RuntimeTrace {
    fn trace(&mut self, message: &str) {
        self.events.push(message.to_string());
    }

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
        let mut line = String::new();
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                line.push(' ');
            }
            match value {
                Value::String(_) | Value::RuntimeString(_) => {
                    line.push_str(strings.value_str(*value).unwrap())
                }
                Value::I32(value) => line.push_str(&value.to_string()),
                Value::Bool(value) => line.push_str(&value.to_string()),
                Value::Null => line.push_str("null"),
            }
        }
        self.events.push(format!("debug {line}"));
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        self.events.push(format!("launch {app}"));
        Ok(())
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        self.events.push(format!("arm {app}"));
        Ok(())
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        self.events.push(format!("disarm {app}"));
        Ok(())
    }

    fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
        self.events
            .push(format!("service.timer.every {event} {interval_ms}"));
        Ok(())
    }

    fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
        self.events
            .push(format!("service.timer.after {event} {delay_ms}"));
        Ok(())
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(out, "RAM 292 KiB").map_err(|_| VmError::InvalidOperand)
    }

    fn system_storage_text(&mut self, name: &str, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(out, "{name} 1 MiB").map_err(|_| VmError::InvalidOperand)
    }
}

#[derive(Default)]
struct StateTrace {
    events: Vec<String>,
    saved_state: Vec<u8>,
    reset_count: usize,
}

impl TraceSink for StateTrace {
    fn trace(&mut self, message: &str) {
        self.events.push(message.to_string());
    }

    fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
        if self.saved_state.is_empty() {
            return Ok(None);
        }
        out[..self.saved_state.len()].copy_from_slice(&self.saved_state);
        Ok(Some(self.saved_state.len()))
    }

    fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        self.saved_state = bytes.to_vec();
        Ok(())
    }

    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        self.saved_state.clear();
        self.reset_count += 1;
        Ok(())
    }
}

#[test]
fn runs_headless_counter_fixture_from_real_bytecode() {
    let bytes = fixture_counter_sqbc();
    let program = Program::parse(&bytes).expect("valid fixture");
    let mut vm = Vm::new(program);
    let mut trace = Trace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
    assert_eq!(vm.state_value("count"), Ok(Value::I32(0)));

    vm.dispatch("key.SELECT", &mut trace).unwrap();
    vm.dispatch("key.SELECT", &mut trace).unwrap();
    assert_eq!(vm.state_value("count"), Ok(Value::I32(2)));

    vm.dispatch("key.BACK", &mut trace).unwrap();
    assert!(vm.exited());
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "state.load",
            "state.save",
            "key.SELECT",
            "state.save",
            "key.SELECT",
            "state.save",
            "key.BACK",
            "app.exit",
        ]
    );
}

#[test]
fn rejects_ir_json_sqbc_v1_container() {
    let bytes = b"SQBC\x01\0\x0c\0\x0e\0\0\0\x00\0\0\0{}";
    assert!(matches!(
        Program::parse(bytes),
        Err(VmError::UnsupportedVersion)
    ));
}

#[test]
fn runs_sqbc_v3_emitted_by_squidc_core() {
    let source = include_str!("../fixtures/headless_counter.squid");
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = Trace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    vm.dispatch("key.SELECT", &mut trace).unwrap();

    assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
    assert_eq!(vm.state_value("count"), Ok(Value::I32(1)));
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "state.load",
            "state.save",
            "key.SELECT",
            "state.save",
        ]
    );
}

#[test]
fn state_save_load_and_reset_use_typed_persistent_record() {
    let source = r#"app "state-demo"
state {
  count: int = 0
  enabled: bool = false
  label: string = "cold"
  retryAt: int? = null
}
event.on("app.start") {
  state.load()
}
event.on("key.SELECT") {
  count = count + 7
  enabled = true
  label = label + "-hot"
  retryAt = 42
  state.save()
}
event.on("key.BACK") {
  state.reset()
}
screen("main") {}
"#;
    let bytes = compile_sqbc(source);
    let mut trace = StateTrace::default();
    {
        let program = Program::parse(&bytes).unwrap();
        let mut vm = Vm::new(program);
        vm.dispatch("app.start", &mut trace).unwrap();
        vm.dispatch("key.SELECT", &mut trace).unwrap();
        assert_eq!(vm.state_value("count"), Ok(Value::I32(7)));
        assert!(!trace.saved_state.is_empty());
    }

    let program = Program::parse(&bytes).unwrap();
    let mut restored = Vm::new(program);
    restored.dispatch("app.start", &mut trace).unwrap();
    assert_eq!(restored.state_value("count"), Ok(Value::I32(7)));
    assert_eq!(restored.state_value("enabled"), Ok(Value::Bool(true)));
    assert_eq!(restored.state_value("retryAt"), Ok(Value::I32(42)));
    assert_eq!(
        restored
            .string_resolver()
            .value_str(restored.state_value("label").unwrap()),
        Ok("cold-hot")
    );

    restored.dispatch("key.BACK", &mut trace).unwrap();
    assert_eq!(restored.state_value("count"), Ok(Value::I32(0)));
    assert!(trace.saved_state.is_empty());
    assert_eq!(trace.reset_count, 1);
}

#[test]
fn state_load_rejects_malformed_record_and_type_mismatch() {
    let source = r#"app "state-demo"
state { count: int = 0 }
event.on("app.start") {
  state.load()
}
screen("main") {}
"#;
    let bytes = compile_sqbc(source);
    let mut trace = StateTrace {
        saved_state: b"bad".to_vec(),
        ..StateTrace::default()
    };
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::InvalidStateRecord)
    );

    trace.saved_state = mismatched_count_state_record();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::StateTypeMismatch)
    );
}

#[test]
fn parses_sqbc_v3_header_and_section_records_for_partial_loading() {
    let bytes = fixture_counter_sqbc();
    let header = Program::parse_header(&bytes[..16]).unwrap();
    let header_bytes = &bytes[..header.header_len];

    assert_eq!(header.file_len, bytes.len());
    assert_eq!(header.section_count, 5);
    let first = Program::parse_section_record(header_bytes, 0).unwrap();
    assert_eq!(first.kind, SECTION_STRINGS);
    assert!(first.offset >= header.header_len);
    assert!(first.len > 0);
}

#[test]
fn parses_preload_handler_metadata_from_real_bytecode() {
    let source = r#"app "preload-demo"
@preload
event.on("key.SELECT") {
  debug.print("fast")
}
event.on("key.BACK") {
  app.exit()
}
screen("main") {}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();

    assert_eq!(program.handler_preload("key.SELECT"), Ok(true));
    assert_eq!(program.handler_preload("key.BACK"), Ok(false));
}

#[test]
fn program_index_parses_metadata_without_full_app_read() {
    let bytes = fixture_counter_sqbc();
    let mut reader = CountingReader::new(&bytes);
    let mut scratch = [0u8; MAX_APP_BYTES];

    let index = ProgramIndex::parse_from_reader(&mut reader, &mut scratch).unwrap();

    assert_eq!(index.string(3), Ok("app.start"));
    assert!(reader.reads.iter().all(|(_, len)| *len < bytes.len()));
}

#[test]
fn chunked_vm_reads_handler_code_range_from_reader() {
    let bytes = fixture_counter_sqbc();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::new(&bytes);
    let mut vm = ChunkedVm::new(index);

    vm.dispatch(&mut reader, "app.start").unwrap();

    assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
    assert!(reader
        .reads
        .iter()
        .any(|(_, len)| *len > 0 && *len <= MAX_CODE_CHUNK_BYTES));
}

#[test]
fn chunked_vm_rejects_oversized_handler_chunk() {
    let strings = encode_strings(&["oversized", "app.start"]);
    let state = vec![0, 0];
    let functions = vec![0, 0];
    let mut handlers = Vec::new();
    push_u16(&mut handlers, 1);
    push_u16(&mut handlers, 1);
    push_u16(&mut handlers, 0);
    push_u32(&mut handlers, 0);
    push_u32(&mut handlers, (MAX_CODE_CHUNK_BYTES + 1) as u32);
    let code = vec![OP_HALT; MAX_CODE_CHUNK_BYTES + 1];
    let bytes = encode_container(vec![
        (SECTION_STRINGS, strings),
        (SECTION_STATE, state),
        (SECTION_FUNCTIONS, functions),
        (SECTION_HANDLERS, handlers),
        (SECTION_CODE, code),
    ]);
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::new(&bytes);
    let mut vm = ChunkedVm::new(index);

    assert_eq!(
        vm.dispatch(&mut reader, "app.start"),
        Err(VmError::ChunkTooLarge)
    );
}

#[test]
fn chunked_vm_dispatches_functions_and_screens_on_demand() {
    let source = r#"app "chunk-demo"
state { count: int = 0 }
function bump() {
  count = count + 1
}
event.on("app.start") {
  bump()
  screen.open("main")
}
screen("main") {
  bump()
}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::new(&bytes);
    let mut vm = ChunkedVm::new(index);

    vm.dispatch(&mut reader, "app.start").unwrap();

    assert_eq!(vm.state_value("count"), Ok(Value::I32(2)));
    assert_eq!(reader.events, vec!["app.start"]);
}

#[test]
fn chunk_cache_prefers_evicting_cold_unhinted_chunks() {
    let hot = ChunkRef {
        app: 1,
        kind: ChunkKind::Handler,
        index: 0,
    };
    let cold = ChunkRef {
        app: 1,
        kind: ChunkKind::Handler,
        index: 1,
    };
    let incoming = ChunkRef {
        app: 1,
        kind: ChunkKind::Handler,
        index: 2,
    };
    let mut cache = ChunkCache::<2>::new();

    cache.insert(hot, true).unwrap();
    cache.insert(cold, false).unwrap();
    cache.begin_execute(hot).unwrap();
    cache.insert(incoming, false).unwrap();

    assert!(cache.contains(hot));
    assert!(cache.contains(incoming));
    assert!(!cache.contains(cold));
}

#[test]
fn chunk_cache_drops_all_chunks_for_replaced_app() {
    let app_one = ChunkRef {
        app: 1,
        kind: ChunkKind::Handler,
        index: 0,
    };
    let app_two = ChunkRef {
        app: 2,
        kind: ChunkKind::Handler,
        index: 0,
    };
    let mut cache = ChunkCache::<2>::new();

    cache.insert(app_one, true).unwrap();
    cache.insert(app_two, false).unwrap();
    cache.drop_app(1);

    assert!(!cache.contains(app_one));
    assert!(cache.contains(app_two));
}

#[test]
fn runs_service_indicator_builtins_from_real_bytecode() {
    let source = r#"app "gpio" target "esp32c3-super-mini"
state { led: bool = false }
event.on("app.start") {
  service.indicator.write(true)
  led = service.indicator.read()
  service.indicator.toggle()
  led = service.indicator.read()
}
screen("main") {}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = GpioTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(vm.state_value("led"), Ok(Value::Bool(false)));
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "indicator write true",
            "indicator read",
            "indicator toggle",
            "indicator read",
        ]
    );
}

struct CountingReader<'a> {
    bytes: &'a [u8],
    reads: Vec<(usize, usize)>,
    events: Vec<String>,
    saved_state: Vec<u8>,
}

impl<'a> CountingReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            reads: Vec::new(),
            events: Vec::new(),
            saved_state: Vec::new(),
        }
    }
}

impl SqbcReader for CountingReader<'_> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let end = offset
            .checked_add(out.len())
            .ok_or(VmError::InvalidSection)?;
        let bytes = self.bytes.get(offset..end).ok_or(VmError::InvalidSection)?;
        out.copy_from_slice(bytes);
        self.reads.push((offset, out.len()));
        Ok(())
    }
}

impl TraceSink for CountingReader<'_> {
    fn trace(&mut self, message: &str) {
        self.events.push(message.to_string());
    }

    fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
        if self.saved_state.is_empty() {
            return Ok(None);
        }
        out[..self.saved_state.len()].copy_from_slice(&self.saved_state);
        Ok(Some(self.saved_state.len()))
    }

    fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        self.saved_state = bytes.to_vec();
        Ok(())
    }

    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        self.saved_state.clear();
        Ok(())
    }
}

#[test]
fn runs_app_launch_timer_service_and_timer_handler_from_real_bytecode() {
    let source = r#"app "timer-demo"
state { count: int = 0 }
event.on("app.start") {
  app.launch("timer-armed-app")
  service.timer.every("timer.debug", 1000)
}
event.on("timer.debug") {
  debug.print("timer", count)
}
screen("main") {}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    vm.dispatch("timer.debug", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "launch timer-armed-app",
            "service.timer.every timer.debug 1000",
            "timer.debug",
            "debug timer 0",
        ]
    );
}

#[test]
fn runs_generic_event_lifecycle_builtins_from_real_bytecode() {
    let source = r#"app "event-demo"
state { count: int = 0 }
event.on("app.start") {
  app.arm("break-reminder")
  app.launch("reader")
  service.timer.every("timer.clock", 60000)
}
event.on("timer.clock") {
  count = count + 1
  app.disarm("break-reminder")
}
screen("main") {}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    vm.dispatch("timer.clock", &mut trace).unwrap();

    assert_eq!(vm.state_value("count"), Ok(Value::I32(1)));
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "arm break-reminder",
            "launch reader",
            "service.timer.every timer.clock 60000",
            "timer.clock",
            "disarm break-reminder",
        ]
    );
}

#[test]
fn runs_system_resource_string_builtins_from_real_bytecode() {
    let source = r#"app "resources"
event.on("app.start") {
  debug.print(system.memory())
  debug.print(system.storage("apps"))
}
screen("main") {}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec!["app.start", "debug RAM 292 KiB", "debug apps 1 MiB"]
    );
}

#[test]
fn state_reset_restores_typed_defaults() {
    let source = r#"app "reset-demo"
state {
  count: int = 4
}
event.on("app.start") {
  count = count + 1
  state.reset()
}
screen("main") {}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = Trace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(vm.state_value("count"), Ok(Value::I32(4)));
    assert_eq!(trace.events, vec!["app.start", "state.reset"]);
}

#[test]
fn rejects_mixed_arithmetic_without_null_or_bool_coercion() {
    let source = r#"app "bad-math"
state {
  count: int = 0
  label: string = "count"
}
event.on("app.start") {
  count = label + 1
}
screen("main") {}
"#;
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = Trace::default();

    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::InvalidOperand)
    );
}

fn fixture_counter_sqbc() -> Vec<u8> {
    let strings = encode_strings(&[
        "headless-counter",
        "count",
        "started",
        "app.start",
        "key.SELECT",
        "key.BACK",
    ]);
    let mut state = Vec::new();
    push_u16(&mut state, 2);
    push_u16(&mut state, 1);
    state.push(STATE_TYPE_INT);
    state.push(0);
    state.push(VALUE_I32);
    push_i32(&mut state, 0);
    push_u16(&mut state, 2);
    state.push(STATE_TYPE_INT);
    state.push(0);
    state.push(VALUE_I32);
    push_i32(&mut state, 0);

    let functions = vec![0, 0];
    let mut code = Vec::new();
    let on_start = code.len();
    code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_STATE_LOAD, OP_PUSH_INT]);
    push_i32(&mut code, 1);
    code.push(OP_SET_STATE);
    push_u16(&mut code, 1);
    code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_STATE_SAVE, OP_HALT]);
    let select = code.len();
    code.push(OP_GET_STATE);
    push_u16(&mut code, 0);
    code.push(OP_PUSH_INT);
    push_i32(&mut code, 1);
    code.push(OP_ADD);
    code.push(OP_SET_STATE);
    push_u16(&mut code, 0);
    code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_STATE_SAVE, OP_HALT]);
    let back = code.len();
    code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_APP_EXIT, OP_HALT]);

    let mut handlers = Vec::new();
    push_u16(&mut handlers, 3);
    push_u16(&mut handlers, 3);
    push_u16(&mut handlers, 0);
    push_u32(&mut handlers, on_start as u32);
    push_u32(&mut handlers, (select - on_start) as u32);
    push_u16(&mut handlers, 4);
    push_u16(&mut handlers, 0);
    push_u32(&mut handlers, select as u32);
    push_u32(&mut handlers, (back - select) as u32);
    push_u16(&mut handlers, 5);
    push_u16(&mut handlers, 0);
    push_u32(&mut handlers, back as u32);
    push_u32(&mut handlers, (code.len() - back) as u32);

    encode_container(vec![
        (SECTION_STRINGS, strings),
        (SECTION_STATE, state),
        (SECTION_FUNCTIONS, functions),
        (SECTION_HANDLERS, handlers),
        (SECTION_CODE, code),
    ])
}

fn compile_sqbc(source: &str) -> Vec<u8> {
    let compiled = squidc_core::compile(squidc_core::CompileRequest {
        source: source.to_string(),
        target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap()
}

fn mismatched_count_state_record() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(STATE_RECORD_MAGIC);
    out.push(STATE_RECORD_VERSION);
    out.push(1);
    out.push(5);
    out.extend_from_slice(b"count");
    out.push(STATE_TYPE_BOOL);
    out.push(0);
    out.push(VALUE_BOOL);
    out.push(1);
    out
}

fn encode_strings(values: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    push_u16(&mut out, values.len() as u16);
    for value in values {
        push_u16(&mut out, value.len() as u16);
        out.extend_from_slice(value.as_bytes());
    }
    out
}

fn encode_container(sections: Vec<(u16, Vec<u8>)>) -> Vec<u8> {
    let header_len = 16 + sections.len() * 12;
    let file_len = header_len + sections.iter().map(|(_, data)| data.len()).sum::<usize>();
    let mut out = Vec::new();
    out.extend_from_slice(b"SQBC");
    push_u16(&mut out, 3);
    push_u16(&mut out, header_len as u16);
    push_u32(&mut out, file_len as u32);
    push_u32(&mut out, sections.len() as u32);
    let mut offset = header_len;
    for (kind, data) in &sections {
        push_u16(&mut out, *kind);
        push_u16(&mut out, 0);
        push_u32(&mut out, offset as u32);
        push_u32(&mut out, data.len() as u32);
        offset += data.len();
    }
    for (_, data) in sections {
        out.extend_from_slice(&data);
    }
    out
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
