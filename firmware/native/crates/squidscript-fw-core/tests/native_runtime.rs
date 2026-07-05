use squidc_core::compile::{compile, CompileRequest};
use squidscript_fw_core::{
    native_runtime::{
        BoundedNativeFileBackend, NativeBinBookBackend, NativeDisplaySink, NativeFileBackend,
        NativeFileStorage, NativeFileStorageError, NativeRadioBackend, NativeRuntime,
        NativeRuntimeError, NativeWifiApIp, NativeWifiStatus, NoopBinBookBackend,
        NoopRadioBackend,
    },
    radio_lifecycle::RadioKind,
};
use squidvm_core::{
    host::{
        BinBookChapterEntry, BinBookChapterListSummary, BinBookChapterListWriter,
        BinBookChapterResult, BinBookInfoResult, BinBookOpenResult, BinBookReadPageResult,
        ContentBinBookEntry, ContentBinBookListResult, FileReadLinesResult, FileReadTextResult,
        WifiAccessPoint,
    },
    value::{Handle, HandleKind},
};
use std::vec::Vec;

fn compile_sqbc(source: &str) -> Vec<u8> {
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap()
}

fn run_temp_app<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    F: NativeFileBackend,
>(
    runtime: &mut NativeRuntime<B, D, C, F>,
    app_id: &str,
    sqbc: &[u8],
) {
    runtime.begin_temp_run(app_id, sqbc.len()).unwrap();
    runtime.write_temp_run_chunk(0, sqbc).unwrap();
    runtime.commit_temp_run().unwrap();
}

fn install_app<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    F: NativeFileBackend,
>(
    runtime: &mut NativeRuntime<B, D, C, F>,
    app_id: &str,
    sqbc: &[u8],
) {
    runtime.begin_app_install(app_id, sqbc.len()).unwrap();
    runtime.write_app_install_chunk(0, sqbc).unwrap();
    runtime.commit_app_install().unwrap();
}

