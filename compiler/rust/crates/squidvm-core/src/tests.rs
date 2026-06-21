use core::fmt::{self, Write};
use squidc_core::{
    compile::{compile, CompileRequest},
    profile::PORTABLE_TARGET_ID,
};

use crate::{
    bytecode::*, chunk::*, error::*, host::*, limits::*, program::*, reader::*, strings::*,
    value::*, vm::*,
};

struct EmptyStringTable;

impl StringTable for EmptyStringTable {
    fn string(&self, _id: u16) -> Result<&str, VmError> {
        Err(VmError::InvalidOperand)
    }

    fn find_string(&self, _value: &str) -> Result<Option<u16>, VmError> {
        Ok(None)
    }
}

#[test]
fn runtime_record_field_limit_matches_largest_service_record() {
    assert_eq!(MAX_RUNTIME_RECORDS, 8);
    assert_eq!(MAX_RUNTIME_RECORD_FIELDS, 26);
}

#[test]
fn runtime_string_writer_rejects_oversized_writes_without_truncating() {
    let mut strings = StringInterner::new();
    let original = "ok";
    let value = strings.intern_event(&EmptyStringTable, original).unwrap();

    let oversized = "x".repeat(MAX_RUNTIME_STRING_BYTES + 1);
    assert_eq!(
        strings.intern_event(&EmptyStringTable, &oversized),
        Err(VmError::InvalidOperand)
    );

    let Value::String(StringRef::Dynamic(id)) = value else {
        panic!("expected dynamic string");
    };
    assert_eq!(
        strings
            .get(&EmptyStringTable, StringRef::Dynamic(id))
            .unwrap(),
        original
    );
}

#[test]
fn dynamic_string_refs_fail_when_slots_are_exhausted_instead_of_wrapping() {
    let mut strings = StringInterner::new();

    for index in 0..MAX_SERVICE_STRINGS {
        strings
            .intern_event(&EmptyStringTable, &format!("s{index}"))
            .unwrap();
    }

    match strings.intern_event(&EmptyStringTable, "one-too-many") {
        Err(err) => assert_eq!(err, VmError::TooManyStrings),
        Ok(_) => panic!("expected dynamic string allocation failure"),
    }
}

#[test]
fn dynamic_string_refs_reuse_existing_substring_bytes() {
    let mut strings = StringInterner::new();

    let parent = strings
        .intern_event(&EmptyStringTable, "prefix-shared-suffix")
        .unwrap();
    let child = strings.intern_event(&EmptyStringTable, "shared").unwrap();

    assert_eq!(strings.dynamic_bytes_used(), "prefix-shared-suffix".len());
    assert_eq!(
        strings.value_str(&EmptyStringTable, parent),
        Ok("prefix-shared-suffix")
    );
    assert_eq!(strings.value_str(&EmptyStringTable, child), Ok("shared"));
}

#[test]
fn dynamic_string_refs_reuse_static_substring_bytes() {
    let mut strings = StringInterner::new();

    let child = strings.intern_event(&EmptyStringTable, "ead").unwrap();

    assert_eq!(strings.dynamic_bytes_used(), 0);
    assert_eq!(strings.value_str(&EmptyStringTable, child), Ok("ead"));
}

#[test]
fn retained_substring_state_value_survives_without_parent_value() {
    let mut strings = StringInterner::new();
    let _parent = strings
        .intern_event(&EmptyStringTable, "prefix-shared-suffix")
        .unwrap();
    let child = strings.intern_event(&EmptyStringTable, "shared").unwrap();
    let mut state = [child];

    strings
        .retain_state_values(&EmptyStringTable, &mut state)
        .unwrap();

    assert_eq!(strings.dynamic_bytes_used(), "shared".len());
    assert_eq!(strings.value_str(&EmptyStringTable, state[0]), Ok("shared"));
}

