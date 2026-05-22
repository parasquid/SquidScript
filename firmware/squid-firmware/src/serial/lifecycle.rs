use core::fmt::Write;

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use squidvm_core::{error::VmError, limits::MAX_APP_BYTES};

use crate::dev_harness::{AppRegistry, AppSlot, AppStorage, AppStorageError};
use crate::kernel::LifecycleCommand;

use super::{
    vm::{dispatch_loaded_vm, load_vm_for_app, AppRef, RuntimeError},
    ActiveVm, RuntimeSink, TempApp,
};

pub fn storage_error_from_persistent(
    error: crate::dev_harness::PersistentAppError,
) -> AppStorageError {
    match error {
        crate::dev_harness::PersistentAppError::Storage(error) => error,
        _ => AppStorageError::Io,
    }
}

pub fn boot_main(
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    registry: &AppRegistry,
    app_storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &mut TempApp,
    trace: &mut RuntimeSink<'_>,
    vm: &mut Option<ActiveVm>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    let Some(slot) = registry.find("main") else {
        trace.active_app = None;
        writeln!(serial, "BOOT dev-shell no-main").ok();
        return;
    };
    *temp_app = TempApp::empty();
    trace.reset_stack();
    trace.active_app = Some(AppRef::Persistent(slot));
    *vm = None;
    *vm_slot = None;
    match load_vm_for_app(
        AppRef::Persistent(slot),
        registry,
        app_storage,
        app_load_bytes,
        temp_app,
    ) {
        Ok(loaded) => {
            *vm = Some(loaded);
            *vm_slot = Some(slot);
        }
        Err(error) => {
            set_runtime_error(error, last_error, storage_error);
            trace.active_app = None;
            writeln!(serial, "BOOT dev-shell invalid-main").ok();
            return;
        }
    }
    match dispatch_loaded_vm(
        vm,
        AppRef::Persistent(slot),
        "app.start",
        registry,
        app_storage,
        trace,
    ) {
        Ok(()) => {
            process_pending_actions(
                trace,
                registry,
                app_storage,
                app_load_bytes,
                temp_app,
                vm,
                vm_slot,
                last_error,
                storage_error,
            );
            if trace.exited {
                finish_current_exit(
                    trace,
                    registry,
                    app_storage,
                    app_load_bytes,
                    temp_app,
                    vm,
                    vm_slot,
                    last_error,
                    storage_error,
                );
            }
            writeln!(serial, "BOOT main app=main").ok();
        }
        Err(error) => {
            set_runtime_error(error, last_error, storage_error);
            trace.active_app = None;
            *vm = None;
            *vm_slot = None;
            writeln!(serial, "BOOT dev-shell invalid-main").ok();
        }
    }
}

pub(super) fn set_runtime_error(
    error: RuntimeError,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    match error {
        RuntimeError::Vm(error) => *last_error = Some(error),
        RuntimeError::Storage(error) => *storage_error = Some(error),
    }
}

pub(super) fn app_ref_available(app: AppRef, registry: &AppRegistry, temp_app: &TempApp) -> bool {
    match app {
        AppRef::Persistent(slot) => registry.entry(slot).is_some(),
        AppRef::Temp => temp_app.is_available(),
    }
}

pub(super) fn run_app_event(
    app: AppRef,
    event: &str,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &TempApp,
    trace: &mut RuntimeSink<'_>,
) -> Result<(), RuntimeError> {
    let previous = trace.current_app;
    trace.current_app = Some(app);
    let mut vm = Some(load_vm_for_app(
        app,
        registry,
        storage,
        app_load_bytes,
        temp_app,
    )?);
    let result = dispatch_loaded_vm(&mut vm, app, event, registry, storage, trace);
    trace.current_app = previous;
    result
}

fn dispatch_exit_hook(
    app: AppRef,
    trace: &mut RuntimeSink<'_>,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &TempApp,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    if trace.in_exit_hook {
        return;
    }
    trace.in_exit_hook = true;
    let result = run_app_event(
        app,
        "app.exit",
        registry,
        storage,
        app_load_bytes,
        temp_app,
        trace,
    );
    trace.in_exit_hook = false;
    trace.exited = false;
    match result {
        Ok(()) | Err(RuntimeError::Vm(VmError::HandlerNotFound)) => {}
        Err(error) => set_runtime_error(error, last_error, storage_error),
    }
}