#[test]
fn temp_run_dispatches_app_start_and_records_diagnostics() {
    let sqbc = compile_sqbc(
        r#"app "native-temp"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  debug.print("native", state.count)
  state.save()
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-temp", &sqbc);

    let output = runtime.output_lines();
    assert_eq!(output.as_slice(), &["native 1"]);
    let trace = runtime.trace_lines();
    assert_eq!(trace.as_slice(), &["app.start", "state.load", "state.save"]);
    assert!(!runtime.state_bytes().is_empty());
    assert_eq!(runtime.active_app(), Some("native-temp"));
    assert_eq!(
        runtime.lifecycle_lines().as_slice()[0],
        "active=native-temp"
    );
}

#[test]
fn reset_clears_temp_app_and_diagnostics() {
    let sqbc = compile_sqbc(
        r#"app "native-temp"
event.on("app.start") { debug.print("hello") }
"#,
    );
    let mut runtime = NativeRuntime::new();
    run_temp_app(&mut runtime, "native-temp", &sqbc);

    runtime.reset();

    assert_eq!(runtime.active_app(), None);
    assert!(runtime.output_lines().as_slice().is_empty());
    assert!(runtime.trace_lines().as_slice().is_empty());
    assert!(runtime.state_bytes().is_empty());
}

#[test]
fn installed_app_launches_from_dedicated_slot() {
    let sqbc = compile_sqbc(
        r#"app "installed"
event.on("app.start") { debug.print("installed start") }
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed", &sqbc);
    runtime.launch_app("installed").unwrap();

    assert_eq!(runtime.output_lines().as_slice(), &["installed start"]);
    assert_eq!(runtime.active_app(), Some("installed"));
    assert_eq!(runtime.installed_app(), Some(("installed", sqbc.len())));
}

#[test]
fn installed_app_state_survives_reset_and_relaunch() {
    let sqbc = compile_sqbc(
        r#"app "installed-state"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  debug.print("count", state.count)
  state.save()
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed-state", &sqbc);
    runtime.launch_app("installed-state").unwrap();
    assert_eq!(runtime.output_lines().as_slice(), &["count 1"]);

    runtime.reset();
    runtime.launch_app("installed-state").unwrap();

    assert_eq!(runtime.output_lines().as_slice(), &["count 2"]);
    assert_eq!(runtime.active_app(), Some("installed-state"));
}

#[test]
fn installed_app_dispatches_key_and_named_events() {
    let sqbc = compile_sqbc(
        r#"app "installed-events"
event.on("app.start") { debug.print("start") }
event.on("key.SELECT") { debug.print("select") }
event.on("repl") { debug.print("repl") }
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed-events", &sqbc);
    runtime.launch_app("installed-events").unwrap();
    runtime.dispatch_event("key.SELECT").unwrap();
    runtime
        .dispatch_app_event("installed-events", "repl")
        .unwrap();

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["start", "select", "repl"]
    );
}

#[test]
fn imported_state_is_visible_to_next_installed_launch() {
    let sqbc = compile_sqbc(
        r#"app "installed-import"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  debug.print("count", state.count)
  state.save()
}
"#,
    );
    let mut source_runtime = NativeRuntime::new();
    install_app(&mut source_runtime, "installed-import", &sqbc);
    source_runtime.launch_app("installed-import").unwrap();
    let saved = source_runtime.state_bytes().to_vec();

    let mut runtime = NativeRuntime::new();
    install_app(&mut runtime, "installed-import", &sqbc);
    runtime.import_state(&saved).unwrap();
    runtime.launch_app("installed-import").unwrap();

    assert_eq!(runtime.output_lines().as_slice(), &["count 2"]);
}

#[test]
fn fresh_runtime_reports_inactive_lifecycle() {
    let runtime = NativeRuntime::new();

    assert_eq!(
        runtime.lifecycle_lines().as_slice(),
        &["active=", "armed_stack="]
    );
}

#[test]
fn temp_run_rejects_oversize_payloads() {
    let mut runtime = NativeRuntime::new();

    let error = runtime
        .begin_temp_run(
            "too-large",
            squidscript_fw_core::native_runtime::MAX_TEMP_SQBC_BYTES + 1,
        )
        .unwrap_err();

    assert_eq!(error, NativeRuntimeError::TooLarge);
}

#[test]
fn resources_report_vm_and_temp_run_state() {
    let sqbc = compile_sqbc(
        r#"app "native-temp"
event.on("app.start") { debug.print("hello") }
"#,
    );
    let mut runtime = NativeRuntime::new();
    run_temp_app(&mut runtime, "native-temp", &sqbc);

    let resources = runtime.resource_metrics();

    assert!(resources
        .iter()
        .any(|metric| metric.key == "runtime_current_app_present" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "vm_sqbc_chunk_bytes" && metric.value > 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "runtime_static_bytes" && metric.value > 0));
}

#[test]
fn screen_open_records_native_display_drawlog() {
    let sqbc = compile_sqbc(
        r#"app "native-display"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
  service.display.text("hello", {
    x: 4
    y: 8
    w: 120
    h: 24
    fontHeight: 16
    textColor: color.BLACK
    backgroundColor: color.WHITE
  })
  service.display.rect(1, 2, 30, 40, {
    fillColor: color.GRAY8
    strokeColor: color.BLACK
  })
  service.display.line(5, 6, 7, 8, {
    color: color.GRAY4
  })
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-display", &sqbc);

    assert_eq!(
        runtime.drawlog_lines().as_slice(),
        &[
            "clear 0",
            "text hello x=4 y=8 w=120 h=24 font=16 fg=15 bg=0",
            "rect x=1 y=2 w=30 h=40 fill=8 stroke=15",
            "line x1=5 y1=6 x2=7 y2=8 color=4",
        ]
    );
}

#[test]
fn display_info_reports_native_x4_display_profile() {
    let sqbc = compile_sqbc(
        r#"app "native-display-info"
event.on("app.start") {
  let info = display.info()
  debug.print(info.ok, info.available, info.status, info.driver, info.width, info.height)
  debug.print(info.physicalWidth, info.physicalHeight, info.rotation, info.nativePixelFormat)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-display-info", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "true true ready xteink-x4-display 480 800",
            "800 480 270 GRAY2_PACKED",
        ]
    );
}

#[test]
fn display_resource_operations_record_native_drawlog() {
    let sqbc = compile_sqbc(
        r#"app "native-display-resources"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.select("status")
  service.display.image("data/icon.bmp", {
    x: 20
    y: 24
  })
  service.display.draw("drawable/page", {
    x: 0
    y: 0
  })
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-display-resources", &sqbc);

    assert_eq!(
        runtime.drawlog_lines().as_slice(),
        &[
            "select status",
            "image data/icon.bmp x=20 y=24 w=0 h=0",
            "draw drawable/page x=0 y=0 w=0 h=0",
        ]
    );
}

#[derive(Default)]
struct FakeFileBackend {
    read_text_calls: usize,
    read_lines_calls: usize,
}

const TEST_NOTE_LINES: [&str; 2] = ["alpha", "beta"];

impl NativeFileBackend for FakeFileBackend {
    fn file_read_text<'a>(
        &'a mut self,
        path: &str,
    ) -> Result<FileReadTextResult<'a>, squidvm_core::error::VmError> {
        self.read_text_calls += 1;
        assert_eq!(path, "notes/status.txt");
        Ok(FileReadTextResult {
            ok: true,
            error: None,
            text: Some("ready"),
        })
    }

    fn file_read_lines<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<FileReadLinesResult<'a>, squidvm_core::error::VmError> {
        self.read_lines_calls += 1;
        assert_eq!(path, "notes/list.txt");
        assert_eq!(max_lines, 2);
        Ok(FileReadLinesResult {
            ok: true,
            error: None,
            lines: &TEST_NOTE_LINES,
        })
    }
}

#[test]
fn native_file_backend_drives_file_read_text_and_lines() {
    let sqbc = compile_sqbc(
        r#"app "native-file"
event.on("app.start") {
  let text = file.readText("notes/status.txt")
  debug.print("text", text.ok, text.text)
  let lines = file.readLines("notes/list.txt", 2)
  debug.print("lines", lines.ok)
  for line in lines.lines max 2 {
    debug.print("line", line)
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FakeFileBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["text true ready", "lines true", "line alpha", "line beta",]
    );
    let backend = runtime.file_backend();
    assert_eq!(backend.read_text_calls, 1);
    assert_eq!(backend.read_lines_calls, 1);
}

#[derive(Default)]
struct StaticFileStorage {
    reads: usize,
    copied: Vec<u8>,
    published: Vec<u8>,
    published_path: Option<String>,
    deleted: Vec<String>,
    formatted: bool,
}

impl NativeFileStorage for StaticFileStorage {
    fn for_each_file(
        &mut self,
        visit: &mut dyn FnMut(&str, u64),
    ) -> Result<(), NativeFileStorageError> {
        visit("books/readme.binbook", 4096);
        visit("notes/status.txt", 5);
        visit("notes/list.txt", 17);
        Ok(())
    }

    fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError> {
        match path {
            "notes/status.txt" => Ok(5),
            "notes/list.txt" => Ok(17),
            "books/copied.txt" if !self.copied.is_empty() => Ok(self.copied.len() as u64),
            path if Some(path) == self.published_path.as_deref() => Ok(self.published.len() as u64),
            _ => Err(NativeFileStorageError::NotFound),
        }
    }

    fn read_at(
        &mut self,
        path: &str,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), NativeFileStorageError> {
        self.reads += 1;
        let source: &[u8] = match path {
            "notes/status.txt" => b"ready",
            "notes/list.txt" => b"alpha\nbeta\ngamma\n",
            "books/copied.txt" if !self.copied.is_empty() => &self.copied,
            path if Some(path) == self.published_path.as_deref() => &self.published,
            _ => return Err(NativeFileStorageError::NotFound),
        };
        let offset = offset as usize;
        let available = source.len().saturating_sub(offset);
        let read_len = available.min(out.len());
        out[..read_len].copy_from_slice(&source[offset..offset + read_len]);
        for byte in out.iter_mut().skip(read_len) {
            *byte = 0;
        }
        Ok(())
    }

    fn create_or_truncate(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if path == "books/copied.txt" {
            self.copied.clear();
            return Ok(());
        }
        if path.starts_with("books/") && path.ends_with(".binbook") {
            self.published_path = Some(path.to_string());
            self.published.clear();
            return Ok(());
        }
        {
            return Err(NativeFileStorageError::InvalidName);
        }
    }

    fn write_at(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<(), NativeFileStorageError> {
        if path == "books/copied.txt" && offset as usize == self.copied.len() {
            self.copied.extend_from_slice(data);
            return Ok(());
        }
        if Some(path) == self.published_path.as_deref() && offset as usize == self.published.len() {
            self.published.extend_from_slice(data);
            return Ok(());
        }
        {
            return Err(NativeFileStorageError::InvalidName);
        }
    }

    fn flush(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if path == "books/copied.txt" || Some(path) == self.published_path.as_deref() {
            Ok(())
        } else {
            Err(NativeFileStorageError::InvalidName)
        }
    }

    fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if Some(path) == self.published_path.as_deref() {
            self.deleted.push(path.to_string());
            self.published_path = None;
            self.published.clear();
            return Ok(());
        }
        Err(NativeFileStorageError::NotFound)
    }

    fn format(&mut self) -> Result<(), NativeFileStorageError> {
        self.formatted = true;
        self.copied.clear();
        self.published.clear();
        self.published_path = None;
        Ok(())
    }
}

#[test]
fn bounded_native_file_backend_reads_text_and_lines_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-file"
event.on("app.start") {
  let text = file.readText("notes/status.txt")
  debug.print("text", text.ok, text.error, text.text)
  let lines = file.readLines("notes/list.txt", 2)
  debug.print("lines", lines.ok, lines.error)
  for line in lines.lines max 2 {
    debug.print("line", line)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-file", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "text true null ready",
            "lines true null",
            "line alpha",
            "line beta",
        ]
    );
    assert_eq!(runtime.file_backend().storage().reads, 2);
}

#[test]
fn bounded_native_file_backend_picks_file_by_extension_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-picker"
event.on("app.start") {
  let picked = file.pickFile(".txt")
  debug.print("pick", picked.ok, picked.error, picked.path)
  if picked.ok {
    let text = file.readText(picked.path)
    debug.print("text", text.ok, text.error, text.text)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-picker", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["pick true null notes/status.txt", "text true null ready",]
    );
}

#[test]
fn bounded_native_file_backend_copies_file_with_bounded_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-copy"
event.on("app.start") {
  let copied = file.copy("notes/status.txt", {
    library: "books"
    name: "copied.txt"
  })
  debug.print("copy", copied.ok, copied.error, copied.ref, copied.bytesWritten)
  if copied.ok {
    let text = file.readText(copied.ref)
    debug.print("text", text.ok, text.error, text.text)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-copy", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["copy true null books/copied.txt 5", "text true null ready",]
    );
}

#[test]
fn bounded_native_file_backend_lists_binbook_content_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-content"
event.on("app.start") {
  let listing = content.binbook.list("books", { offset: 0, limit: 2 })
  debug.print("list", listing.ok, listing.error, listing.count, listing.hasMore)
  for item in listing.items max 2 {
    debug.print("item", item.name, item.ref, item.size)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-content", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "list true null 1 false",
            "item readme.binbook books/readme.binbook 4096",
        ]
    );
}

#[test]
fn bounded_native_file_backend_lists_generic_library_files_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-file-list"
event.on("app.start") {
  let listing = file.list("books", { offset: 0, limit: 4 })
  debug.print("list", listing.ok, listing.error, listing.count, listing.hasMore)
  for item in listing.items max 4 {
    debug.print("item", item.name, item.ref, item.size)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-file-list", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "list true null 1 false",
            "item readme.binbook books/readme.binbook 4096",
        ]
    );
}