const WIFI_SCAN_TEST_NETWORKS: [WifiAccessPoint; 2] = [
    WifiAccessPoint::from_fixed_parts(
        [
            b'L', b'a', b'b', b'N', b'e', b't', b'w', b'o', b'r', b'k', 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        10,
        [
            b'0', b'0', b':', b'1', b'1', b':', b'2', b'2', b':', b'3', b'3', b':', b'4', b'4',
            b':', b'5', b'5',
        ],
        true,
        10,
        6,
        -42,
        Some("wpa2"),
        false,
    ),
    WifiAccessPoint::from_fixed_parts(
        [0; WIFI_SCAN_SSID_CAP],
        0,
        [0; WIFI_SCAN_BSSID_TEXT_LEN],
        false,
        0,
        11,
        -80,
        Some("unknown"),
        true,
    ),
];

const WIFI_SCAN_BUDGET_NETWORKS: [WifiAccessPoint; 4] = [
    WifiAccessPoint::from_fixed_parts(
        [
            b'N', b'e', b't', b'0', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ],
        4,
        [
            b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0',
            b':', b'0', b'0',
        ],
        true,
        4,
        1,
        -40,
        Some("wpa2"),
        false,
    ),
    WifiAccessPoint::from_fixed_parts(
        [
            b'N', b'e', b't', b'1', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ],
        4,
        [
            b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0',
            b':', b'1', b'1',
        ],
        true,
        4,
        6,
        -50,
        Some("wpa3"),
        false,
    ),
    WifiAccessPoint::from_fixed_parts(
        [
            b'N', b'e', b't', b'2', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ],
        4,
        [
            b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0',
            b':', b'2', b'2',
        ],
        true,
        4,
        11,
        -60,
        Some("open"),
        false,
    ),
    WifiAccessPoint::from_fixed_parts(
        [
            b'N', b'e', b't', b'3', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ],
        4,
        [
            b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0', b':', b'0', b'0',
            b':', b'3', b'3',
        ],
        true,
        4,
        13,
        -70,
        Some("wep"),
        true,
    ),
];

const REGISTRY_BUDGET_APPS: [AppRegistryEntry; 8] = [
    AppRegistryEntry {
        id: "app0",
        name: "App 0",
        build: "build0",
        description: "Desc 0",
    },
    AppRegistryEntry {
        id: "app1",
        name: "App 1",
        build: "build1",
        description: "Desc 1",
    },
    AppRegistryEntry {
        id: "app2",
        name: "App 2",
        build: "build2",
        description: "Desc 2",
    },
    AppRegistryEntry {
        id: "app3",
        name: "App 3",
        build: "build3",
        description: "Desc 3",
    },
    AppRegistryEntry {
        id: "app4",
        name: "App 4",
        build: "build4",
        description: "Desc 4",
    },
    AppRegistryEntry {
        id: "app5",
        name: "App 5",
        build: "build5",
        description: "Desc 5",
    },
    AppRegistryEntry {
        id: "app6",
        name: "App 6",
        build: "build6",
        description: "Desc 6",
    },
    AppRegistryEntry {
        id: "app7",
        name: "App 7",
        build: "build7",
        description: "Desc 7",
    },
];

const FILE_BUDGET_LINES: [&str; 4] = ["line0", "line1", "line2", "line3"];
const LONG_FILE_LINES: [&str; 4] = [
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "cccccccccccccccccccccccccccccccccccccccccccccccc",
    "dddddddddddddddddddddddddddddddddddddddddddddddd",
];

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

    fn service_indicator_breathe(&mut self) -> Result<(), VmError> {
        self.events.push("indicator breathe".to_string());
        Ok(())
    }

    fn service_indicator_blink(&mut self, on_ms: i32, off_ms: i32) -> Result<(), VmError> {
        self.events
            .push(format!("indicator blink {on_ms}/{off_ms}"));
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
    installed_app_id: String,
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
                Value::String(_) => line.push_str(strings.value_str(*value).unwrap()),
                Value::I32(value) => line.push_str(&value.to_string()),
                Value::Bool(value) => line.push_str(&value.to_string()),
                Value::Null => line.push_str("null"),
                Value::Record(_) => line.push_str("<record>"),
                Value::List(_) => line.push_str("<list>"),
                Value::Handle(handle) => {
                    write!(line, "<handle:{:?}:{}>", handle.kind, handle.id).unwrap();
                }
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

    fn app_install<'a>(
        &'a mut self,
        file_ref: &str,
        app_id: Option<&str>,
    ) -> Result<AppInstallResult<'a>, VmError> {
        let app_id = app_id.unwrap_or("metadata-app");
        self.events.push(format!("install {file_ref} {app_id}"));
        self.installed_app_id.clear();
        self.installed_app_id.push_str(app_id);
        Ok(AppInstallResult {
            id: &self.installed_app_id,
        })
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

    fn service_power_sleep(&mut self, wake_after_ms: i32) -> Result<(), VmError> {
        self.events
            .push(format!("service.power.sleep {wake_after_ms}"));
        Ok(())
    }

    fn service_ble_start(&mut self, id: &str) -> Result<(), VmError> {
        self.events.push(format!("service.ble.start {id}"));
        Ok(())
    }

    fn service_ble_stop(&mut self) -> Result<(), VmError> {
        self.events.push("service.ble.stop".to_string());
        Ok(())
    }

    fn service_http_start(&mut self, id: &str) -> Result<(), VmError> {
        self.events.push(format!("service.http.start {id}"));
        Ok(())
    }

    fn service_http_stop(&mut self) -> Result<(), VmError> {
        self.events.push("service.http.stop".to_string());
        Ok(())
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(out, "RAM 292 KiB").map_err(|_| VmError::InvalidOperand)
    }

    fn system_storage_text(&mut self, name: &str, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(out, "{name} 1 MiB").map_err(|_| VmError::InvalidOperand)
    }

    fn system_start_reason_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(out, "wake").map_err(|_| VmError::InvalidOperand)
    }

    fn display_info<'a>(&'a mut self) -> Result<DisplayInfo<'a>, VmError> {
        self.events.push("display.info".to_string());
        Ok(DisplayInfo {
            ok: true,
            error: None,
            warning: None,
            available: true,
            status: "ready",
            binding: "display.default",
            driver: "ssd1306",
            transport: "i2c",
            width: 78,
            height: 40,
            physical_width: 78,
            physical_height: 40,
            rotation: 0,
            color_model: "mono",
            logical_gray_levels: 2,
            native_bpp: 1,
            native_pixel_format: "MONO1_PACKED",
            default_font_height: 8,
            supports_partial_refresh: false,
            supports_fast_refresh: true,
        })
    }

    fn device_config_load<'a>(
        &'a mut self,
        source: &str,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        self.events.push(format!("device.config.load {source}"));
        Ok(DeviceConfigResult {
            ok: true,
            error: None,
            warning: Some("loaded"),
        })
    }

    fn device_config_set<'a>(
        &'a mut self,
        key: &str,
        value: Value,
        strings: &StringResolver<'_>,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        self.events.push(format!(
            "device.config.set {key} {}",
            strings.value_str(value).unwrap_or("<value>")
        ));
        Ok(DeviceConfigResult {
            ok: true,
            error: None,
            warning: None,
        })
    }

    fn device_config_rebind<'a>(
        &'a mut self,
        binding: &str,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        self.events.push(format!("device.config.rebind {binding}"));
        Ok(DeviceConfigResult {
            ok: true,
            error: None,
            warning: Some("rebound"),
        })
    }

    fn device_config_save<'a>(
        &'a mut self,
        destination: &str,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        self.events
            .push(format!("device.config.save {destination}"));
        Ok(DeviceConfigResult {
            ok: true,
            error: None,
            warning: None,
        })
    }

    fn file_pick_file<'a>(
        &'a mut self,
        extension: &str,
    ) -> Result<FilePickFileResult<'a>, VmError> {
        self.events.push(format!("file.pickFile {extension}"));
        Ok(FilePickFileResult::unsupported())
    }

    fn file_read_text<'a>(&'a mut self, path: &str) -> Result<FileReadTextResult<'a>, VmError> {
        self.events.push(format!("file.readText {path}"));
        Ok(FileReadTextResult::unsupported())
    }

    fn file_read_lines<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<FileReadLinesResult<'a>, VmError> {
        self.events
            .push(format!("file.readLines {path} {max_lines}"));
        Ok(FileReadLinesResult::unsupported())
    }

    fn binbook_open<'a>(&'a mut self, path: &str) -> Result<BinBookOpenResult<'a>, VmError> {
        self.events.push(format!("binbook.open {path}"));
        Ok(BinBookOpenResult {
            ok: true,
            error: None,
            book: Some(Handle::new(HandleKind::BinBook, 7)),
        })
    }

    fn file_copy<'a>(
        &'a mut self,
        source: &str,
        library: &str,
        name: &str,
    ) -> Result<FileCopyResult<'a>, VmError> {
        self.events
            .push(format!("file.copy {source} {library} {name}"));
        Ok(FileCopyResult {
            ok: true,
            error: None,
            reference: Some("content:books/r/upload.binbook"),
            bytes_written: 4096,
        })
    }

    fn binbook_info<'a>(&'a mut self, book: Handle) -> Result<BinBookInfoResult<'a>, VmError> {
        self.events
            .push(format!("binbook.info {:?}:{}", book.kind, book.id));
        Ok(BinBookInfoResult {
            ok: true,
            error: None,
            title: Some("Sample Book"),
            page_count: 3,
            chapter_count: BINBOOK_TEST_CHAPTERS.len() as i32,
        })
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, VmError> {
        self.events.push(format!(
            "binbook.readPage {:?}:{} {page_index}",
            book.kind, book.id
        ));
        Ok(BinBookReadPageResult {
            ok: true,
            error: None,
            drawable: Some(Handle::new(HandleKind::Drawable, 11)),
        })
    }

    fn binbook_chapters<'a>(
        &'a mut self,
        book: Handle,
        offset: i32,
        limit: i32,
    ) -> Result<BinBookChapterListResult<'a>, VmError> {
        self.events.push(format!(
            "binbook.chapters {:?}:{} {offset} {limit}",
            book.kind, book.id
        ));
        Ok(BinBookChapterListResult {
            ok: true,
            error: None,
            items: &BINBOOK_TEST_CHAPTERS[..2],
            count: BINBOOK_TEST_CHAPTERS.len() as i32,
            has_more: true,
        })
    }

    fn binbook_chapter<'a>(
        &'a mut self,
        book: Handle,
        index: i32,
    ) -> Result<BinBookChapterResult<'a>, VmError> {
        self.events.push(format!(
            "binbook.chapter {:?}:{} {index}",
            book.kind, book.id
        ));
        Ok(BinBookChapterResult {
            ok: index >= 0 && (index as usize) < BINBOOK_TEST_CHAPTERS.len(),
            error: None,
            chapter: BINBOOK_TEST_CHAPTERS.get(index as usize).copied(),
        })
    }

    fn draw_resource(
        &mut self,
        _strings: &StringResolver<'_>,
        drawable: Value,
        options: DisplayResourceOptions,
    ) {
        self.events.push(format!(
            "draw.resource {drawable:?} {} {} {} {}",
            options.x, options.y, options.w, options.h
        ));
    }

    fn app_armed_stack<'a>(&'a mut self) -> Result<AppArmedStack<'a>, VmError> {
        self.events.push("armed.stack".to_string());
        Ok(AppArmedStack {
            entries: &LIFECYCLE_ARMED_STACK,
        })
    }
}

#[derive(Default)]
struct RegistryTrace {
    events: Vec<String>,
}

const REGISTRY_TEST_APPS: [AppRegistryEntry; 2] = [
    AppRegistryEntry {
        id: "main",
        name: "Main",
        build: "dev-main",
        description: "Root app",
    },
    AppRegistryEntry {
        id: "reader",
        name: "Reader",
        build: "dev-reader",
        description: "Read documents",
    },
];

const CONTENT_TEST_BOOKS: [ContentBinBookEntry; 2] = [
    ContentBinBookEntry {
        name: "alpha.binbook",
        reference: "content:books/p/alpha.binbook",
        size: 4096,
    },
    ContentBinBookEntry {
        name: "travel-notes.binbook",
        reference: "content:books/r/travel-notes.binbook",
        size: 8192,
    },
];

const BINBOOK_TEST_CHAPTERS: [BinBookChapterEntry; 3] = [
    BinBookChapterEntry {
        index: 0,
        title: "Chapter One",
        page_index: 0,
        level: 0,
        entry_type: 3,
    },
    BinBookChapterEntry {
        index: 1,
        title: "Chapter Two",
        page_index: 4,
        level: 0,
        entry_type: 3,
    },
    BinBookChapterEntry {
        index: 2,
        title: "Extra",
        page_index: 9,
        level: 1,
        entry_type: 4,
    },
];

const LIFECYCLE_PROCESS_STACK: [&str; 2] = ["main", "reader"];
const LIFECYCLE_ARMED_STACK: [AppArmedStackEntry; 2] = [
    AppArmedStackEntry {
        app_id: "break-reminder",
        event: "timer.break",
    },
    AppArmedStackEntry {
        app_id: "weather-sync",
        event: "timer.sync",
    },
];

impl TraceSink for RegistryTrace {
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
                Value::String(_) => line.push_str(strings.value_str(*value).unwrap()),
                Value::I32(value) => line.push_str(&value.to_string()),
                Value::Bool(value) => line.push_str(&value.to_string()),
                Value::Null => line.push_str("null"),
                Value::Record(_) => line.push_str("<record>"),
                Value::List(_) => line.push_str("<list>"),
                Value::Handle(_) => line.push_str("<handle>"),
            }
        }
        self.events.push(format!("debug {line}"));
    }

    fn app_registry_list<'a>(&'a mut self) -> Result<AppRegistryList<'a>, VmError> {
        self.events.push("registry.list".to_string());
        Ok(AppRegistryList {
            apps: &REGISTRY_TEST_APPS,
        })
    }

    fn app_registry_get<'a>(&'a mut self, app_id: &str) -> Result<AppRegistryEntry<'a>, VmError> {
        self.events.push(format!("registry.get {app_id}"));
        REGISTRY_TEST_APPS
            .iter()
            .copied()
            .find(|app| app.id == app_id)
            .ok_or(VmError::InvalidOperand)
    }

    fn content_binbook_list<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
    ) -> Result<ContentBinBookListResult<'a>, VmError> {
        self.events
            .push(format!("content.binbook.list {library} {offset} {limit}"));
        Ok(ContentBinBookListResult {
            ok: true,
            error: None,
            warning: Some("sd-unavailable"),
            items: &CONTENT_TEST_BOOKS,
            count: 7,
            has_more: true,
        })
    }

    fn binbook_open<'a>(&'a mut self, path: &str) -> Result<BinBookOpenResult<'a>, VmError> {
        self.events.push(format!("binbook.open {path}"));
        Ok(BinBookOpenResult {
            ok: true,
            error: None,
            book: Some(Handle::new(HandleKind::BinBook, 7)),
        })
    }

    fn app_process_stack<'a>(&'a mut self) -> Result<AppProcessStack<'a>, VmError> {
        self.events.push("process.stack".to_string());
        Ok(AppProcessStack {
            apps: &LIFECYCLE_PROCESS_STACK,
        })
    }

    fn app_armed_stack<'a>(&'a mut self) -> Result<AppArmedStack<'a>, VmError> {
        self.events.push("armed.stack".to_string());
        Ok(AppArmedStack {
            entries: &LIFECYCLE_ARMED_STACK,
        })
    }
}

