use core::fmt::Write;

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use squidvm_core::{
    limits::MAX_APP_BYTES,
    strings::{StringResolver, StringTable},
    value::Value,
};

use crate::{dev_harness::AppRegistry, storage::SQUIDFS_LEN};

use super::{ram::live_ram_diagnostics, ActiveVm, TempApp};

pub(super) fn registry_installed_bytes(registry: &AppRegistry) -> usize {
    registry.iter().map(|(_, entry)| entry.len()).sum()
}

fn app_storage_available_bytes(registry: &AppRegistry) -> usize {
    SQUIDFS_LEN.saturating_sub(registry_installed_bytes(registry))
}

pub(super) fn print_resources(
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    registry: &AppRegistry,
    temp_app: &TempApp,
    vm: Option<&ActiveVm>,
) {
    let app_used = registry_installed_bytes(registry);
    let temp_app_buffer_bytes = if temp_app.is_available() {
        MAX_APP_BYTES
    } else {
        0
    };
    let temp_app_bytes = if temp_app.is_available() {
        temp_app.len
    } else {
        0
    };
    let installed_code_cache_bytes = vm.map_or(0, ActiveVm::installed_code_cache_bytes);
    let ram = live_ram_diagnostics();
    writeln!(serial, "BEGIN RESOURCES").ok();
    writeln!(serial, "ram_total_bytes={}", ram.ram_total_bytes).ok();
    writeln!(serial, "ram_heap_total_bytes={}", ram.heap_total_bytes).ok();
    writeln!(serial, "ram_heap_used_bytes={}", ram.heap_used_bytes).ok();
    writeln!(
        serial,
        "ram_heap_available_bytes={}",
        ram.heap_available_bytes()
    )
    .ok();
    writeln!(
        serial,
        "ram_heap_peak_used_bytes={}",
        ram.heap_peak_used_bytes
    )
    .ok();
    writeln!(
        serial,
        "ram_heap_total_allocated_bytes={}",
        ram.heap_total_allocated_bytes
    )
    .ok();
    writeln!(
        serial,
        "ram_heap_total_freed_bytes={}",
        ram.heap_total_freed_bytes
    )
    .ok();
    writeln!(serial, "temp_app_buffer_bytes={temp_app_buffer_bytes}").ok();
    writeln!(serial, "temp_app_bytes={temp_app_bytes}").ok();
    writeln!(
        serial,
        "installed_code_cache_bytes={installed_code_cache_bytes}"
    )
    .ok();
    writeln!(serial, "app_storage_total_bytes={SQUIDFS_LEN}").ok();
    writeln!(serial, "app_storage_used_bytes={app_used}").ok();
    writeln!(
        serial,
        "app_storage_available_bytes={}",
        app_storage_available_bytes(registry)
    )
    .ok();
    writeln!(serial, "END RESOURCES").ok();
    writeln!(serial, "OK RESOURCES.GET").ok();
}

pub(super) fn import_state(vm: &mut ActiveVm, bytes: &[u8]) {
    let Ok(text) = core::str::from_utf8(bytes) else {
        return;
    };
    for line in text.lines() {
        let Some((name, raw_value)) = line.split_once('=') else {
            continue;
        };
        if let Some(value) = parse_value(vm.string_table(), raw_value.trim()) {
            let _ = vm.set_state_value(name.trim(), value);
        }
    }
}

fn parse_value(strings: &dyn StringTable, input: &str) -> Option<Value> {
    if input == "null" {
        Some(Value::Null)
    } else if input == "true" {
        Some(Value::Bool(true))
    } else if input == "false" {
        Some(Value::Bool(false))
    } else if let Ok(value) = input.parse::<i32>() {
        Some(Value::I32(value))
    } else if let Some(text) = input.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        for id in 0..64u16 {
            if strings.string(id).ok()? == text {
                return Some(Value::String(id));
            }
        }
        None
    } else {
        None
    }
}

pub(super) fn print_state(serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>, vm: &ActiveVm) {
    let strings = vm.string_resolver();
    for index in 0..vm.state_count() {
        let name = vm.state_name(index).unwrap_or("<bad-state>");
        let value = vm.state_at(index).unwrap_or(Value::Null);
        write!(serial, "{name}=").ok();
        print_value(serial, &strings, value);
        writeln!(serial).ok();
    }
    writeln!(serial, "exited={}", vm.exited()).ok();
}

fn print_value(
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    strings: &StringResolver<'_>,
    value: Value,
) {
    match value {
        Value::Null => write!(serial, "null").ok(),
        Value::Bool(value) => write!(serial, "{value}").ok(),
        Value::I32(value) => write!(serial, "{value}").ok(),
        Value::String(_) | Value::RuntimeString(_) => write!(
            serial,
            "\"{}\"",
            strings.value_str(value).unwrap_or("<bad-string>")
        )
        .ok(),
        Value::Record(_) => write!(serial, "<record>").ok(),
        Value::List(_) => write!(serial, "<list>").ok(),
    };
}