#[test]
fn bounded_native_file_backend_publishes_content_file_with_bounded_chunks() {
    let mut file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());

    let path = file_backend
        .content_install_begin("proof.binbook", 8)
        .unwrap();
    assert_eq!(path, "books/proof.binbook");
    file_backend
        .content_install_chunk("books/proof.binbook", 0, b"ABCD")
        .unwrap();
    file_backend
        .content_install_chunk("books/proof.binbook", 4, b"EFGH")
        .unwrap();
    file_backend
        .content_install_commit("books/proof.binbook")
        .unwrap();

    assert_eq!(
        file_backend.storage().published_path.as_deref(),
        Some("books/proof.binbook")
    );
    assert_eq!(file_backend.storage().published.as_slice(), b"ABCDEFGH");
}

#[test]
fn bounded_native_file_backend_checks_published_content_size_and_crc32() {
    let mut file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());

    let path = file_backend
        .content_install_begin("proof.binbook", 8)
        .unwrap()
        .to_string();
    file_backend
        .content_install_chunk(&path, 0, b"ABCD")
        .unwrap();
    file_backend
        .content_install_chunk(&path, 4, b"EFGH")
        .unwrap();
    file_backend.content_install_commit(&path).unwrap();

    let checked = file_backend.content_check("proof.binbook").unwrap();

    assert_eq!(checked.name, "proof.binbook");
    assert_eq!(checked.size, 8);
    assert_eq!(checked.crc32, 0x68dc_b61c);
}