#[derive(Default)]
struct WifiTrace {
    events: Vec<String>,
    active: bool,
    ssid: String,
    teardown_count: usize,
}

impl TraceSink for WifiTrace {
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
                Value::String(_) => line.push_str(strings.value_str(*value).unwrap()),
                Value::I32(value) => line.push_str(&value.to_string()),
                Value::Bool(value) => line.push_str(&value.to_string()),
                Value::Null => line.push_str("null"),
                Value::Record(_) => line.push_str("<record>"),
                Value::List(_) => line.push_str("<list>"),
                Value::Handle(_) => line.push_str("<handle>"),
            }
        }
        self.events.push(format!("debug {line}"));
    }

    fn service_wifi_start_ap<'a>(&'a mut self, ssid: &str) -> Result<WifiOperation<'a>, VmError> {
        self.events.push(format!("wifi.startAP {ssid}"));
        self.active = true;
        self.ssid = ssid.to_string();
        Ok(WifiOperation {
            active: true,
            kind: Some("startAP"),
            state: "done",
            done: true,
            cancelled: false,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_stop_ap<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.events.push("wifi.stopAP".to_string());
        self.active = false;
        Ok(WifiOperation {
            active: true,
            kind: Some("stopAP"),
            state: "done",
            done: true,
            cancelled: false,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_connect<'a>(&'a mut self, profile: &str) -> Result<WifiOperation<'a>, VmError> {
        self.events.push(format!("wifi.connect {profile}"));
        self.active = true;
        self.ssid = profile.to_string();
        Ok(WifiOperation {
            active: true,
            kind: Some("connect"),
            state: "done",
            done: true,
            cancelled: false,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_disconnect<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.events.push("wifi.disconnect".to_string());
        self.active = false;
        Ok(WifiOperation {
            active: true,
            kind: Some("disconnect"),
            state: "done",
            done: true,
            cancelled: false,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        self.events.push("wifi.status".to_string());
        Ok(WifiStatus {
            active: self.active,
            mode: Some("ap"),
            ip_address: Some("192.168.4.1"),
            ssid: Some(&self.ssid),
            clients: 0,
            error: None,
            state: if self.active { "started" } else { "stopped" },
            backend: "sim",
            driver_started: self.active,
            configured: self.active,
            driver_mode: if self.active { Some("ap") } else { None },
            channel: 1,
            ap_start_events: if self.active { 1 } else { 0 },
            ap_stop_events: 0,
            probe_events: 0,
            sta_connected_events: 0,
            sta_disconnected_events: 0,
            last_backend_code: None,
            profile: if self.active { Some(&self.ssid) } else { None },
            connected: self.active,
            scan_matches: if self.active { 1 } else { 0 },
            rssi: if self.active { -42 } else { 0 },
            auth: if self.active { Some("wpa2") } else { None },
            bssid: if self.active {
                Some("00:11:22:33:44:55")
            } else {
                None
            },
            disconnect_reason: None,
            disconnect_reason_code: 0,
        })
    }

    fn service_wifi_get_ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        self.events.push("wifi.getAPIP".to_string());
        Ok(WifiApIp {
            ip: Some("192.168.4.1"),
            gw: Some("192.168.4.1"),
            netmask: Some("255.255.255.0"),
            error: None,
        })
    }

    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.events.push("wifi.scan".to_string());
        Ok(WifiOperation {
            active: true,
            kind: Some("scan"),
            state: "started",
            done: false,
            cancelled: false,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_operation<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.events.push("wifi.operation".to_string());
        Ok(WifiOperation {
            active: true,
            kind: Some("scan"),
            state: "done",
            done: true,
            cancelled: false,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_result<'a>(&'a mut self) -> Result<WifiOperationResult<'a>, VmError> {
        self.events.push("wifi.result".to_string());
        Ok(WifiOperationResult {
            ready: true,
            kind: Some("scan"),
            state: "done",
            ok: true,
            error: None,
            cancelled: false,
            count: WIFI_SCAN_TEST_NETWORKS.len() as i32,
        })
    }

    fn service_wifi_cancel<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.events.push("wifi.cancel".to_string());
        Ok(WifiOperation {
            active: true,
            kind: Some("scan"),
            state: "cancelled",
            done: true,
            cancelled: true,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_scan_network<'a>(
        &'a mut self,
        index: i32,
    ) -> Result<WifiScanNetwork<'a>, VmError> {
        self.events.push(format!("wifi.scanNetwork {index}"));
        let network = usize::try_from(index)
            .ok()
            .and_then(|index| WIFI_SCAN_TEST_NETWORKS.get(index).copied());
        Ok(WifiScanNetwork {
            ok: network.is_some(),
            error: if network.is_some() {
                None
            } else {
                Some("not found")
            },
            network,
        })
    }

    fn service_wifi_teardown(&mut self) -> Result<(), VmError> {
        self.events.push("wifi.teardown".to_string());
        self.teardown_count += 1;
        self.active = false;
        Ok(())
    }
}

#[derive(Default)]
struct BudgetTrace {
    events: Vec<String>,
    wifi_teardown_count: usize,
}

impl TraceSink for BudgetTrace {
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
                Value::String(_) => line.push_str(strings.value_str(*value).unwrap()),
                Value::I32(value) => line.push_str(&value.to_string()),
                Value::Bool(value) => line.push_str(&value.to_string()),
                Value::Null => line.push_str("null"),
                Value::Record(_) => line.push_str("<record>"),
                Value::List(_) => line.push_str("<list>"),
                Value::Handle(_) => line.push_str("<handle>"),
            }
        }
        self.events.push(format!("debug {line}"));
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(out, "RAM 292 KiB").map_err(|_| VmError::InvalidOperand)
    }

    fn system_start_reason_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(out, "wake").map_err(|_| VmError::InvalidOperand)
    }

    fn display_info<'a>(&'a mut self) -> Result<DisplayInfo<'a>, VmError> {
        self.events.push("display.info".to_string());
        Ok(DisplayInfo {
            ok: true,
            error: None,
            warning: None,
            available: true,
            status: "ready",
            binding: "display.default",
            driver: "ssd1306",
            transport: "i2c",
            width: 78,
            height: 40,
            physical_width: 78,
            physical_height: 40,
            rotation: 0,
            color_model: "mono",
            logical_gray_levels: 2,
            native_bpp: 1,
            native_pixel_format: "MONO1_PACKED",
            default_font_height: 8,
            supports_partial_refresh: false,
            supports_fast_refresh: true,
        })
    }

    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.events.push("wifi.scan".to_string());
        Ok(WifiOperation {
            active: true,
            kind: Some("scan"),
            state: "started",
            done: false,
            cancelled: false,
            ok: true,
            error: None,
        })
    }

    fn service_wifi_result<'a>(&'a mut self) -> Result<WifiOperationResult<'a>, VmError> {
        self.events.push("wifi.result".to_string());
        Ok(WifiOperationResult {
            ready: true,
            kind: Some("scan"),
            state: "done",
            ok: true,
            error: None,
            cancelled: false,
            count: WIFI_SCAN_BUDGET_NETWORKS.len() as i32,
        })
    }

    fn service_wifi_scan_network<'a>(
        &'a mut self,
        index: i32,
    ) -> Result<WifiScanNetwork<'a>, VmError> {
        self.events.push(format!("wifi.scanNetwork {index}"));
        let network = usize::try_from(index)
            .ok()
            .and_then(|index| WIFI_SCAN_BUDGET_NETWORKS.get(index).copied());
        Ok(WifiScanNetwork {
            ok: network.is_some(),
            error: if network.is_some() {
                None
            } else {
                Some("not found")
            },
            network,
        })
    }

    fn service_wifi_teardown(&mut self) -> Result<(), VmError> {
        self.events.push("wifi.teardown".to_string());
        self.wifi_teardown_count += 1;
        Ok(())
    }

    fn app_registry_list<'a>(&'a mut self) -> Result<AppRegistryList<'a>, VmError> {
        self.events.push("registry.list".to_string());
        Ok(AppRegistryList {
            apps: &REGISTRY_BUDGET_APPS,
        })
    }

    fn app_registry_get<'a>(&'a mut self, app_id: &str) -> Result<AppRegistryEntry<'a>, VmError> {
        self.events.push(format!("registry.get {app_id}"));
        REGISTRY_BUDGET_APPS
            .iter()
            .copied()
            .find(|app| app.id == app_id)
            .ok_or(VmError::InvalidOperand)
    }

    fn file_read_lines<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<FileReadLinesResult<'a>, VmError> {
        self.events
            .push(format!("file.readLines {path} {max_lines}"));
        let lines = if path == "long.txt" {
            &LONG_FILE_LINES
        } else {
            &FILE_BUDGET_LINES
        };
        Ok(FileReadLinesResult {
            ok: true,
            error: None,
            lines,
        })
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
fn runs_sqbc_emitted_by_squidc_core() {
    let source = include_str!("../../../fixtures/conformance/headless_counter.squid");
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
fn app_install_accepts_a_runtime_file_ref_expression() {
    // app.install's file_ref is a runtime expression (here a state string), not a
    // compile-time string literal. The handler must reach the host with the actual
    // runtime path -- this is what lets an app install a file whose path is only
    // known at runtime (e.g. a BLE-received object's staged path).
    let source = r##"app "install-demo"
state {
  path: string = "/sd/apps/x/main.sqbc"
}
event.on("app.start") {
  state.load()
  app.install(state.path, "target")
}
"##;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert!(
        trace
            .events
            .contains(&"install /sd/apps/x/main.sqbc target".to_string()),
        "expected the runtime file_ref to reach the host, got events: {:?}",
        trace.events
    );
}

