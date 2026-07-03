#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]

#[cfg(target_arch = "riscv32")]
use esp_backtrace as _;

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use esp_println::println;

#[cfg(all(target_arch = "riscv32", not(any(feature = "wifi", feature = "ble"))))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use esp_hal::ram;

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
use esp_alloc::export::enumset::EnumSet;

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use squidscript_fw_core::radio_lifecycle::{
    format_cycle_snapshot, CycleSnapshot, RadioKind, ReclaimSummary,
};

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use squidscript_fw_x4::radio_probe::radio_stack_metadata;

#[cfg(target_arch = "riscv32")]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(target_arch = "riscv32")]
#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    #[cfg(not(any(feature = "wifi", feature = "ble")))]
    {
        run_serial_protocol(UsbSerialJtag::new(peripherals.USB_DEVICE));
    }

    #[cfg(any(feature = "wifi", feature = "ble"))]
    {
        let radio = radio_stack_metadata();
        println!(
            "squidscript native x4 radio_probe stack={} version={} features={:?}",
            radio.stack, radio.version, radio.features
        );

        println!("radio_probe_stage allocator_init");
        esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
        esp_alloc::heap_allocator!(size: 36 * 1024);

        let timer = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let software_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        println!("radio_probe_stage rtos_start");
        esp_rtos::start(timer.timer0, software_interrupt.software_interrupt0);
        println!("radio_probe_stage rtos_ready");

        #[cfg(feature = "wifi")]
        run_radio_probe(RadioKind::Wifi);

        #[cfg(feature = "ble")]
        run_radio_probe(RadioKind::Ble);

        loop {
            core::hint::spin_loop();
        }
    }
}

#[cfg(all(target_arch = "riscv32", not(any(feature = "wifi", feature = "ble"))))]
fn run_serial_protocol(mut serial: UsbSerialJtag<'static, esp_hal::Blocking>) -> ! {
    use squid_device_protocol::{
        encode_empty_response_into, encode_hello_response_into, DeviceRequest, Opcode, Status,
        MAGIC,
    };

    const MAX_REQUEST_BYTES: usize = 256;
    const MAX_RESPONSE_BYTES: usize = 256;
    const SERIAL_MAX_FRAME_BYTES: u64 = 4096;

    let mut request = [0u8; MAX_REQUEST_BYTES];
    let mut request_len = 0usize;
    let mut response = [0u8; MAX_RESPONSE_BYTES];

    loop {
        match serial.read_byte() {
            Ok(byte) => {
                if request_len == request.len() {
                    request_len = 0;
                }
                request[request_len] = byte;
                request_len += 1;

                if request_len >= MAGIC.len() && request[..MAGIC.len()] != MAGIC {
                    if let Some(start) = find_magic(&request[..request_len]) {
                        request.copy_within(start..request_len, 0);
                        request_len -= start;
                    } else {
                        let keep = MAGIC.len().saturating_sub(1).min(request_len);
                        request.copy_within(request_len - keep..request_len, 0);
                        request_len = keep;
                    }
                    continue;
                }

                let Some(frame_len) = complete_request_len(&request[..request_len]) else {
                    continue;
                };
                if frame_len > request_len {
                    continue;
                }

                if let Ok(parsed) = DeviceRequest::decode(&request[..frame_len]) {
                    let encoded = match parsed.opcode {
                        Opcode::Hello => encode_hello_response_into(
                            Opcode::Hello,
                            parsed.sequence,
                            "xteink-x4",
                            "squidscript-native-x4",
                            false,
                            SERIAL_MAX_FRAME_BYTES,
                            &mut response,
                        ),
                        Opcode::Reset => encode_empty_response_into(
                            Opcode::Reset,
                            Status::Ok,
                            parsed.sequence,
                            &mut response,
                        ),
                        opcode => encode_empty_response_into(
                            opcode,
                            Status::Error,
                            parsed.sequence,
                            &mut response,
                        ),
                    };
                    if let Ok(len) = encoded {
                        let _ = serial.write(&response[..len]);
                    }
                }

                let remaining = request_len - frame_len;
                request.copy_within(frame_len..request_len, 0);
                request_len = remaining;
            }
            Err(_) => core::hint::spin_loop(),
        }
    }
}

