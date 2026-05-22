use crate::kernel::RamDiagnostics;

pub(super) const RAM_TOTAL_BYTES: usize = 400 * 1024;

pub(super) fn live_ram_diagnostics() -> RamDiagnostics {
    let stats = esp_alloc::HEAP.stats();
    RamDiagnostics {
        ram_total_bytes: RAM_TOTAL_BYTES,
        heap_total_bytes: stats.size,
        heap_used_bytes: stats.current_usage,
        heap_peak_used_bytes: stats.max_usage,
        heap_total_allocated_bytes: stats.total_allocated,
        heap_total_freed_bytes: stats.total_freed,
    }
}