#[test]
fn app_install_can_return_installed_id_for_dynamic_launch() {
    let source = r##"app "install-demo"
event.on("app.start") {
  let installed = app.install("/sq/tmp/upload.sqbc")
  debug.print(installed.id)
  app.launch(installed.id)
}
"##;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "install /sq/tmp/upload.sqbc metadata-app",
            "debug metadata-app",
            "launch metadata-app",
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
  state.count = state.count + 7
  state.enabled = true
  state.label = state.label + "-hot"
  state.retryAt = 42
  state.save()
}
event.on("key.BACK") {
  state.reset()
}
screen("main") {}
"#;
    let bytes = compile_sqbc(&source);
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
    assert!(matches!(
        restored.state_value("label").unwrap(),
        Value::String(StringRef::Dynamic(_))
    ));
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
fn state_load_reuses_matching_sqbc_string_literal() {
    let source = r#"app "state-literal-load"
state { label: string = "cold" }
event.on("app.start") {
  state.load()
}
screen("main") {}
"#;
    let bytes = compile_sqbc(&source);
    let program = Program::parse(&bytes).unwrap();
    let mut trace = StateTrace {
        saved_state: string_state_record("label", "cold"),
        ..StateTrace::default()
    };
    let mut vm = Vm::new(program);

    vm.dispatch("app.start", &mut trace).unwrap();

    let loaded = vm.state_value("label").unwrap();
    assert!(matches!(loaded, Value::String(_)), "{loaded:?}");
    assert_eq!(vm.string_resolver().value_str(loaded), Ok("cold"));
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
    let bytes = compile_sqbc(&source);
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
fn parses_sqbc_header_and_section_records_for_partial_loading() {
    let bytes = fixture_counter_sqbc();
    let header = Program::parse_header(&bytes[..14]).unwrap();
    let header_bytes = &bytes[..header.header_len];

    assert_eq!(header.file_len, bytes.len());
    assert_eq!(header.section_count, 5);
    let first = Program::parse_section_record(header_bytes, 0).unwrap();
    assert_eq!(first.kind, SECTION_STRINGS);
    assert!(first.offset >= header.header_len);
    assert!(first.len > 0);
}

#[test]
fn rejects_duplicate_sqbc_section_kinds() {
    let mut bytes = fixture_counter_sqbc();
    let first_kind = read_u16(&bytes, 14).unwrap();
    write_u16_at(&mut bytes, 14 + 12, first_kind);

    assert_eq!(
        Program::parse(&bytes).err(),
        Some(VmError::DuplicateSection)
    );

    let mut scratch = [0u8; 4096];
    assert_eq!(
        ProgramIndex::parse(&bytes, &mut scratch).err(),
        Some(VmError::DuplicateSection)
    );
}

#[test]
fn rejects_duplicate_sqbc_state_names() {
    let mut bytes = compile_sqbc(
        r#"app "dup-state"
state {
  first: int = 1
  second: int = 2
}
event.on("app.start") {}
screen("main") {}
"#,
    );
    duplicate_u16_field(&mut bytes, SECTION_STATE, 2, 11);

    assert_eq!(
        Program::parse(&bytes).err(),
        Some(VmError::DuplicateTableKey)
    );

    let mut scratch = [0u8; 4096];
    assert_eq!(
        ProgramIndex::parse(&bytes, &mut scratch).err(),
        Some(VmError::DuplicateTableKey)
    );
}

#[test]
fn rejects_duplicate_sqbc_handler_events_screens_and_triggers() {
    let mut handler_bytes = compile_sqbc(
        r#"app "dup-handlers"
event.on("app.start") {}
event.on("key.SELECT") {}
screen("main") {}
"#,
    );
    duplicate_u16_field(&mut handler_bytes, SECTION_HANDLERS, 2, 18);
    assert_eq!(
        Program::parse(&handler_bytes).err(),
        Some(VmError::DuplicateTableKey)
    );

    let mut screen_bytes = compile_sqbc(
        r#"app "dup-screens"
event.on("app.start") {}
screen("main") {}
screen("alt") {}
"#,
    );
    duplicate_u16_field(&mut screen_bytes, SECTION_SCREENS, 2, 12);
    assert_eq!(
        Program::parse(&screen_bytes).err(),
        Some(VmError::DuplicateTableKey)
    );

    let mut trigger_bytes = compile_sqbc(
        r#"app "dup-triggers"
app.triggers {
  service.timer.every("timer.one", 1000)
  service.timer.every("timer.two", 2000)
}
event.on("app.start") {}
event.on("timer.one") {}
event.on("timer.two") {}
screen("main") {}
"#,
    );
    duplicate_u16_field(&mut trigger_bytes, SECTION_TRIGGERS, 2, 10);
    assert_eq!(
        Program::parse(&trigger_bytes).err(),
        Some(VmError::DuplicateTableKey)
    );
}

#[test]
fn rejects_sqbc_table_string_references_outside_string_table() {
    let mut bytes = fixture_counter_sqbc();
    let (handlers_offset, _) = find_section(&bytes, SECTION_HANDLERS);
    write_u16_at(&mut bytes, handlers_offset + 2, u16::MAX);

    assert_eq!(
        Program::parse(&bytes).err(),
        Some(VmError::InvalidStringRef)
    );

    let mut scratch = [0u8; 4096];
    assert_eq!(
        ProgramIndex::parse(&bytes, &mut scratch).err(),
        Some(VmError::InvalidStringRef)
    );
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
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
fn program_index_parses_trigger_metadata_without_app_arm_handler() {
    let source = r#"app "trigger-index"
event.on("app.start") {
  app.arm("break-reminder")
}
app.triggers {
  service.timer.after("timer.break", 1500000)
  service.timer.every("timer.stretch", 60000)
}
event.on("timer.break") {
  debug.print("break")
}
event.on("timer.stretch") {
  debug.print("stretch")
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    assert!(matches!(
        program.handler("app.arm"),
        Err(VmError::HandlerNotFound)
    ));

    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let triggers = index.trigger_timers().unwrap();
    assert_eq!(
        triggers,
        [
            TriggerTimer {
                event: "timer.break",
                interval_ms: 1500000,
                repeating: false,
            },
            TriggerTimer {
                event: "timer.stretch",
                interval_ms: 60000,
                repeating: true,
            },
        ]
    );

    let mut reader = SliceSqbcReader::new(&bytes);
    let mut small_scratch = [0u8; 160];
    assert_eq!(
        ProgramIndex::trigger_timer_count_from_reader(&mut reader, &mut small_scratch).unwrap(),
        2
    );
    let mut reader = SliceSqbcReader::new(&bytes);
    assert_eq!(
        ProgramIndex::trigger_timer_from_reader(&mut reader, &mut small_scratch, 1).unwrap(),
        TriggerTimer {
            event: "timer.stretch",
            interval_ms: 60000,
            repeating: true,
        }
    );
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
fn chunked_vm_resumable_dispatch_suspends_for_sqbc_chunk_read() {
    let bytes = fixture_counter_sqbc();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::pending(&bytes);
    let mut vm = ChunkedVm::new(index);

    let first = vm.dispatch_resumable(&mut reader, "app.start").unwrap();
    let VmDispatch::PendingStorage(request) = first else {
        panic!("expected sqbc read request, got {first:?}");
    };
    let StorageRequest::SqbcRead { offset, len } = request else {
        panic!("expected sqbc read request, got {request:?}");
    };
    assert!(len > 0);
    assert_eq!(reader.events, vec!["app.start"]);
    assert_eq!(reader.pending_read_count, 1);

    reader.pending_reads = false;
    let completion = StorageCompletion::bytes(&bytes[offset..offset + len]).unwrap();
    let second = vm.resume_storage(&mut reader, completion).unwrap();

    assert!(matches!(
        second,
        VmDispatch::PendingStorage(StorageRequest::StateLoad)
    ));
    assert_eq!(reader.pending_read_count, 1);
}

#[test]
fn chunked_vm_resumable_dispatch_suspends_state_load_without_replaying_side_effects() {
    let source = r#"app "pending-state"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  debug.print("loaded", state.count)
  state.count = state.count + 1
  state.save()
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::new(&bytes);
    let mut vm = ChunkedVm::new(index);

    let first = vm.dispatch_resumable(&mut reader, "app.start").unwrap();

    assert_eq!(
        first,
        VmDispatch::PendingStorage(StorageRequest::state_load())
    );
    assert_eq!(reader.events, vec!["app.start"]);

    let second = vm
        .resume_storage(&mut reader, StorageCompletion::empty())
        .unwrap();

    let VmDispatch::PendingStorage(request) = second else {
        panic!("expected state save request, got {second:?}");
    };
    assert!(matches!(request, StorageRequest::StateSave { len: 18, .. }));
    assert_eq!(
        reader.events,
        vec!["app.start", "state.load", "debug loaded 0"]
    );

    assert_eq!(
        vm.resume_storage(&mut reader, StorageCompletion::empty())
            .unwrap(),
        VmDispatch::Complete
    );
    assert_eq!(vm.state_value("count"), Ok(Value::I32(1)));
    assert_eq!(
        reader.events,
        vec!["app.start", "state.load", "debug loaded 0", "state.save"]
    );
}

#[test]
fn chunked_vm_resumable_screen_render_suspends_for_screen_chunk_read() {
    let source = r#"app "pending-screen"
event.on("app.start") {
  screen.open("main")
  debug.print("after")
}
screen("main") {
  service.display.clear(color.GRAY0)
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::pending(&bytes);
    let mut vm = ChunkedVm::new(index);

    let first = vm.dispatch_resumable(&mut reader, "app.start").unwrap();
    let VmDispatch::PendingStorage(StorageRequest::SqbcRead {
        offset: handler_offset,
        len: handler_len,
    }) = first
    else {
        panic!("expected handler sqbc read request, got {first:?}");
    };
    assert_eq!(reader.pending_read_count, 1);

    let second = vm
        .resume_storage(
            &mut reader,
            StorageCompletion::bytes(&bytes[handler_offset..handler_offset + handler_len]).unwrap(),
        )
        .unwrap();
    let VmDispatch::PendingStorage(StorageRequest::SqbcRead {
        offset: screen_offset,
        len: screen_len,
    }) = second
    else {
        panic!("expected screen sqbc read request, got {second:?}");
    };
    assert_ne!(
        screen_offset, handler_offset,
        "screen rendering must suspend for the screen chunk before reloading the handler"
    );
    assert_eq!(reader.pending_read_count, 2);
    assert_eq!(
        reader.events,
        vec!["app.start".to_string()],
        "screen draw callbacks must not run before the screen chunk request is completed"
    );

    reader.pending_reads = false;
    assert_eq!(
        vm.resume_storage(
            &mut reader,
            StorageCompletion::bytes(&bytes[screen_offset..screen_offset + screen_len]).unwrap(),
        )
        .unwrap(),
        VmDispatch::Complete
    );
    assert_eq!(
        reader.events,
        vec![
            "app.start".to_string(),
            "draw clear 0".to_string(),
            "debug after".to_string(),
        ]
    );
}

#[test]
fn chunked_vm_resumable_function_call_suspends_for_function_chunk_read() {
    let source = r#"app "pending-function"
function helper(value) {
  debug.print("helper", value)
  return value + 1
}
event.on("app.start") {
  debug.print("before")
  debug.print("after", helper(41))
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::pending(&bytes);
    let mut vm = ChunkedVm::new(index);

    let first = vm.dispatch_resumable(&mut reader, "app.start").unwrap();
    let VmDispatch::PendingStorage(StorageRequest::SqbcRead {
        offset: handler_offset,
        len: handler_len,
    }) = first
    else {
        panic!("expected handler sqbc read request, got {first:?}");
    };

    let second = vm
        .resume_storage(
            &mut reader,
            StorageCompletion::bytes(&bytes[handler_offset..handler_offset + handler_len]).unwrap(),
        )
        .unwrap();
    let VmDispatch::PendingStorage(StorageRequest::SqbcRead {
        offset: function_offset,
        len: function_len,
    }) = second
    else {
        panic!("expected function sqbc read request, got {second:?}");
    };
    assert_ne!(
        function_offset, handler_offset,
        "function calls must suspend for the function chunk instead of entering recursive dispatch"
    );
    assert_eq!(
        reader.events,
        vec!["app.start".to_string(), "debug before".to_string()],
        "callee side effects must not run before the function chunk request is completed"
    );

    reader.pending_reads = false;
    assert_eq!(
        vm.resume_storage(
            &mut reader,
            StorageCompletion::bytes(&bytes[function_offset..function_offset + function_len])
                .unwrap(),
        )
        .unwrap(),
        VmDispatch::Complete
    );
    assert_eq!(
        reader.events,
        vec![
            "app.start".to_string(),
            "debug before".to_string(),
            "debug helper 41".to_string(),
            "debug after 42".to_string(),
        ]
    );
}

#[test]
fn chunked_vm_resumable_function_storage_suspend_resumes_callee_then_caller() {
    let source = r#"app "pending-function-state"
state { count: int = 0 }
function helper() {
  state.load()
  debug.print("helper", state.count)
  return state.count + 1
}
event.on("app.start") {
  debug.print("before")
  debug.print("after", helper())
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::pending(&bytes);
    let mut vm = ChunkedVm::new(index);

    let first = vm.dispatch_resumable(&mut reader, "app.start").unwrap();
    let VmDispatch::PendingStorage(StorageRequest::SqbcRead {
        offset: handler_offset,
        len: handler_len,
    }) = first
    else {
        panic!("expected handler sqbc read request, got {first:?}");
    };

    let second = vm
        .resume_storage(
            &mut reader,
            StorageCompletion::bytes(&bytes[handler_offset..handler_offset + handler_len]).unwrap(),
        )
        .unwrap();
    let VmDispatch::PendingStorage(StorageRequest::SqbcRead {
        offset: function_offset,
        len: function_len,
    }) = second
    else {
        panic!("expected function sqbc read request, got {second:?}");
    };

    reader.pending_reads = false;
    assert_eq!(
        vm.resume_storage(
            &mut reader,
            StorageCompletion::bytes(&bytes[function_offset..function_offset + function_len])
                .unwrap(),
        )
        .unwrap(),
        VmDispatch::PendingStorage(StorageRequest::state_load())
    );
    assert_eq!(
        reader.events,
        vec!["app.start".to_string(), "debug before".to_string()]
    );

    assert_eq!(
        vm.resume_storage(&mut reader, StorageCompletion::empty())
            .unwrap(),
        VmDispatch::Complete
    );
    assert_eq!(
        reader.events,
        vec![
            "app.start".to_string(),
            "debug before".to_string(),
            "state.load".to_string(),
            "debug helper 0".to_string(),
            "debug after 1".to_string(),
        ]
    );
}

#[test]
fn chunked_vm_rejects_oversized_handler_chunk() {
    let strings = encode_strings(&["oversized", "app.start"]);
    let state = vec![0, 0];
    let functions = vec![0, 0];
    let mut handlers = Vec::new();
    push_u16(&mut handlers, 1);
    push_handler(&mut handlers, 1, false, 0, 0, 0, MAX_CODE_CHUNK_BYTES + 1);
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
  state.count = state.count + 1
}
function screenValue(value) {
  let next = value
  next = next + 1
  return next
}
event.on("app.start") {
  bump()
  screen.open("main")
}
screen("main") {
  debug.print("screen", screenValue(state.count))
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let mut scratch = [0u8; MAX_APP_BYTES];
    let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
    let mut reader = CountingReader::new(&bytes);
    let mut vm = ChunkedVm::new(index);

    vm.dispatch(&mut reader, "app.start").unwrap();

    assert_eq!(vm.state_value("count"), Ok(Value::I32(1)));
    assert_eq!(reader.events, vec!["app.start", "debug screen 2"]);
}

#[test]
fn in_memory_vm_preserves_stack_when_function_call_underflows() {
    let strings = encode_strings(&["underflow", "fail", "drain", "helper", "drained"]);
    let state = vec![0, 0];
    let mut functions = Vec::new();
    let mut code = Vec::new();
    let helper = code.len();
    code.push(OP_RETURN);
    let fail = code.len();
    code.push(OP_PUSH_INT);
    push_i32(&mut code, 7);
    code.push(OP_CALL_FUNCTION);
    push_u16(&mut code, 0);
    push_u16(&mut code, 3);
    code.push(OP_HALT);
    let drain = code.len();
    code.push(OP_POP);
    code.push(OP_PUSH_STRING);
    push_u16(&mut code, 4);
    code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_DEBUG_PRINT, 1, OP_HALT]);

    push_u16(&mut functions, 1);
    push_u16(&mut functions, 3);
    push_u16(&mut functions, 3);
    push_u16(&mut functions, 3);
    push_u32(&mut functions, helper as u32);
    push_u32(&mut functions, (fail - helper) as u32);

    let mut handlers = Vec::new();
    push_u16(&mut handlers, 2);
    push_handler(&mut handlers, 1, false, 0, 0, fail, drain - fail);
    push_handler(&mut handlers, 2, false, 0, 0, drain, code.len() - drain);

    let bytes = encode_container(vec![
        (SECTION_STRINGS, strings),
        (SECTION_STATE, state),
        (SECTION_FUNCTIONS, functions),
        (SECTION_HANDLERS, handlers),
        (SECTION_CODE, code),
    ]);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    assert_eq!(
        vm.dispatch("fail", &mut trace),
        Err(VmError::StackUnderflow)
    );
    vm.dispatch("drain", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "fail".to_string(),
            "drain".to_string(),
            "debug drained".to_string(),
        ]
    );
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
  state.led = service.indicator.read()
  service.indicator.toggle()
  state.led = service.indicator.read()
  service.indicator.breathe()
  service.indicator.blink()
  service.indicator.blink(120, 80)
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
            "indicator breathe",
            "indicator blink 500/500",
            "indicator blink 120/80",
        ]
    );
}

struct CountingReader<'a> {
    bytes: &'a [u8],
    reads: Vec<(usize, usize)>,
    events: Vec<String>,
    saved_state: Vec<u8>,
    pending_reads: bool,
    pending_read_count: usize,
}

impl<'a> CountingReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            reads: Vec::new(),
            events: Vec::new(),
            saved_state: Vec::new(),
            pending_reads: false,
            pending_read_count: 0,
        }
    }

    fn pending(bytes: &'a [u8]) -> Self {
        let mut reader = Self::new(bytes);
        reader.pending_reads = true;
        reader
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

    fn should_defer_read(&mut self, _offset: usize, _len: usize) -> Result<bool, VmError> {
        if self.pending_reads {
            self.pending_read_count += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl TraceSink for CountingReader<'_> {
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
                Value::String(_) => line.push_str(strings.value_str(*value).unwrap()),
                Value::I32(value) => line.push_str(&value.to_string()),
                Value::Bool(value) => line.push_str(&value.to_string()),
                Value::Null => line.push_str("null"),
                Value::Record(_) => line.push_str("<record>"),
                Value::List(_) => line.push_str("<list>"),
                Value::Handle(_) => line.push_str("<handle>"),
            }
        }
        self.events.push(format!("debug {line}"));
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

    fn draw_clear(&mut self, color: u8) {
        self.events.push(format!("draw clear {color}"));
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
  debug.print("timer", state.count)
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
fn dispatch_handles_service_ble_start_and_stop() {
    let source = r#"app "ble-install"
event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: { complete: "ble.file.complete", error: "ble.file.error" }
  })
}
event.on("done") {
  service.ble.stop()
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    vm.dispatch("done", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "service.ble.start sqbc-install",
            "done",
            "service.ble.stop",
        ]
    );
}