#[test]
fn bounded_native_file_backend_deletes_published_content_by_simple_name() {
    let mut file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());

    let path = file_backend
        .content_install_begin("proof.binbook", 4)
        .unwrap()
        .to_string();
    file_backend
        .content_install_chunk(&path, 0, b"ABCD")
        .unwrap();
    file_backend.content_install_commit(&path).unwrap();

    let deleted = file_backend.content_delete("proof.binbook").unwrap();

    assert_eq!(deleted, "proof.binbook");
    assert_eq!(file_backend.storage().deleted, ["books/proof.binbook"]);
    assert_eq!(
        file_backend.content_check("proof.binbook"),
        Err("not-found")
    );
}

#[derive(Default)]
struct FileBackedBinBookBackend {
    open_calls: usize,
    info_calls: usize,
    read_page_calls: usize,
    chapters_calls: usize,
    chapter_calls: usize,
}

impl NativeFileBackend for FileBackedBinBookBackend {
    fn binbook_open<'a>(
        &'a mut self,
        path: &str,
    ) -> Result<BinBookOpenResult<'a>, squidvm_core::error::VmError> {
        self.open_calls += 1;
        assert_eq!(path, "books/readme.binbook");
        Ok(BinBookOpenResult {
            ok: true,
            error: None,
            book: Some(Handle::new(HandleKind::BinBook, 4)),
        })
    }

    fn binbook_info<'a>(
        &'a mut self,
        book: Handle,
    ) -> Result<BinBookInfoResult<'a>, squidvm_core::error::VmError> {
        self.info_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        Ok(BinBookInfoResult {
            ok: true,
            error: None,
            title: Some("Storage Book"),
            page_count: 12,
            chapter_count: 3,
        })
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, squidvm_core::error::VmError> {
        self.read_page_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        assert_eq!(page_index, 0);
        Ok(BinBookReadPageResult {
            ok: true,
            error: None,
            drawable: Some(Handle::new(HandleKind::Drawable, 2)),
        })
    }

    fn binbook_chapters_into<'a>(
        &'a mut self,
        book: Handle,
        offset: i32,
        limit: i32,
        writer: &mut dyn BinBookChapterListWriter,
    ) -> Result<BinBookChapterListSummary<'a>, squidvm_core::error::VmError> {
        self.chapters_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        assert_eq!(offset, 0);
        assert_eq!(limit, 8);
        writer.push_entry(BinBookChapterEntry {
            index: 0,
            title: "Start",
            page_index: 0,
            level: 0,
            entry_type: 3,
        })?;
        writer.push_entry(BinBookChapterEntry {
            index: 1,
            title: "Next",
            page_index: 4,
            level: 0,
            entry_type: 3,
        })?;
        Ok(BinBookChapterListSummary {
            ok: true,
            error: None,
            count: 3,
            has_more: true,
        })
    }

    fn binbook_chapter<'a>(
        &'a mut self,
        book: Handle,
        index: i32,
    ) -> Result<BinBookChapterResult<'a>, squidvm_core::error::VmError> {
        self.chapter_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        assert_eq!(index, 1);
        Ok(BinBookChapterResult {
            ok: true,
            error: None,
            chapter: Some(BinBookChapterEntry {
                index: 1,
                title: "Next",
                page_index: 4,
                level: 0,
                entry_type: 3,
            }),
        })
    }
}

