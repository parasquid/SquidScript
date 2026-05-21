use core::fmt;

use squidvm_core::{
    error::VmError,
    host::{DisplayLineOptions, DisplayRectOptions, DisplayTextOptions, TraceSink},
    limits::{MAX_APP_BYTES, MAX_CODE_CHUNK_BYTES},
    program::{Program, ProgramIndex},
    reader::SqbcReader,
    strings::{StringResolver, StringTable},
    value::Value,
    vm::{ChunkedVm, Vm},
};

use crate::dev_harness::{AppName, AppRegistry, AppRegistryError, AppSlot, AppStorage, AppStorageError};

use super::RuntimeSink;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AppRef {
    Persistent(AppSlot),
    Temp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TempApp {
    pub(super) name: AppName,
    pub(super) len: usize,
    pub(super) hash: u32,
    pub(super) occupied: bool,
}

impl TempApp {
    pub const fn empty() -> Self {
        Self {
            name: AppName::empty(),
            len: 0,
            hash: 0,
            occupied: false,
        }
    }

    pub(super) fn new(app_id: &str, len: usize, hash: u32) -> Result<Self, AppRegistryError> {
        Ok(Self {
            name: AppName::new(app_id)?,
            len,
            hash,
            occupied: true,
        })
    }

    pub(super) fn is_available(&self) -> bool {
        self.occupied && self.len <= MAX_APP_BYTES
    }
}

pub enum ActiveVm {
    Temp(Vm<'static>),
    Persistent(ChunkedVm),
}

impl ActiveVm {
    pub(super) fn exited(&self) -> bool {
        match self {
            Self::Temp(vm) => vm.exited(),
            Self::Persistent(vm) => vm.exited(),
        }
    }

    pub(super) fn state_count(&self) -> usize {
        match self {
            Self::Temp(vm) => vm.state_count(),
            Self::Persistent(vm) => vm.state_count(),
        }
    }

    pub(super) fn state_name(&self, index: usize) -> Result<&str, VmError> {
        match self {
            Self::Temp(vm) => vm.state_name(index),
            Self::Persistent(vm) => vm.state_name(index),
        }
    }

    pub(super) fn state_at(&self, index: usize) -> Result<Value, VmError> {
        match self {
            Self::Temp(vm) => vm.state_at(index),
            Self::Persistent(vm) => vm.state_at(index),
        }
    }

    pub(super) fn set_state_value(&mut self, name: &str, value: Value) -> Result<(), VmError> {
        match self {
            Self::Temp(vm) => vm.set_state_value(name, value),
            Self::Persistent(vm) => vm.set_state_value(name, value),
        }
    }

    pub(super) fn string_resolver(&self) -> StringResolver<'_> {
        match self {
            Self::Temp(vm) => vm.string_resolver(),
            Self::Persistent(vm) => vm.string_resolver(),
        }
    }

    pub(super) fn string_table(&self) -> &dyn StringTable {
        match self {
            Self::Temp(vm) => vm.program(),
            Self::Persistent(vm) => vm.string_table(),
        }
    }

    pub(super) fn installed_code_cache_bytes(&self) -> usize {
        match self {
            Self::Temp(_) => 0,
            Self::Persistent(_) => MAX_CODE_CHUNK_BYTES,
        }
    }
}


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeError {
    Vm(VmError),
    Storage(AppStorageError),
}

impl From<VmError> for RuntimeError {
    fn from(value: VmError) -> Self {
        Self::Vm(value)
    }
}

impl From<AppStorageError> for RuntimeError {
    fn from(value: AppStorageError) -> Self {
        Self::Storage(value)
    }
}

struct StoredAppReader<'a, S: AppStorage> {
    storage: &'a mut S,
    app_id: &'a str,
}

impl<S: AppStorage> SqbcReader for StoredAppReader<'_, S> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let read = self
            .storage
            .read_app_range(self.app_id, offset, out)
            .map_err(|_| VmError::ReadFailed)?;
        if read == out.len() {
            Ok(())
        } else {
            Err(VmError::ReadFailed)
        }
    }
}

struct StoredAppHost<'a, 'd, S: AppStorage> {
    storage: &'a mut S,
    app_id: &'a str,
    trace: &'a mut RuntimeSink<'d>,
}

impl<S: AppStorage> SqbcReader for StoredAppHost<'_, '_, S> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let read = self
            .storage
            .read_app_range(self.app_id, offset, out)
            .map_err(|_| VmError::ReadFailed)?;
        if read == out.len() {
            Ok(())
        } else {
            Err(VmError::ReadFailed)
        }
    }
}