#[test]
fn dispatch_handles_service_http_start_and_stop() {
    let source = r#"app "http-upload"
event.on("app.start") {
  service.http.start("file-upload", {
    id: "binbook-upload",
    accept: [".binbook"],
    events: { complete: "http.file.complete" }
  })
}
event.on("done") {
  service.http.stop()
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    vm.dispatch("done", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "service.http.start binbook-upload",
            "done",
            "service.http.stop",
        ]
    );
}

#[test]
fn runs_binbook_handle_api_and_draws_drawable_from_real_bytecode() {
    let source = r#"app "binbook-smoke"
state { pageIndex: int = 0 }
event.on("app.start") {
  screen.open("main")
}
screen("main", { render: "stream" }) {
  let opened = binbook.open("books/sample.binbook")
  if (opened.ok) {
    let info = binbook.info(opened.book)
    let page = binbook.readPage(opened.book, state.pageIndex)
    if (page.ok) {
      service.display.draw(page.drawable)
      debug.print("pages", info.pageCount)
    }
  }
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "binbook.open books/sample.binbook",
            "binbook.info BinBook:7",
            "binbook.readPage BinBook:7 0",
            "draw.resource Handle(Handle { kind: Drawable, id: 11 }) 0 0 0 0",
            "debug pages 3",
        ]
    );
}