#[test]
fn native_file_backend_can_drive_binbook_open_and_info() {
    let sqbc = compile_sqbc(
        r#"app "native-file-binbook"
event.on("app.start") {
  let opened = binbook.open("books/readme.binbook")
  debug.print("open", opened.ok, opened.error)
  if opened.ok {
    let info = binbook.info(opened.book)
    debug.print("info", info.ok, info.title, info.pageCount, info.chapterCount)
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FileBackedBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file-binbook", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["open true null", "info true Storage Book 12 3"]
    );
    assert_eq!(runtime.file_backend().open_calls, 1);
    assert_eq!(runtime.file_backend().info_calls, 1);
}

#[test]
fn native_file_backend_can_drive_binbook_read_page_and_drawable_draw() {
    let sqbc = compile_sqbc(
        r#"app "native-file-binbook-page"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  let opened = binbook.open("books/readme.binbook")
  if opened.ok {
    let page = binbook.readPage(opened.book, 0)
    debug.print("page", page.ok, page.error)
    if page.ok {
      service.display.draw(page.drawable)
    }
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FileBackedBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file-binbook-page", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["page true null"]);
    assert_eq!(
        runtime.drawlog_lines().as_slice(),
        &["draw Drawable:2 x=0 y=0 w=0 h=0"]
    );
    assert_eq!(runtime.file_backend().read_page_calls, 1);
}

#[test]
fn native_file_backend_can_drive_binbook_chapters_and_chapter() {
    let sqbc = compile_sqbc(
        r#"app "native-file-binbook-chapters"
event.on("app.start") {
  let opened = binbook.open("books/readme.binbook")
  if opened.ok {
    let chapters = binbook.chapters(opened.book, { offset: 0, limit: 8 })
    debug.print("chapters", chapters.ok, chapters.count, chapters.hasMore)
    for chapter in chapters.items max 8 {
      debug.print(chapter.index, chapter.title, chapter.pageIndex, chapter.level, chapter.type)
    }
    let chapter = binbook.chapter(opened.book, 1)
    debug.print("chapter", chapter.ok, chapter.index, chapter.title, chapter.pageIndex)
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FileBackedBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file-binbook-chapters", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "chapters true 3 true",
            "0 Start 0 0 3",
            "1 Next 4 0 3",
            "chapter true 1 Next 4"
        ]
    );
    assert_eq!(runtime.file_backend().open_calls, 1);
    assert_eq!(runtime.file_backend().chapters_calls, 1);
    assert_eq!(runtime.file_backend().chapter_calls, 1);
}

const TEST_CONTENT_BINBOOKS: [ContentBinBookEntry; 1] = [ContentBinBookEntry {
    name: "proof.binbook",
    reference: "content:books/r/proof.binbook",
    size: 4096,
}];

#[derive(Default)]
struct FakeBinBookBackend {
    list_calls: usize,
    open_calls: usize,
    read_page_calls: usize,
}

impl NativeBinBookBackend for FakeBinBookBackend {
    fn content_binbook_list<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
    ) -> Result<ContentBinBookListResult<'a>, squidvm_core::error::VmError> {
        self.list_calls += 1;
        assert_eq!(library, "books");
        assert_eq!(offset, 0);
        assert_eq!(limit, 1);
        Ok(ContentBinBookListResult {
            ok: true,
            error: None,
            warning: None,
            items: &TEST_CONTENT_BINBOOKS,
            count: TEST_CONTENT_BINBOOKS.len() as i32,
            has_more: false,
        })
    }

    fn binbook_open<'a>(
        &'a mut self,
        path: &str,
    ) -> Result<BinBookOpenResult<'a>, squidvm_core::error::VmError> {
        self.open_calls += 1;
        assert_eq!(path, "content:books/r/proof.binbook");
        Ok(BinBookOpenResult {
            ok: true,
            error: None,
            book: Some(Handle::new(HandleKind::BinBook, 3)),
        })
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, squidvm_core::error::VmError> {
        self.read_page_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 3));
        assert_eq!(page_index, 0);
        Ok(BinBookReadPageResult {
            ok: true,
            error: None,
            drawable: Some(Handle::new(HandleKind::Drawable, 9)),
        })
    }
}

#[test]
fn native_binbook_backend_drives_content_list_open_and_read_page() {
    let sqbc = compile_sqbc(
        r#"app "native-binbook"
event.on("app.start") {
  let listing = content.binbook.list("books", { offset: 0, limit: 1 })
  debug.print("list", listing.ok, listing.count, listing.hasMore)
  if listing.ok {
    for item in listing.items max 1 {
      let opened = binbook.open(item.ref)
      debug.print("open", opened.ok)
      if opened.ok {
        let page = binbook.readPage(opened.book, 0)
        debug.print("page", page.ok)
      }
    }
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_and_binbook(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        FakeBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-binbook", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["list true 1 false", "open true", "page true"]
    );
    let backend = runtime.binbook_backend();
    assert_eq!(backend.list_calls, 1);
    assert_eq!(backend.open_calls, 1);
    assert_eq!(backend.read_page_calls, 1);
}

#[derive(Default)]
struct CountingDisplaySink {
    events: Vec<String>,
    dropped_draws: u32,
}

impl NativeDisplaySink for CountingDisplaySink {
    fn draw_clear(&mut self, color: u8) {
        self.events.push(format!("clear {color}"));
    }

    fn screen_rendered(&mut self, name: &str) {
        self.events.push(format!("rendered {name}"));
    }

    fn pending_refreshes(&self) -> u32 {
        self.events
            .iter()
            .filter(|event| event.starts_with("rendered "))
            .count() as u32
    }

    fn recorded_draws(&self) -> u32 {
        self.events
            .iter()
            .filter(|event| !event.starts_with("rendered "))
            .count() as u32
    }

    fn dropped_draws(&self) -> u32 {
        self.dropped_draws
    }
}

#[test]
fn screen_render_completion_notifies_native_display_sink() {
    let sqbc = compile_sqbc(
        r#"app "native-display-sink"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
}
"#,
    );
    let mut runtime =
        NativeRuntime::with_radio_and_display(NoopRadioBackend, CountingDisplaySink::default());

    run_temp_app(&mut runtime, "native-display-sink", &sqbc);

    assert_eq!(runtime.drawlog_lines().as_slice(), &["clear 0"]);
    assert_eq!(
        runtime.display_sink().events.as_slice(),
        &["clear 0", "rendered main"]
    );
}

#[test]
fn resources_report_display_sink_refresh_state() {
    let sqbc = compile_sqbc(
        r#"app "native-display-resources"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
}
"#,
    );
    let mut runtime =
        NativeRuntime::with_radio_and_display(NoopRadioBackend, CountingDisplaySink::default());

    run_temp_app(&mut runtime, "native-display-resources", &sqbc);

    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "display_pending_refreshes" && metric.value == 1));
}