impl<S: AppStorage> TraceSink for StoredAppHost<'_, '_, S> {
    fn trace(&mut self, message: &str) {
        self.trace.trace(message);
    }

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
        self.trace.debug_print(strings, values);
    }

    fn draw_clear(&mut self, color: &str) {
        self.trace.draw_clear(color);
    }

    fn draw_text(
        &mut self,
        strings: &StringResolver<'_>,
        text: Value,
        options: DisplayTextOptions<'_>,
    ) {
        self.trace.draw_text(strings, text, options);
    }

    fn draw_rect(&mut self, options: DisplayRectOptions<'_>) {
        self.trace.draw_rect(options);
    }

    fn draw_line(&mut self, options: DisplayLineOptions<'_>) {
        self.trace.draw_line(options);
    }

    fn hardware_gpio_write(&mut self, name: &str, value: bool) -> Result<(), VmError> {
        self.trace.hardware_gpio_write(name, value)
    }

    fn hardware_gpio_toggle(&mut self, name: &str) -> Result<(), VmError> {
        self.trace.hardware_gpio_toggle(name)
    }

    fn hardware_gpio_read(&mut self, name: &str) -> Result<bool, VmError> {
        self.trace.hardware_gpio_read(name)
    }

    fn service_indicator_write(&mut self, value: bool) -> Result<(), VmError> {
        self.trace.service_indicator_write(value)
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        self.trace.service_indicator_toggle()
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        self.trace.service_indicator_read()
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        self.trace.app_launch(app)
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        self.trace.app_arm(app)
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        self.trace.app_disarm(app)
    }

    fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
        self.trace.service_timer_every(event, interval_ms)
    }

    fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
        self.trace.service_timer_after(event, delay_ms)
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        self.trace.system_memory_text(out)
    }

    fn system_storage_text(&mut self, name: &str, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        self.trace.system_storage_text(name, out)
    }

    fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
        self.storage
            .read_state(self.app_id, out)
            .map_err(|_| VmError::ReadFailed)
    }

    fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        self.storage
            .write_state(self.app_id, bytes)
            .map_err(|_| VmError::ReadFailed)
    }

    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        self.storage
            .delete_state(self.app_id)
            .map_err(|_| VmError::ReadFailed)
    }
}

pub(super) fn load_vm_for_app(
    app: AppRef,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &TempApp,
) -> Result<ActiveVm, RuntimeError> {
    match app {
        AppRef::Temp => {
            let bytes = &app_load_bytes[..temp_app.len];
            let ptr = bytes.as_ptr();
            let len = bytes.len();
            // The firmware owns APP_LOAD_BYTES for the lifetime of the temp VM
            // and drops that VM before reusing the buffer.
            let stable = unsafe { core::slice::from_raw_parts(ptr, len) };
            Ok(ActiveVm::Temp(Vm::new(Program::parse(stable)?)))
        }
        AppRef::Persistent(slot) => {
            let Some(entry) = registry.entry(slot) else {
                return Err(RuntimeError::Storage(AppStorageError::NotFound));
            };
            let mut reader = StoredAppReader {
                storage,
                app_id: entry.name(),
            };
            let index = ProgramIndex::parse_from_reader(&mut reader, app_load_bytes)?;
            Ok(ActiveVm::Persistent(ChunkedVm::new(index)))
        }
    }
}

pub(super) fn dispatch_loaded_vm(
    vm: &mut Option<ActiveVm>,
    app: AppRef,
    event: &str,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    trace: &mut RuntimeSink<'_>,
) -> Result<(), RuntimeError> {
    let Some(active) = vm.as_mut() else {
        return Err(RuntimeError::Vm(VmError::InvalidOperand));
    };
    let previous = trace.current_app;
    trace.current_app = Some(app);
    let result = match (&mut *active, app) {
        (ActiveVm::Temp(active), AppRef::Temp) => active.dispatch(event, trace),
        (ActiveVm::Persistent(active), AppRef::Persistent(slot)) => {
            let Some(entry) = registry.entry(slot) else {
                return Err(RuntimeError::Storage(AppStorageError::NotFound));
            };
            let mut host = StoredAppHost {
                storage,
                app_id: entry.name(),
                trace,
            };
            active.dispatch(&mut host, event)
        }
        _ => Err(VmError::InvalidOperand),
    };
    if active.exited() {
        trace.exited = true;
    }
    trace.current_app = previous;
    result.map_err(RuntimeError::Vm)
}