#[test]
fn runs_binbook_chapters_from_real_bytecode() {
    let source = r#"app "binbook-chapters"
event.on("app.start") {
  let opened = binbook.open("books/sample.binbook")
  if (opened.ok) {
    let chapters = binbook.chapters(opened.book, { offset: 0, limit: 8 })
    debug.print(chapters.ok, chapters.error, chapters.count, chapters.hasMore)
    for chapter in chapters.items max 8 {
      debug.print(chapter.index, chapter.title, chapter.pageIndex, chapter.level, chapter.type)
    }
  }
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "binbook.open books/sample.binbook",
            "binbook.chapters BinBook:7 0 8",
            "debug true null 3 true",
            "debug 0 Chapter One 0 0 3",
            "debug 1 Chapter Two 4 0 3",
        ]
    );
}

#[test]
fn runs_binbook_chapter_from_real_bytecode() {
    let source = r#"app "binbook-chapter"
event.on("app.start") {
  let opened = binbook.open("books/sample.binbook")
  if (opened.ok) {
    let info = binbook.info(opened.book)
    let chapter = binbook.chapter(opened.book, 1)
    debug.print(info.pageCount, info.chapterCount)
    debug.print(chapter.ok, chapter.error, chapter.index, chapter.title, chapter.pageIndex, chapter.level, chapter.type)
  }
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "binbook.open books/sample.binbook",
            "binbook.info BinBook:7",
            "binbook.chapter BinBook:7 1",
            "debug 3 3",
            "debug true null 1 Chapter Two 4 0 3",
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
  state.count = state.count + 1
  app.disarm("break-reminder")
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
fn runs_power_sleep_and_start_reason_builtins_from_real_bytecode() {
    let source = r#"app "power"
event.on("app.start") {
  debug.print(system.startReason())
  service.power.sleep({ wakeAfterMs: 30000 })
}
event.on("power.sleep") {
  debug.print("prep")
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    vm.dispatch("power.sleep", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "debug wake",
            "service.power.sleep 30000",
            "power.sleep",
            "debug prep",
        ]
    );
}

#[test]
fn runs_device_config_result_builtins_from_real_bytecode() {
    let source = r#"app "device-config"
event.on("app.start") {
  let loaded = device.config.load("package:device/indicator.sqdevice")
  let set = device.config.set("mode", "gpio")
  let rebound = device.config.rebind("indicator.default")
  let saved = device.config.save("flash")
  debug.print(loaded.ok, loaded.error, loaded.warning)
  debug.print(set.ok, set.error, rebound.ok, rebound.warning, saved.ok)
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "device.config.load package:device/indicator.sqdevice",
            "device.config.set mode gpio",
            "device.config.rebind indicator.default",
            "device.config.save flash",
            "debug true null loaded",
            "debug true null true rebound true",
        ]
    );
}

#[test]
fn runs_display_info_record_builtin_from_real_bytecode() {
    let source = r#"app "display-info"
event.on("app.start") {
  let info = display.info()
  debug.print(info.ok, info.available, info.status, info.binding, info.driver, info.transport)
  debug.print(info.width, info.height, info.colorModel, info.nativePixelFormat, info.defaultFontHeight)
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "display.info",
            "debug true true ready display.default ssd1306 i2c",
            "debug 78 40 mono MONO1_PACKED 8",
        ]
    );
}

#[test]
fn runs_file_pick_file_unsupported_result_from_real_bytecode() {
    let source = r#"app "file-picker"
event.on("app.start") {
  let picked = file.pickFile(".binbook")
  debug.print(picked.ok, picked.error, picked.path)
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "file.pickFile .binbook",
            "debug false unsupported null",
        ]
    );
}

#[test]
fn runs_file_read_unsupported_results_from_real_bytecode() {
    let source = r#"app "file-read"
event.on("app.start") {
  let text = file.readText("notes.txt")
  let lines = file.readLines("notes.txt", 4)
  debug.print(text.ok, text.error, text.text)
  debug.print(lines.ok, lines.error, lines.lines)
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "file.readText notes.txt",
            "file.readLines notes.txt 4",
            "debug false unsupported null",
            "debug false unsupported <list>",
        ]
    );
}

#[test]
fn runs_app_registry_list_and_get_from_real_bytecode() {
    let source = r#"app "launcher"
event.on("app.start") {
  let apps = app.registry()
  for appId in apps max 2 {
    debug.print(appId)
  }
  let selected = app.registry.get(apps, 1)
  debug.print(selected.id, selected.name, selected.build, selected.description)
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RegistryTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "registry.list",
            "debug main",
            "debug reader",
            "registry.get reader",
            "debug reader Reader dev-reader Read documents",
        ]
    );
}

#[test]
fn runs_file_copy_to_content_library_from_real_bytecode() {
    let source = r#"app "file-copy"
event.on("app.start") {
  let copied = file.copy("/sq/tmp/http-upload.binbook", {
    library: "books",
    name: "upload.binbook"
  })
  debug.print(copied.ok, copied.error, copied.ref, copied.bytesWritten)
}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "file.copy /sq/tmp/http-upload.binbook books upload.binbook",
            "debug true null content:books/r/upload.binbook 4096",
        ]
    );
}