#[test]
fn resources_report_display_sink_flush_queue_state() {
    let sqbc = compile_sqbc(
        r#"app "native-display-queue"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_and_display(
        NoopRadioBackend,
        CountingDisplaySink {
            events: Vec::new(),
            dropped_draws: 2,
        },
    );

    run_temp_app(&mut runtime, "native-display-queue", &sqbc);

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "display_recorded_draws" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "display_dropped_draws" && metric.value == 2));
}

#[test]
fn temp_run_reports_capability_demand_from_sqbc_builtins() {
    let sqbc = compile_sqbc(
        r#"app "native-demand"
event.on("app.start") {
  let status = service.wifi.status()
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: { complete: "ble.done" }
  })
  let info = display.info()
  let files = file.list("books", { offset: 0, limit: 8 })
  debug.print(status.active, info.available, files.count)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-demand", &sqbc);

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_wifi" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_ble" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_display" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_storage" && metric.value == 1));
}

#[test]
fn installed_launch_replaces_capability_demand_metadata() {
    let radio_sqbc = compile_sqbc(
        r#"app "installed-radio"
event.on("app.start") {
  let status = service.wifi.status()
  debug.print(status.active)
}
"#,
    );
    let plain_sqbc = compile_sqbc(
        r#"app "installed-plain"
event.on("app.start") {
  debug.print("plain")
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed-radio", &radio_sqbc);
    runtime.launch_app("installed-radio").unwrap();
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "demand_wifi" && metric.value == 1));

    install_app(&mut runtime, "installed-plain", &plain_sqbc);
    runtime.launch_app("installed-plain").unwrap();

    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "demand_wifi" && metric.value == 0));
}

#[test]
fn wifi_service_calls_update_native_radio_lease_metrics() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  debug.print("wifi", ap.ok, ap.error)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-wifi", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["wifi true null"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 1));
}

#[test]
fn wifi_stop_releases_native_radio_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-stop"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let stop = service.wifi.stopAP()
  debug.print("wifi", ap.ok, stop.ok)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-wifi-stop", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["wifi true true"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
}

#[test]
fn wifi_status_reports_native_ap_configuration_and_events() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-status"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let started = service.wifi.status()
  let stop = service.wifi.stopAP()
  let stopped = service.wifi.status()
  debug.print("started", ap.ok, started.active, started.mode, started.ssid)
  debug.print("started-state", started.state, started.backend, started.driverStarted, started.configured, started.driverMode)
  debug.print("started-events", started.channel, started.apStartEvents, started.apStopEvents)
  debug.print("stopped", stop.ok, stopped.active, stopped.mode, stopped.ssid)
  debug.print("stopped-state", stopped.state, stopped.backend, stopped.driverStarted, stopped.configured, stopped.driverMode)
  debug.print("stopped-events", stopped.channel, stopped.apStartEvents, stopped.apStopEvents)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-status", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "started true true ap SquidNative",
            "started-state started native-x4 true true ap",
            "started-events 1 1 0",
            "stopped true false null null",
            "stopped-state stopped native-x4 false false null",
            "stopped-events 0 1 1",
        ]
    );
}

#[test]
fn wifi_scan_without_native_scan_support_reports_result_and_releases_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-scan"
event.on("app.start") {
  let scan = service.wifi.scan()
  let op = service.wifi.operation()
  let result = service.wifi.result()
  debug.print("scan", scan.ok, scan.error, scan.active, scan.kind, scan.state, scan.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-scan", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "scan false unsupported false scan error true",
            "operation false unsupported false scan error true",
            "result true false unsupported scan error 0",
        ]
    );
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
}

#[test]
fn wifi_scan_reports_backend_networks_and_releases_temporary_scan_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-scan-real"
event.on("app.start") {
  let scan = service.wifi.scan()
  let result = service.wifi.result()
  let row0 = service.wifi.scanNetwork(0)
  let row1 = service.wifi.scanNetwork(1)
  debug.print("scan", scan.ok, scan.error, scan.active, scan.kind, scan.state, scan.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("row0", row0.ok, row0.error, row0.ssid, row0.ssidLength, row0.channel, row0.rssi, row0.auth, row0.bssid, row0.hidden)
  debug.print("row1", row1.ok, row1.error, row1.ssidLength)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        scan_supported: true,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-scan-real", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "scan true null false scan done true",
            "result true true null scan done 1",
            "row0 true null SquidLab 8 6 -42 WPA2_PSK 02:04:06:08:0a:0c false",
            "row1 false not-found 0",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 1);
    assert_eq!(backend.wifi_scan_count, 1);
    assert_eq!(backend.wifi_release_count, 1);
}

#[test]
fn wifi_scan_while_ap_active_reports_busy_and_keeps_ap_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-busy-scan"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let scan = service.wifi.scan()
  let op = service.wifi.operation()
  let result = service.wifi.result()
  let status = service.wifi.status()
  debug.print("scan", ap.ok, scan.ok, scan.error, scan.active, scan.kind, scan.state, scan.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("status", status.active, status.mode, status.ssid)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-busy-scan", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "scan true false wifi busy false scan error true",
            "operation false wifi busy false scan error true",
            "result true false wifi busy scan error 0",
            "status true ap SquidNative",
        ]
    );
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 1));
}