#[cfg(all(target_arch = "riscv32", not(any(feature = "wifi", feature = "ble"))))]
fn complete_request_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < squid_device_protocol::HEADER_LEN {
        return None;
    }
    let payload_len = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    squid_device_protocol::HEADER_LEN.checked_add(payload_len)
}

#[cfg(all(target_arch = "riscv32", not(any(feature = "wifi", feature = "ble"))))]
fn find_magic(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(squid_device_protocol::MAGIC.len())
        .position(|window| window == squid_device_protocol::MAGIC)
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
fn run_radio_probe(radio: RadioKind) {
    const CYCLE_COUNT: usize = 5;
    let mut cycles = [CycleSnapshot {
        radio,
        before_free_bytes: 0,
        active_free_bytes: 0,
        after_deinit_free_bytes: 0,
        before_largest_free_block: None,
        after_largest_free_block: None,
    }; CYCLE_COUNT];

    for (index, cycle) in cycles.iter_mut().enumerate() {
        println!(
            "radio_probe_stage cycle_start radio={} cycle={}",
            squidscript_fw_core::radio_lifecycle::radio_name(radio),
            index + 1
        );
        *cycle = match run_radio_cycle(radio, index + 1) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                println!(
                    "radio_probe_error radio={} cycle={} error={}",
                    squidscript_fw_core::radio_lifecycle::radio_name(radio),
                    index + 1,
                    error
                );
                return;
            }
        };
        let mut line = StackLine::<192>::new();
        let _ = format_cycle_snapshot(index + 1, cycle, &mut line);
        println!("{}", line.as_str());
    }

    let summary = squidscript_fw_core::radio_lifecycle::evaluate_reusable_reclaim(
        radio,
        &cycles,
        squidscript_fw_x4::radio_probe::REUSABLE_RECLAIM_GATE,
    );
    print_summary(&summary);
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
fn run_radio_cycle(radio: RadioKind, cycle_index: usize) -> Result<CycleSnapshot, &'static str> {
    print_heap_stats("before", radio, cycle_index);
    let before_free = heap_free_bytes();
    match radio {
        #[cfg(feature = "wifi")]
        RadioKind::Wifi => {
            println!("radio_probe_stage wifi_init");
            // The lifecycle harness recreates the singleton handle only after the
            // previous controller has been dropped.
            let wifi = unsafe { esp_hal::peripherals::WIFI::steal() };
            let controller = esp_radio::wifi::WifiController::new(wifi, Default::default())
                .map_err(|_| "wifi_init")?;
            println!("radio_probe_stage wifi_active");
            print_heap_stats("wifi_active", radio, cycle_index);
            let active_free = heap_free_bytes();
            drop(controller);
            println!("radio_probe_stage wifi_dropped");
            print_heap_stats("wifi_dropped", radio, cycle_index);
            settle_after_deinit();
            print_heap_stats("wifi_settled", radio, cycle_index);
            Ok(CycleSnapshot {
                radio,
                before_free_bytes: before_free,
                active_free_bytes: active_free,
                after_deinit_free_bytes: heap_free_bytes(),
                before_largest_free_block: None,
                after_largest_free_block: None,
            })
        }
        #[cfg(feature = "ble")]
        RadioKind::Ble => {
            println!("radio_probe_stage ble_init");
            // The lifecycle harness recreates the singleton handle only after the
            // previous connector has been dropped.
            let bt = unsafe { esp_hal::peripherals::BT::steal() };
            let connector = esp_radio::ble::controller::BleConnector::new(bt, Default::default())
                .map_err(|_| "ble_init")?;
            println!("radio_probe_stage ble_active");
            print_heap_stats("ble_active", radio, cycle_index);
            let active_free = heap_free_bytes();
            drop(connector);
            println!("radio_probe_stage ble_dropped");
            print_heap_stats("ble_dropped", radio, cycle_index);
            settle_after_deinit();
            print_heap_stats("ble_settled", radio, cycle_index);
            Ok(CycleSnapshot {
                radio,
                before_free_bytes: before_free,
                active_free_bytes: active_free,
                after_deinit_free_bytes: heap_free_bytes(),
                before_largest_free_block: None,
                after_largest_free_block: None,
            })
        }
        #[allow(unreachable_patterns)]
        _ => Err("radio_feature_disabled"),
    }
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
fn heap_free_bytes() -> usize {
    esp_alloc::HEAP.free()
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
fn print_heap_stats(stage: &str, radio: RadioKind, cycle: usize) {
    let stats = esp_alloc::HEAP.stats();
    println!(
        "heap_stats stage={} radio={} cycle={} free={} used={} max_used={} total_allocated={} total_freed={}",
        stage,
        squidscript_fw_core::radio_lifecycle::radio_name(radio),
        cycle,
        stats.size.saturating_sub(stats.current_usage),
        stats.current_usage,
        stats.max_usage,
        stats.total_allocated,
        stats.total_freed
    );
    for (index, region) in stats.region_stats.iter().enumerate() {
        if let Some(region) = region {
            println!(
                "heap_region stage={} radio={} cycle={} region={} size={} used={} free={}",
                stage,
                squidscript_fw_core::radio_lifecycle::radio_name(radio),
                cycle,
                index,
                region.size,
                region.used,
                region.free
            );
        }
    }
    #[cfg(feature = "alloc-trace")]
    print_live_allocations(stage, radio, cycle);
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
fn settle_after_deinit() {
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
fn print_summary(summary: &ReclaimSummary) {
    let mut line = StackLine::<192>::new();
    let _ = squidscript_fw_core::radio_lifecycle::format_reclaim_summary(summary, &mut line);
    println!("{}", line.as_str());
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
struct StackLine<const N: usize> {
    buf: [u8; N],
    len: usize,
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
impl<const N: usize> StackLine<N> {
    const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("invalid_utf8")
    }
}

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
impl<const N: usize> core::fmt::Write for StackLine<N> {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        let available = N.saturating_sub(self.len);
        if value.len() > available {
            return Err(core::fmt::Error);
        }
        let end = self.len + value.len();
        self.buf[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
#[derive(Clone, Copy)]
struct AllocationRecord {
    ptr: usize,
    size: usize,
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
static mut LIVE_ALLOCATIONS: [AllocationRecord; 512] = [AllocationRecord { ptr: 0, size: 0 }; 512];

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
#[no_mangle]
pub extern "Rust" fn _esp_alloc_alloc(
    _heap: &esp_alloc::EspHeap,
    _caps: EnumSet<esp_alloc::MemoryCapability>,
    ptr: usize,
    size: usize,
) {
    unsafe {
        let base = core::ptr::addr_of_mut!(LIVE_ALLOCATIONS).cast::<AllocationRecord>();
        for index in 0..512 {
            let slot = base.add(index);
            if (*slot).ptr == 0 {
                *slot = AllocationRecord { ptr, size };
                break;
            }
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
#[no_mangle]
pub extern "Rust" fn _esp_alloc_dealloc(_heap: &esp_alloc::EspHeap, ptr: usize, _size: usize) {
    unsafe {
        let base = core::ptr::addr_of_mut!(LIVE_ALLOCATIONS).cast::<AllocationRecord>();
        for index in 0..512 {
            let slot = base.add(index);
            if (*slot).ptr == ptr {
                *slot = AllocationRecord { ptr: 0, size: 0 };
                break;
            }
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
fn print_live_allocations(stage: &str, radio: RadioKind, cycle: usize) {
    let mut total = 0;
    let mut count = 0;
    let mut sizes = [0usize; 8];
    unsafe {
        let base = core::ptr::addr_of!(LIVE_ALLOCATIONS).cast::<AllocationRecord>();
        for index in 0..512 {
            let slot = *base.add(index);
            if slot.ptr == 0 {
                continue;
            }
            total += slot.size;
            count += 1;
            if let Some(size_slot) = sizes
                .iter_mut()
                .find(|size_slot| **size_slot == 0 || **size_slot == slot.size)
            {
                *size_slot = slot.size;
            }
        }
    }
    println!(
        "live_allocs stage={} radio={} cycle={} count={} total={} sample_sizes={},{},{},{},{},{},{},{}",
        stage,
        squidscript_fw_core::radio_lifecycle::radio_name(radio),
        cycle,
        count,
        total,
        sizes[0],
        sizes[1],
        sizes[2],
        sizes[3],
        sizes[4],
        sizes[5],
        sizes[6],
        sizes[7]
    );
}

#[cfg(not(target_arch = "riscv32"))]
fn main() {}