#[test]
fn runs_content_binbook_list_from_real_bytecode() {
    let source = r#"app "library"
event.on("app.start") {
  let page = content.binbook.list("books", { offset: 0, limit: 8 })
  debug.print(page.ok, page.error, page.warning, page.count, page.hasMore)
  for item in page.items max 8 {
    debug.print(item.name, item.ref, item.size)
  }
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RegistryTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "content.binbook.list books 0 8",
            "debug true null sd-unavailable 7 true",
            "debug alpha.binbook content:books/p/alpha.binbook 4096",
            "debug travel-notes.binbook content:books/r/travel-notes.binbook 8192",
        ]
    );
}

#[test]
fn opens_content_binbook_ref_from_list_item_real_bytecode() {
    let source = r#"app "library-open"
event.on("app.start") {
  let page = content.binbook.list("books", { offset: 0, limit: 8 })
  for item in page.items max 1 {
    let opened = binbook.open(item.ref)
    debug.print(opened.ok, opened.error)
  }
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RegistryTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "content.binbook.list books 0 8",
            "binbook.open content:books/p/alpha.binbook",
            "debug true null",
        ]
    );
}

#[test]
fn runs_app_lifecycle_stack_inspection_from_real_bytecode() {
    let source = r#"app "launcher"
event.on("app.start") {
  let process = app.processStack()
  for appId in process max 2 {
    debug.print(appId)
  }
  let armed = app.armedStack()
  for armedApp in armed max 2 {
    debug.print(armedApp.appId)
  }
  let selected = app.armedStack.get(armed, 1)
  debug.print(selected.appId, selected.event)
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RegistryTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "process.stack",
            "debug main",
            "debug reader",
            "armed.stack",
            "debug break-reminder",
            "debug weather-sync",
            "debug weather-sync timer.sync",
        ]
    );
}

#[test]
fn display_info_static_strings_work_without_dynamic_entries() {
    let source = r#"app "display-budget"
event.on("app.start") {
  let first = display.info()
  let second = display.info()
  debug.print(first.status, second.nativePixelFormat)
}
"#;
    let bytes = compile_sqbc(&source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "display.info",
            "display.info",
            "debug ready MONO1_PACKED",
        ]
    );
}

#[test]
fn repeated_display_info_static_strings_do_not_consume_dynamic_entries() {
    let source = r#"app "display-over-budget"
event.on("app.start") {
  let first = display.info()
  let second = display.info()
  let third = display.info()
  debug.print(first.status, second.status, third.status)
}
"#;
    let bytes = compile_sqbc(&source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "display.info",
            "display.info",
            "display.info",
            "debug ready ready ready",
        ]
    );
}

#[test]
fn static_service_result_strings_do_not_consume_interned_dynamic_entries() {
    let mut source = String::from(
        r#"app "display-static-intern"
event.on("app.start") {
  let first = display.info()
  let last = first
"#,
    );
    for _ in 0..40 {
        let _ = writeln!(source, "  last = display.info()");
    }
    source.push_str(
        r#"  debug.print(first.status, last.nativePixelFormat)
}
"#,
    );
    let bytes = compile_sqbc(&source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events.last().map(String::as_str),
        Some("debug ready MONO1_PACKED")
    );
}

#[test]
fn wifi_scan_dynamic_strings_work_with_current_budget() {
    let source = r#"app "wifi-budget"
event.on("app.start") {
  wifi.scan()
  let result = wifi.result()
  debug.print(result.count)
}
"#;
    let bytes = compile_sqbc(&source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec!["app.start", "wifi.scan", "wifi.result", "debug 4"]
    );
}

#[test]
fn wifi_scan_static_strings_do_not_consume_dynamic_entries() {
    let source = r#"app "wifi-over-budget"
event.on("app.start") {
  wifi.scan()
  let result = wifi.result()
  debug.print(result.count)
  debug.print(system.startReason())
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "wifi.scan",
            "wifi.result",
            "debug 4",
            "debug wake"
        ]
    );
    assert_eq!(trace.wifi_teardown_count, 0);
}

#[test]
fn app_registry_dynamic_strings_work_with_current_budget() {
    let source = r#"app "registry-budget"
event.on("app.start") {
  let apps = app.registry()
  let selected = app.registry.get(apps, 0)
  debug.print(selected.id, selected.name, selected.build, selected.description)
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "registry.list",
            "registry.get app0",
            "debug app0 App 0 build0 Desc 0",
        ]
    );
}

#[test]
fn app_registry_dynamic_string_interning_leaves_room_for_system_text() {
    let source = r#"app "registry-over-budget"
event.on("app.start") {
  let apps = app.registry()
  let selected = app.registry.get(apps, 0)
  debug.print(selected.id)
  debug.print(system.memory())
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "registry.list",
            "registry.get app0",
            "debug app0",
            "debug RAM 292 KiB",
        ]
    );
}

#[test]
fn file_read_lines_dynamic_strings_work_with_current_budget() {
    let source = r#"app "file-budget"
event.on("app.start") {
  let first = file.readLines("a.txt", 4)
  let second = file.readLines("b.txt", 4)
  let third = file.readLines("c.txt", 4)
  debug.print(first.lines, second.lines, third.lines)
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "file.readLines a.txt 4",
            "file.readLines b.txt 4",
            "file.readLines c.txt 4",
            "debug <list> <list> <list>",
        ]
    );
}

#[test]
fn file_read_lines_dynamic_string_interning_leaves_room_for_system_text() {
    let source = r#"app "file-over-budget"
event.on("app.start") {
  let first = file.readLines("a.txt", 4)
  let second = file.readLines("b.txt", 4)
  let third = file.readLines("c.txt", 4)
  debug.print(first.lines, second.lines, third.lines)
  debug.print(system.memory())
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "file.readLines a.txt 4",
            "file.readLines b.txt 4",
            "file.readLines c.txt 4",
            "debug <list> <list> <list>",
            "debug RAM 292 KiB",
        ]
    );
}

#[test]
fn dynamic_service_string_arena_fails_cleanly_when_exhausted() {
    let source = r#"app "service-arena-over-budget"
event.on("app.start") {
  let apps = app.registry()
  let e0 = app.registry.get(apps, 0)
  let e1 = app.registry.get(apps, 1)
  let e2 = app.registry.get(apps, 2)
  let e3 = app.registry.get(apps, 3)
  let e4 = app.registry.get(apps, 4)
  let e5 = app.registry.get(apps, 5)
  let e6 = app.registry.get(apps, 6)
  let e7 = app.registry.get(apps, 7)
  debug.print(system.memory())
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::TooManyStrings)
    );
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "registry.list",
            "registry.get app0",
            "registry.get app1",
            "registry.get app2",
            "registry.get app3",
            "registry.get app4",
            "registry.get app5",
            "registry.get app6",
            "registry.get app7",
            "wifi.teardown",
        ]
    );
}

#[test]
fn string_concatenation_dynamic_strings_work_with_current_budget() {
    let source = r#"app "concat-budget"
event.on("app.start") {
  let s0 = system.startReason()
  let s1 = s0 + "x"
  let s2 = s1 + "x"
  let s3 = s2 + "x"
  let s4 = s3 + "x"
  let s5 = s4 + "x"
  let s6 = s5 + "x"
  let s7 = s6 + "x"
  let s8 = s7 + "x"
  let s9 = s8 + "x"
  let s10 = s9 + "x"
  let s11 = s10 + "x"
  debug.print(s11)
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert_eq!(trace.events, vec!["app.start", "debug wakexxxxxxxxxxx"]);
}

#[test]
fn string_concatenation_dynamic_strings_fail_cleanly_over_event_budget() {
    let mut source = String::from(
        r#"app "concat-over-budget"
event.on("app.start") {
  let s0 = system.startReason()
"#,
    );
    for _ in 0..32 {
        let _ = writeln!(source, "  s0 = s0 + \"x\"");
    }
    source.push_str(
        r#"
  debug.print(s0)
}
"#,
    );
    let bytes = compile_sqbc(&source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::TooManyStrings)
    );
    assert_eq!(trace.events, vec!["app.start", "wifi.teardown"]);
}

#[test]
fn dynamic_strings_are_reclaimed_between_events_after_budget_exhaustion() {
    let mut source = String::from(
        r#"app "budget-reclaim"
event.on("app.start") {
  let s0 = system.startReason()
"#,
    );
    for _ in 0..32 {
        let _ = writeln!(source, "  s0 = s0 + \"x\"");
    }
    source.push_str(
        r#"
  debug.print(s0)
}

event.on("timer.clock") {
  let first = display.info()
  let second = display.info()
  debug.print(first.status, second.status)
}
"#,
    );
    let bytes = compile_sqbc(&source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::TooManyStrings)
    );
    vm.dispatch("timer.clock", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "wifi.teardown",
            "timer.clock",
            "display.info",
            "display.info",
            "debug ready ready",
        ]
    );
}

#[test]
fn service_result_string_assigned_to_state_survives_event_cleanup() {
    let source = r#"app "service-state"
state {
  label: string = ""
}
event.on("app.start") {
  let info = display.info()
  state.label = info.status
}

event.on("timer.clock") {
  debug.print(state.label)
}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = BudgetTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    vm.dispatch("timer.clock", &mut trace).unwrap();

    assert_eq!(
        trace.events,
        vec!["app.start", "display.info", "timer.clock", "debug ready"]
    );
}