pub(super) fn finish_current_exit(
    trace: &mut RuntimeSink<'_>,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &mut TempApp,
    vm: &mut Option<ActiveVm>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    *vm = None;
    *vm_slot = None;
    let Some(current) = trace.active_app else {
        trace.exited = false;
        return;
    };
    dispatch_exit_hook(
        current,
        trace,
        registry,
        storage,
        app_load_bytes,
        temp_app,
        last_error,
        storage_error,
    );
    trace.remove_timers_for(current);
    if matches!(current, AppRef::Temp) {
        *temp_app = TempApp::empty();
    }
    trace.exited = false;
    trace.active_app = trace.pop_return_target().map(AppRef::Persistent);
    if trace.active_app.is_none() {
        trace.request_root_restart();
        return;
    }
    if let Some(app) = trace.active_app {
        match load_vm_for_app(app, registry, storage, app_load_bytes, temp_app) {
            Ok(loaded) => {
                *vm = Some(loaded);
                *vm_slot = match app {
                    AppRef::Persistent(slot) => Some(slot),
                    AppRef::Temp => None,
                };
                if let Err(error) =
                    dispatch_loaded_vm(vm, app, "app.start", registry, storage, trace)
                {
                    set_runtime_error(error, last_error, storage_error);
                }
            }
            Err(error) => set_runtime_error(error, last_error, storage_error),
        }
    }
}

pub(super) fn process_pending_actions(
    trace: &mut RuntimeSink<'_>,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &mut TempApp,
    vm: &mut Option<ActiveVm>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    while let Some(command) = trace.lifecycle_service.pop_command() {
        match command {
            LifecycleCommand::DisarmApp(app_name) => {
                let Some(app) = registry.find(app_name.as_str()) else {
                    *last_error = Some(VmError::InvalidOperand);
                    return;
                };
                trace.remove_timers_for(AppRef::Persistent(app));
            }
            LifecycleCommand::ArmApp(app_name) => {
                let Some(app) = registry.find(app_name.as_str()) else {
                    *last_error = Some(VmError::InvalidOperand);
                    return;
                };
                trace.registration_mode = true;
                let result = run_app_event(
                    AppRef::Persistent(app),
                    "app.arm",
                    registry,
                    storage,
                    app_load_bytes,
                    temp_app,
                    trace,
                );
                trace.registration_mode = false;
                if let Err(error) = result {
                    set_runtime_error(error, last_error, storage_error);
                    return;
                }
            }
            LifecycleCommand::LaunchApp(app_name) => {
                let Some(app) = registry.find(app_name.as_str()) else {
                    *last_error = Some(VmError::InvalidOperand);
                    return;
                };
                let app = AppRef::Persistent(app);
                if let Some(AppRef::Persistent(current)) = trace.active_app {
                    trace.push_return_target(current);
                }
                let current = trace.active_app;
                if let Some(current) = current {
                    *vm = None;
                    *vm_slot = None;
                    dispatch_exit_hook(
                        current,
                        trace,
                        registry,
                        storage,
                        app_load_bytes,
                        temp_app,
                        last_error,
                        storage_error,
                    );
                    trace.remove_timers_for(current);
                }
                trace.active_app = Some(app);
                *vm = None;
                *vm_slot = None;
                if let Err(error) = run_app_event(
                    app,
                    "app.start",
                    registry,
                    storage,
                    app_load_bytes,
                    temp_app,
                    trace,
                ) {
                    set_runtime_error(error, last_error, storage_error);
                    return;
                }
                if trace.exited {
                    finish_current_exit(
                        trace,
                        registry,
                        storage,
                        app_load_bytes,
                        temp_app,
                        vm,
                        vm_slot,
                        last_error,
                        storage_error,
                    );
                }
            }
            LifecycleCommand::ExitApp => {
                trace.exited = true;
                finish_current_exit(
                    trace,
                    registry,
                    storage,
                    app_load_bytes,
                    temp_app,
                    vm,
                    vm_slot,
                    last_error,
                    storage_error,
                );
            }
            LifecycleCommand::RootRestart => {
                trace.lifecycle_service.root_restart().ok();
                return;
            }
            LifecycleCommand::DispatchAppEvent { app, event } => {
                let Some(app) = registry.find(app.as_str()) else {
                    *last_error = Some(VmError::InvalidOperand);
                    return;
                };
                if let Err(error) = run_app_event(
                    AppRef::Persistent(app),
                    event.as_str(),
                    registry,
                    storage,
                    app_load_bytes,
                    temp_app,
                    trace,
                ) {
                    set_runtime_error(error, last_error, storage_error);
                    return;
                }
            }
        }
    }
}
