use core::{
    fmt::{self, Write},
    mem::MaybeUninit,
};

use squidvm_core::{
    error::VmError,
    host::TraceSink,
    limits::{MAX_APP_BYTES, MAX_SAVED_STATE_BYTES},
    reader::{SliceSqbcReader, SqbcReader},
    strings::StringResolver,
    value::Value,
    vm::ChunkedVm,
};

pub const MAX_TEMP_SQBC_BYTES: usize = MAX_APP_BYTES;
const MAX_LINE_COUNT: usize = 8;
const MAX_LINE_BYTES: usize = 64;
const MAX_APP_ID_BYTES: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRuntimeError {
    TooLarge,
    InvalidOffset,
    IncompleteTempRun,
    Vm(VmError),
}

impl From<VmError> for NativeRuntimeError {
    fn from(value: VmError) -> Self {
        Self::Vm(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceMetric {
    pub key: &'static str,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceMetrics {
    metrics: [ResourceMetric; 6],
    len: usize,
}

impl ResourceMetrics {
    pub fn iter(&self) -> impl Clone + Iterator<Item = ResourceMetric> + '_ {
        self.metrics[..self.len].iter().copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineView<'a> {
    lines: [&'a str; MAX_LINE_COUNT],
    len: usize,
}

impl<'a> LineView<'a> {
    pub fn iter(&self) -> impl Clone + Iterator<Item = &'a str> + '_ {
        self.lines[..self.len].iter().copied()
    }

    pub fn as_slice(&self) -> &[&'a str] {
        &self.lines[..self.len]
    }
}

pub struct NativeRuntime {
    host: RuntimeHost,
    vm: MaybeUninit<ChunkedVm>,
    vm_active: bool,
    scratch: [u8; MAX_TEMP_SQBC_BYTES],
}

impl NativeRuntime {
    pub const fn new() -> Self {
        Self {
            host: RuntimeHost::new(),
            vm: MaybeUninit::uninit(),
            vm_active: false,
            scratch: [0; MAX_TEMP_SQBC_BYTES],
        }
    }

    pub fn reset(&mut self) {
        self.host.clear_all();
        self.vm_active = false;
    }

    pub fn begin_temp_run(
        &mut self,
        app_id: &str,
        total_len: usize,
    ) -> Result<(), NativeRuntimeError> {
        if total_len == 0 || total_len > self.host.temp_sqbc.len() {
            return Err(NativeRuntimeError::TooLarge);
        }
        self.host.begin_temp_run(app_id, total_len)
    }

    pub fn write_temp_run_chunk(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        self.host.write_temp_run_chunk(offset, bytes)
    }

    pub fn commit_temp_run(&mut self) -> Result<(), NativeRuntimeError> {
        if self.host.temp_received != self.host.temp_expected_len {
            return Err(NativeRuntimeError::IncompleteTempRun);
        }
        self.host.clear_diagnostics();
        self.host.saved_state_len = None;
        let mut reader = SliceSqbcReader::new(self.host.temp_bytes());
        unsafe {
            ChunkedVm::init_in_place_from_reader(
                self.vm.as_mut_ptr(),
                &mut reader,
                &mut self.scratch,
            )?;
        }
        self.vm_active = true;
        self.dispatch_app_start()
    }

    pub fn output_lines(&self) -> LineView<'_> {
        self.host.output.view()
    }

    pub fn trace_lines(&self) -> LineView<'_> {
        self.host.trace.view()
    }

    pub fn lifecycle_lines(&self) -> LineView<'_> {
        if self.host.lifecycle.is_empty() {
            inactive_lifecycle_view()
        } else {
            self.host.lifecycle.view()
        }
    }

    pub fn state_bytes(&self) -> &[u8] {
        self.host.state_bytes()
    }

    pub fn active_app(&self) -> Option<&str> {
        self.vm_active.then(|| self.host.app_id.as_str())
    }

    pub fn resource_metrics(&self) -> ResourceMetrics {
        let metrics = [
            ResourceMetric {
                key: "ram_total_bytes",
                value: 400 * 1024,
            },
            ResourceMetric {
                key: "runtime_static_bytes",
                value: core::mem::size_of::<Self>() as u64,
            },
            ResourceMetric {
                key: "vm_sqbc_chunk_bytes",
                value: squidvm_core::limits::MAX_CODE_CHUNK_BYTES as u64,
            },
            ResourceMetric {
                key: "runtime_current_app_present",
                value: u64::from(self.vm_active),
            },
            ResourceMetric {
                key: "last_sqbc_reads",
                value: self.host.sqbc_reads as u64,
            },
            ResourceMetric {
                key: "last_sqbc_bytes",
                value: self.host.sqbc_bytes as u64,
            },
        ];
        ResourceMetrics {
            metrics,
            len: metrics.len(),
        }
    }

    fn dispatch_app_start(&mut self) -> Result<(), NativeRuntimeError> {
        let vm = unsafe { self.vm.assume_init_mut() };
        vm.dispatch(&mut self.host, "app.start")?;
        Ok(())
    }
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

struct RuntimeHost {
    temp_sqbc: [u8; MAX_TEMP_SQBC_BYTES],
    temp_expected_len: usize,
    temp_received: usize,
    app_id: FixedText<MAX_APP_ID_BYTES>,
    saved_state: [u8; MAX_SAVED_STATE_BYTES],
    saved_state_len: Option<usize>,
    output: LineStore,
    trace: LineStore,
    lifecycle: LineStore,
    sqbc_reads: usize,
    sqbc_bytes: usize,
}

impl RuntimeHost {
    const fn new() -> Self {
        Self {
            temp_sqbc: [0; MAX_TEMP_SQBC_BYTES],
            temp_expected_len: 0,
            temp_received: 0,
            app_id: FixedText::new(),
            saved_state: [0; MAX_SAVED_STATE_BYTES],
            saved_state_len: None,
            output: LineStore::new(),
            trace: LineStore::new(),
            lifecycle: LineStore::new(),
            sqbc_reads: 0,
            sqbc_bytes: 0,
        }
    }

    fn clear_all(&mut self) {
        self.temp_expected_len = 0;
        self.temp_received = 0;
        self.app_id.clear();
        self.saved_state_len = None;
        self.clear_diagnostics();
        self.set_inactive_lifecycle();
    }

    fn clear_diagnostics(&mut self) {
        self.output.clear();
        self.trace.clear();
        self.sqbc_reads = 0;
        self.sqbc_bytes = 0;
    }

    fn begin_temp_run(&mut self, app_id: &str, total_len: usize) -> Result<(), NativeRuntimeError> {
        self.temp_expected_len = total_len;
        self.temp_received = 0;
        self.app_id.set(app_id)?;
        self.set_active_lifecycle();
        Ok(())
    }

    fn write_temp_run_chunk(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        let end = offset
            .checked_add(bytes.len())
            .ok_or(NativeRuntimeError::InvalidOffset)?;
        if end > self.temp_expected_len || end > self.temp_sqbc.len() {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.temp_sqbc[offset..end].copy_from_slice(bytes);
        self.temp_received = self.temp_received.max(end);
        Ok(())
    }

    fn temp_bytes(&self) -> &[u8] {
        &self.temp_sqbc[..self.temp_expected_len]
    }

    fn state_bytes(&self) -> &[u8] {
        self.saved_state_len
            .map(|len| &self.saved_state[..len])
            .unwrap_or(&[])
    }

    fn set_inactive_lifecycle(&mut self) {
        self.lifecycle.clear();
        self.lifecycle.push("active=");
        self.lifecycle.push("armed_stack=");
    }

    fn set_active_lifecycle(&mut self) {
        let app_id = self.app_id;
        self.lifecycle.clear();
        self.lifecycle.push_fmt(|line| {
            write!(line, "active={}", app_id.as_str())?;
            Ok(())
        });
        self.lifecycle.push("armed_stack=");
    }
}

impl SqbcReader for RuntimeHost {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let end = offset.checked_add(out.len()).ok_or(VmError::ReadFailed)?;
        let bytes = self.temp_sqbc.get(offset..end).ok_or(VmError::ReadFailed)?;
        out.copy_from_slice(bytes);
        self.sqbc_reads += 1;
        self.sqbc_bytes += out.len();
        Ok(())
    }
}

impl TraceSink for RuntimeHost {
    fn trace(&mut self, message: &str) {
        self.trace.push(message);
    }

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
        self.output.push_fmt(|line| {
            for (index, value) in values.iter().copied().enumerate() {
                if index > 0 {
                    line.write_str(" ")?;
                }
                write_value(line, strings, value)?;
            }
            Ok(())
        });
    }

    fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
        let Some(len) = self.saved_state_len else {
            return Ok(None);
        };
        if len > out.len() {
            return Err(VmError::StateTooLarge);
        }
        out[..len].copy_from_slice(&self.saved_state[..len]);
        Ok(Some(len))
    }

    fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        if bytes.len() > self.saved_state.len() {
            return Err(VmError::StateTooLarge);
        }
        self.saved_state[..bytes.len()].copy_from_slice(bytes);
        self.saved_state_len = Some(bytes.len());
        Ok(())
    }

    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        self.saved_state_len = None;
        Ok(())
    }
}

fn write_value(
    out: &mut impl fmt::Write,
    strings: &StringResolver<'_>,
    value: Value,
) -> fmt::Result {
    match value {
        Value::Null => out.write_str("null"),
        Value::Bool(true) => out.write_str("true"),
        Value::Bool(false) => out.write_str("false"),
        Value::I32(value) => write!(out, "{value}"),
        Value::String(_) => out.write_str(strings.value_str(value).unwrap_or("<string>")),
        Value::Record(_) => out.write_str("<record>"),
        Value::List(_) => out.write_str("<list>"),
        Value::Handle(_) => out.write_str("<handle>"),
    }
}

struct LineStore {
    lines: [FixedText<MAX_LINE_BYTES>; MAX_LINE_COUNT],
    len: usize,
}

impl LineStore {
    const fn new() -> Self {
        Self {
            lines: [FixedText::new(); MAX_LINE_COUNT],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
        for line in &mut self.lines {
            line.clear();
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn push(&mut self, value: &str) {
        if self.len == self.lines.len() {
            self.lines.rotate_left(1);
            self.len -= 1;
        }
        let _ = self.lines[self.len].set(value);
        self.len += 1;
    }

    fn push_fmt(&mut self, write: impl FnOnce(&mut FixedText<MAX_LINE_BYTES>) -> fmt::Result) {
        if self.len == self.lines.len() {
            self.lines.rotate_left(1);
            self.len -= 1;
        }
        self.lines[self.len].clear();
        let _ = write(&mut self.lines[self.len]);
        self.len += 1;
    }

    fn view(&self) -> LineView<'_> {
        let mut lines = [""; MAX_LINE_COUNT];
        let mut index = 0;
        while index < self.len {
            lines[index] = self.lines[index].as_str();
            index += 1;
        }
        LineView {
            lines,
            len: self.len,
        }
    }
}

fn inactive_lifecycle_view<'a>() -> LineView<'a> {
    let mut lines = [""; MAX_LINE_COUNT];
    lines[0] = "active=";
    lines[1] = "armed_stack=";
    LineView { lines, len: 2 }
}

#[derive(Clone, Copy)]
struct FixedText<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> FixedText<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn set(&mut self, value: &str) -> Result<(), NativeRuntimeError> {
        if value.len() > self.bytes.len() {
            return Err(NativeRuntimeError::TooLarge);
        }
        self.bytes[..value.len()].copy_from_slice(value.as_bytes());
        self.len = value.len();
        Ok(())
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> fmt::Write for FixedText<N> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}