#[test]
fn wifi_status_and_ap_ip_report_backend_provided_network_details() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-real-status"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let ip = service.wifi.getAPIP()
  let status = service.wifi.status()
  debug.print("ap", ap.ok, ip.error, ip.ip, ip.gw, ip.netmask)
  debug.print("status", status.active, status.mode, status.ipAddress, status.ssid, status.clients, status.channel)
  debug.print("events", status.apStartEvents, status.apStopEvents, status.probeEvents)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        ap_ip_supported: true,
        connected_clients: 2,
        probe_events: 3,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-real-status", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "ap true null 192.0.2.1 192.0.2.1 255.255.255.0",
            "status true ap 192.0.2.1 SquidNative 2 6",
            "events 1 0 3",
        ]
    );
}

#[test]
fn wifi_connect_missing_profile_reports_error_without_acquiring_radio() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-missing-profile"
event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let op = service.wifi.operation()
  let result = service.wifi.result()
  let status = service.wifi.status()
  debug.print("connect", connect.ok, connect.error, connect.active, connect.kind, connect.state, connect.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("status", status.active, status.mode, status.profile, status.connected)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-missing-profile", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "connect false profile missing false connect error true",
            "operation false profile missing false connect error true",
            "result true false profile missing connect error 0",
            "status false null null false",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 0);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
}

#[test]
fn wifi_connect_configures_matching_profile_as_station_operation() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-profile"
event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let op = service.wifi.operation()
  let result = service.wifi.result()
  let status = service.wifi.status()
  debug.print("connect", connect.ok, connect.error, connect.active, connect.kind, connect.state, connect.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("status", status.active, status.mode, status.profile, status.connected)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());
    runtime
        .set_wifi_profile("dev", "SquidNet", "password")
        .unwrap();

    run_temp_app(&mut runtime, "native-wifi-profile", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "connect true null true connect running false",
            "operation true null true connect running false",
            "result false true null connect running 0",
            "status true sta dev false",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 1);
    assert_eq!(backend.wifi_connect_count, 1);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
}

#[test]
fn ble_service_start_and_reset_release_native_radio_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-ble"
event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("ble ready")
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-ble", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["ble ready"]);
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 1));

    runtime.reset();

    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
}

#[test]
fn ble_service_start_records_native_profile_state() {
    let sqbc = compile_sqbc(
        r#"app "native-ble-profile"
event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("ble ready")
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-ble-profile", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["ble ready"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_id_len" && metric.value == 2));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_start_events" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_stop_events" && metric.value == 0));

    let backend = runtime.radio_backend();
    assert_eq!(backend.ble_profile_start_count, 1);
    assert_eq!(backend.ble_profile_stop_count, 0);
    assert_eq!(backend.ble_profile_id, "rx");
}