#[test]
fn repeated_armed_stack_inspection_from_timer_does_not_exhaust_dynamic_strings() {
    let source = r#"app "reader"
event.on("timer.clock") {
  let armed = app.armedStack()
  for armedApp in armed max 2 {
    debug.print("armed", armedApp.appId, armedApp.event)
  }
  let selected = app.armedStack.get(armed, 0)
  debug.print("selected", selected.appId, selected.event)
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RegistryTrace::default();

    for tick in 0..10 {
        vm.dispatch("timer.clock", &mut trace)
            .unwrap_or_else(|err| panic!("timer.clock tick {tick} failed: {err:?}"));
    }

    assert_eq!(
        trace
            .events
            .iter()
            .filter(|event| event.as_str() == "armed.stack")
            .count(),
        10
    );
    assert!(trace
        .events
        .iter()
        .any(|event| { event.as_str() == "debug selected break-reminder timer.break" }));
}

#[test]
fn event_runtime_string_cleanup_preserves_string_state_values() {
    let source = r#"app "stateful"
state {
  label: string = ""
}
event.on("app.start") {
  state.label = system.memory()
  debug.print("saved", state.label)
}
event.on("timer.clock") {
  let armed = app.armedStack()
  for armedApp in armed max 2 {
    debug.print("armed", armedApp.appId, armedApp.event)
  }
  debug.print("state", state.label)
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = RuntimeTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();
    for tick in 0..10 {
        vm.dispatch("timer.clock", &mut trace)
            .unwrap_or_else(|err| panic!("timer.clock tick {tick} failed: {err:?}"));
    }

    assert!(trace
        .events
        .iter()
        .any(|event| event.as_str() == "debug state RAM 292 KiB"));
}

#[test]
fn runs_wifi_ap_records_and_tears_down_on_exit() {
    let source = r#"app "wifi-ap"
state {}

event.on("app.start") {
  let ap = service.wifi.startAP("SquidScript")
  let status = wifi.status()
  let ip = service.wifi.getAPIP()
  debug.print(ap.ok, status.active, status.mode, status.ipAddress, status.ssid, ip.ip)
  app.exit()
}

screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = WifiTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert!(vm.exited());
    assert!(!trace.active);
    assert_eq!(trace.teardown_count, 1);
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "wifi.startAP SquidScript",
            "wifi.status",
            "wifi.getAPIP",
            "debug true true ap 192.168.4.1 SquidScript 192.168.4.1",
            "wifi.teardown",
            "app.exit",
        ]
    );
}

#[test]
fn exposes_wifi_driver_state_fields_in_status_record() {
    let source = r#"app "wifi-status"
state {}

event.on("app.start") {
  service.wifi.startAP("SquidScript")
  let status = service.wifi.status()
  debug.print(status.state, status.backend, status.driverStarted, status.configured, status.driverMode, status.channel, status.apStartEvents, status.probeEvents, status.lastBackendCode)
  app.exit()
}

screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = WifiTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert!(trace
        .events
        .contains(&"debug started sim true true ap 1 1 0 null".to_string()));
}

#[test]
fn runs_wifi_station_profile_records_and_disconnects_on_exit() {
    let source = r#"app "wifi-sta"
state {}

event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let status = wifi.status()
  debug.print(connect.ok, status.profile, status.connected, status.scanMatches, status.rssi, status.auth, status.bssid)
  app.exit()
}

screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = WifiTrace::default();

    vm.dispatch("app.start", &mut trace).unwrap();

    assert!(vm.exited());
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "wifi.connect dev",
            "wifi.status",
            "debug true dev true 1 -42 wpa2 00:11:22:33:44:55",
            "wifi.teardown",
            "app.exit",
        ]
    );
}

#[test]
fn runs_wifi_scan_record_and_bounded_network_records() {
    let source = r#"app "wifi-scan"
state {}

event.on("app.start") {
  let started = wifi.scan()
  let result = wifi.result()
  let network = wifi.scanNetwork(0)
  debug.print(started.ok, result.error, result.count)
  debug.print(network.ssid, network.ssidLength, network.channel, network.rssi, network.auth, network.hidden)
  debug.print(network.password)
  app.exit()
}

screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = WifiTrace::default();

    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::InvalidOperand),
        "credential fields must not exist"
    );

    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "wifi.scan",
            "wifi.result",
            "wifi.scanNetwork 0",
            "debug true null 2",
            "debug LabNetwork 10 6 -42 wpa2 false",
            "wifi.teardown",
        ]
    );
}

#[test]
fn runs_async_wifi_operation_poll_result_cancel_and_scan_network() {
    let source = r#"app "wifi-async-runtime"
state {}

event.on("app.start") {
  let started = wifi.scan()
  let op = wifi.operation()
  let result = wifi.result()
  let row = wifi.scanNetwork(0)
  let cancel = wifi.cancel()
  debug.print(started.active, started.kind, op.state, result.ready, result.count, row.ok, row.ssidLength, cancel.cancelled)
}

screen("main") {}
"#;
    let bytes = compile_sqbc(source);
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = WifiTrace::default();
    vm.dispatch("app.start", &mut trace).unwrap();
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "wifi.scan",
            "wifi.operation",
            "wifi.result",
            "wifi.scanNetwork 0",
            "wifi.cancel",
            "debug true scan done true 2 true 10 true",
        ]
    );
}

#[test]
fn tears_down_wifi_ap_on_runtime_error() {
    let source = r#"app "wifi-crash"
state {}

event.on("app.start") {
  service.wifi.startAP("SquidScript")
  debug.print(wifi.status().missing)
}

screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
    let program = Program::parse(&bytes).unwrap();
    let mut vm = Vm::new(program);
    let mut trace = WifiTrace::default();

    assert_eq!(
        vm.dispatch("app.start", &mut trace),
        Err(VmError::InvalidOperand)
    );

    assert!(!trace.active);
    assert_eq!(trace.teardown_count, 1);
    assert_eq!(
        trace.events,
        vec![
            "app.start",
            "wifi.startAP SquidScript",
            "wifi.status",
            "wifi.teardown",
        ]
    );
}

#[test]
fn state_reset_restores_typed_defaults() {
    let source = r#"app "reset-demo"
state {
  count: int = 4
}
event.on("app.start") {
  state.count = state.count + 1
  state.reset()
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
  state.count = state.label + 1
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let bytes = squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap();
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
    push_handler(&mut handlers, 3, false, 0, 0, on_start, select - on_start);
    push_handler(&mut handlers, 4, false, 0, 0, select, back - select);
    push_handler(&mut handlers, 5, false, 0, 0, back, code.len() - back);

    encode_container(vec![
        (SECTION_STRINGS, strings),
        (SECTION_STATE, state),
        (SECTION_FUNCTIONS, functions),
        (SECTION_HANDLERS, handlers),
        (SECTION_CODE, code),
    ])
}

fn compile_sqbc(source: &str) -> Vec<u8> {
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap()
}

fn mismatched_count_state_record() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(STATE_RECORD_MAGIC);
    out.push(1);
    out.push(5);
    out.extend_from_slice(b"count");
    out.push(STATE_TYPE_BOOL);
    out.push(0);
    out.push(VALUE_BOOL);
    out.push(1);
    out
}

fn string_state_record(name: &str, value: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(STATE_RECORD_MAGIC);
    out.push(1);
    out.push(name.len() as u8);
    out.extend_from_slice(name.as_bytes());
    out.push(STATE_TYPE_STRING);
    out.push(0);
    out.push(VALUE_STRING);
    out.push(value.len() as u8);
    out.extend_from_slice(value.as_bytes());
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
    let header_len = 14 + sections.len() * 12;
    let file_len = header_len + sections.iter().map(|(_, data)| data.len()).sum::<usize>();
    let mut out = Vec::new();
    out.extend_from_slice(b"SQBC");
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

fn find_section(bytes: &[u8], kind: u16) -> (usize, usize) {
    let header = Program::parse_header(bytes).unwrap();
    for index in 0..header.section_count {
        let section = Program::parse_section_record(&bytes[..header.header_len], index).unwrap();
        if section.kind == kind {
            return (section.offset, section.len);
        }
    }
    panic!("missing section {kind}");
}

fn duplicate_u16_field(
    bytes: &mut [u8],
    kind: u16,
    source_relative: usize,
    target_relative: usize,
) {
    let (section_offset, _) = find_section(bytes, kind);
    let value = read_u16(bytes, section_offset + source_relative).unwrap();
    write_u16_at(bytes, section_offset + target_relative, value);
}

fn write_u16_at(out: &mut [u8], offset: usize, value: u16) {
    out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_handler(
    out: &mut Vec<u8>,
    event_id: u16,
    preload: bool,
    param_count: u16,
    local_count: u16,
    start: usize,
    len: usize,
) {
    push_u16(out, event_id);
    out.push(u8::from(preload));
    out.push(0);
    push_u16(out, param_count);
    push_u16(out, local_count);
    push_u32(out, start as u32);
    push_u32(out, len as u32);
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