#[test]
fn ble_service_stop_clears_native_profile_state() {
    let sqbc = compile_sqbc(
        r#"app "native-ble-profile-stop"
event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  service.ble.stop()
  debug.print("ble stopped")
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-ble-profile-stop", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["ble stopped"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_id_len" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_start_events" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "ble_profile_stop_events" && metric.value == 1));

    let backend = runtime.radio_backend();
    assert_eq!(backend.ble_profile_start_count, 1);
    assert_eq!(backend.ble_profile_stop_count, 1);
    assert!(backend.ble_profile_id.is_empty());
}

#[test]
fn wifi_and_ble_service_calls_can_hold_native_leases_together() {
    let sqbc = compile_sqbc(
        r#"app "native-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("radios", ap.ok)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-radios", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["radios true"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 2));
}

#[test]
fn app_exit_releases_all_native_radio_leases() {
    let sqbc = compile_sqbc(
        r#"app "native-exit-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("before-exit", ap.ok)
  app.exit()
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-exit-radios", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["before-exit true"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}

#[test]
fn runtime_error_releases_all_native_radio_leases() {
    let sqbc = compile_sqbc(
        r#"app "native-error-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print(ap.missing)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());
    runtime
        .begin_temp_run("native-error-radios", sqbc.len())
        .unwrap();
    runtime.write_temp_run_chunk(0, &sqbc).unwrap();

    assert_eq!(
        runtime.commit_temp_run(),
        Err(NativeRuntimeError::Vm(
            squidvm_core::error::VmError::InvalidOperand
        ))
    );

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}

#[test]
fn storage_format_releases_radio_leases_and_formats_files() {
    let sqbc = compile_sqbc(
        r#"app "native-format-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("format-ready", ap.ok)
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        CountingRadioBackend::default(),
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-format-radios", &sqbc);
    runtime.storage_format().unwrap();

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
    assert!(runtime.file_backend().storage().formatted);

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}

#[test]
fn replacing_temp_app_releases_previous_native_radio_leases() {
    let radio_sqbc = compile_sqbc(
        r#"app "native-radio"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("radio", ap.ok)
}
"#,
    );
    let plain_sqbc = compile_sqbc(
        r#"app "native-plain"
event.on("app.start") {
  debug.print("plain")
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-radio", &radio_sqbc);
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 2));

    run_temp_app(&mut runtime, "native-plain", &plain_sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["plain"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
}

#[derive(Default)]
struct CountingRadioBackend {
    wifi_acquire_count: usize,
    wifi_release_count: usize,
    wifi_start_ap_count: usize,
    wifi_stop_ap_count: usize,
    wifi_connect_count: usize,
    wifi_scan_count: usize,
    ble_acquire_count: usize,
    ble_release_count: usize,
    ble_profile_start_count: usize,
    ble_profile_stop_count: usize,
    ble_profile_id: String,
    ap_mode: bool,
    sta_mode: bool,
    ap_ssid: String,
    scan_supported: bool,
    ap_ip_supported: bool,
    connected_clients: i32,
    probe_events: i32,
}

impl NativeRadioBackend for CountingRadioBackend {
    fn acquire(&mut self, radio: RadioKind) -> Result<(), ()> {
        match radio {
            RadioKind::Wifi => self.wifi_acquire_count += 1,
            RadioKind::Ble => self.ble_acquire_count += 1,
        }
        Ok(())
    }

    fn release(&mut self, radio: RadioKind) {
        match radio {
            RadioKind::Wifi => {
                self.wifi_release_count += 1;
                if self.ap_mode {
                    self.wifi_stop_ap_count += 1;
                }
                self.ap_mode = false;
                self.sta_mode = false;
                self.ap_ssid.clear();
            }
            RadioKind::Ble => {
                self.ble_release_count += 1;
                self.ble_profile_id.clear();
            }
        }
    }

    fn start_wifi_ap(&mut self, ssid: &str) -> Result<(), ()> {
        assert_eq!(ssid, "SquidNative");
        self.wifi_start_ap_count += 1;
        self.ap_mode = true;
        self.ap_ssid.clear();
        self.ap_ssid.push_str(ssid);
        Ok(())
    }

    fn start_ble_profile(&mut self, id: &str) -> Result<(), ()> {
        self.ble_profile_start_count += 1;
        self.ble_profile_id.clear();
        self.ble_profile_id.push_str(id);
        Ok(())
    }

    fn stop_ble_profile(&mut self) {
        self.ble_profile_stop_count += 1;
        self.ble_profile_id.clear();
    }

    fn wifi_mode(&self) -> Option<&'static str> {
        if self.ap_mode {
            Some("ap")
        } else if self.sta_mode {
            Some("sta")
        } else {
            None
        }
    }

    fn connect_wifi_station(&mut self, ssid: &str, password: &str) -> Result<(), ()> {
        assert_eq!(ssid, "SquidNet");
        assert_eq!(password, "password");
        self.wifi_connect_count += 1;
        self.sta_mode = true;
        Ok(())
    }

    fn wifi_status(&self) -> NativeWifiStatus<'_> {
        NativeWifiStatus {
            mode: self.wifi_mode(),
            ssid: self.ap_mode.then_some(self.ap_ssid.as_str()),
            ip_address: (self.ap_mode && self.ap_ip_supported).then_some("192.0.2.1"),
            state: if self.ap_mode {
                "started"
            } else if self.sta_mode {
                "starting"
            } else {
                "stopped"
            },
            driver_started: self.ap_mode || self.sta_mode,
            configured: self.ap_mode || self.sta_mode,
            channel: if self.ap_mode {
                if self.ap_ip_supported { 6 } else { 1 }
            } else {
                0
            },
            clients: if self.ap_mode {
                self.connected_clients
            } else {
                0
            },
            ap_start_events: self.wifi_start_ap_count as i32,
            ap_stop_events: self.wifi_stop_ap_count as i32,
            probe_events: self.probe_events,
            sta_connected_events: if self.sta_mode { 1 } else { 0 },
            sta_disconnected_events: 0,
            last_backend_code: None,
            connected: false,
            scan_matches: if self.scan_supported { 1 } else { 0 },
            rssi: 0,
            auth: None,
            bssid: None,
            disconnect_reason: None,
            disconnect_reason_code: 0,
        }
    }

    fn wifi_ap_ip(&self) -> NativeWifiApIp<'_> {
        if self.ap_mode && self.ap_ip_supported {
            NativeWifiApIp {
                ip: Some("192.0.2.1"),
                gw: Some("192.0.2.1"),
                netmask: Some("255.255.255.0"),
                error: None,
            }
        } else {
            NativeWifiApIp::unavailable()
        }
    }

    fn scan_wifi(&mut self) -> Result<i32, &'static str> {
        self.wifi_scan_count += 1;
        if self.scan_supported {
            Ok(1)
        } else {
            Err("unsupported")
        }
    }

    fn wifi_scan_network(&self, index: i32) -> Result<Option<WifiAccessPoint>, &'static str> {
        if !self.scan_supported {
            return Err("unsupported");
        }
        if index != 0 {
            return Ok(None);
        }
        Ok(Some(
            WifiAccessPoint::new(
                b"SquidLab",
                Some([0x02, 0x04, 0x06, 0x08, 0x0a, 0x0c]),
                6,
                -42,
                Some("WPA2_PSK"),
                false,
            )
            .unwrap(),
        ))
    }
}

#[test]
fn service_calls_drive_physical_radio_backend_once_per_active_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  let status = service.wifi.status()
  debug.print("radios", ap.ok, status.active)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-radios", &sqbc);

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 1);
    assert_eq!(backend.wifi_start_ap_count, 1);
    assert_eq!(backend.ble_acquire_count, 1);
    assert_eq!(backend.wifi_release_count, 0);
    assert_eq!(backend.ble_release_count, 0);
    assert_eq!(runtime.output_lines().as_slice(), &["radios true true"]);
}

#[test]
fn app_replacement_drives_physical_radio_backend_release() {
    let radio_sqbc = compile_sqbc(
        r#"app "native-radio"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.ble.start("file-transfer", {
    id: "rx"
    accept: [".sqbc"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("radio", ap.ok)
}
"#,
    );
    let plain_sqbc = compile_sqbc(
        r#"app "native-plain"
event.on("app.start") {
  debug.print("plain")
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-radio", &radio_sqbc);
    run_temp_app(&mut runtime, "native-plain", &plain_sqbc);

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}
