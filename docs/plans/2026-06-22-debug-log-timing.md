# Debug Log Timing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a 4KB RAM circular buffer debug log with timestamped entries that captures button-press-to-display-flush timing, retrievable via a new `device debug-log` CLI command.

**Architecture:** A standalone `debug_log` module provides a thread-safe 4KB ring buffer with 64-byte fixed-size entries. Each entry stores a `k_uptime_get()` timestamp and event tag. A new protocol opcode `SQ_OPCODE_DEBUG_LOG_GET` (92) reads the buffer via the existing `repeated_runtime_lines_response` pattern. Five instrumentation points capture the full button-to-display pipeline.

**Tech Stack:** C (firmware), Rust (squid-device-protocol, squidc-cli), Zephyr RTOS

---

## File Map

| Action | File | Purpose |
|--------|------|---------|
| Create | `firmware/zephyr/src/debug_log.h` | API: init, append, read, line_count |
| Create | `firmware/zephyr/src/debug_log.c` | 4KB circular buffer implementation |
| Modify | `firmware/zephyr/src/protocol.h:58` | Add `SQ_OPCODE_DEBUG_LOG_GET = 92` |
| Modify | `firmware/zephyr/src/device_protocol.c:2342-2346,3659-3662` | Add debug_log case in response handler + dispatch |
| Modify | `firmware/zephyr/src/vm_runtime_indicator_gpio.c:555` | Instrument GPIO button press |
| Modify | `firmware/zephyr/src/xteink_x4_button_probe.c:238` | Instrument X4 ADC button press |
| Modify | `firmware/zephyr/src/vm_runtime.c:531-563,1205` | Instrument dispatch start + display flush start/end |
| Modify | `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:1449` | Instrument composed refresh decision |
| Modify | `firmware/zephyr/src/main.c` | Call `sq_debug_log_init()` |
| Modify | `compiler/rust/crates/squid-device-protocol/src/lib.rs` | Add opcode + request/decoder |
| Modify | `compiler/rust/crates/squidc-cli/src/serial.rs` | Add `debug_log_lines()` method |
| Modify | `compiler/rust/crates/squidc-cli/src/main.rs` | Add `device debug-log` command |

---

## Design Details

**Entry format (64 bytes fixed):** `"<millis>:<event>:<detail>\n"`

**Ring buffer:** 4096 / 64 = 64 entries. Wraps silently. NOT cleared between dispatches. Thread-safe via `k_mutex`.

**5 Instrumentation Points:**

| Tag | Thread | Location | Captures |
|-----|--------|----------|----------|
| `btn` | main | button poll | Button name + timestamp |
| `dispatch_start` | main | `sq_vm_runtime_start_event` | Event name |
| `composed_decide` | display flush | after `sq_ssd1677_composed_refresh_decide` | Decision enum, op_count, prev_valid |
| `flush_start` | display flush | before `sq_display_backend_flush` | op_count, refresh_mode |
| `flush_done` | display flush | after `sq_display_backend_flush` | result, duration_us |

---

## Tasks

### Task 1: Create debug_log module

**Files:**
- Create: `firmware/zephyr/src/debug_log.h`
- Create: `firmware/zephyr/src/debug_log.c`

- [ ] **Step 1: Create header file**

```c
#ifndef SQUIDSCRIPT_DEBUG_LOG_H
#define SQUIDSCRIPT_DEBUG_LOG_H

#include <stddef.h>
#include <stdint.h>

#define SQ_DEBUG_LOG_SIZE 4096
#define SQ_DEBUG_LOG_ENTRY_LEN 64

void sq_debug_log_init(void);
void sq_debug_log_append(const char *fmt, ...);
int sq_debug_log_read(char *out, size_t out_len);
int sq_debug_log_line_count(void);

extern char sq_debug_log_buf[SQ_DEBUG_LOG_SIZE];

#endif
```

- [ ] **Step 2: Create implementation file**

```c
#include "debug_log.h"
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <zephyr/kernel.h>

char sq_debug_log_buf[SQ_DEBUG_LOG_SIZE];
static size_t debug_log_pos;
static bool debug_log_wrapped;
static struct k_mutex debug_log_mutex;

void sq_debug_log_init(void)
{
	memset(sq_debug_log_buf, 0, sizeof(sq_debug_log_buf));
	debug_log_pos = 0;
	debug_log_wrapped = false;
	k_mutex_init(&debug_log_mutex);
}

void sq_debug_log_append(const char *fmt, ...)
{
	char line[SQ_DEBUG_LOG_ENTRY_LEN];
	va_list args;
	int len;

	va_start(args, fmt);
	len = vsnprintf(line, sizeof(line), fmt, args);
	va_end(args);

	if (len < 0 || (size_t)len >= sizeof(line)) {
		return;
	}

	k_mutex_lock(&debug_log_mutex, K_FOREVER);
	if (debug_log_pos + SQ_DEBUG_LOG_ENTRY_LEN > SQ_DEBUG_LOG_SIZE) {
		debug_log_pos = 0;
		debug_log_wrapped = true;
	}
	memset(sq_debug_log_buf + debug_log_pos, 0, SQ_DEBUG_LOG_ENTRY_LEN);
	memcpy(sq_debug_log_buf + debug_log_pos, line, len);
	debug_log_pos += SQ_DEBUG_LOG_ENTRY_LEN;
	k_mutex_unlock(&debug_log_mutex);
}

int sq_debug_log_read(char *out, size_t out_len)
{
	size_t total;

	if (out == NULL || out_len == 0) {
		return 0;
	}

	k_mutex_lock(&debug_log_mutex, K_FOREVER);
	if (!debug_log_wrapped) {
		total = debug_log_pos;
		if (total > out_len) {
			total = out_len;
		}
		memcpy(out, sq_debug_log_buf, total);
	} else {
		size_t first = SQ_DEBUG_LOG_SIZE - debug_log_pos;

		total = SQ_DEBUG_LOG_SIZE;
		if (total > out_len) {
			total = out_len;
		}
		if (first >= total) {
			memcpy(out, sq_debug_log_buf + debug_log_pos, total);
		} else {
			memcpy(out, sq_debug_log_buf + debug_log_pos, first);
			memcpy(out + first, sq_debug_log_buf, total - first);
		}
	}
	k_mutex_unlock(&debug_log_mutex);
	return total;
}

int sq_debug_log_line_count(void)
{
	int count;

	k_mutex_lock(&debug_log_mutex, K_FOREVER);
	if (!debug_log_wrapped) {
		count = debug_log_pos / SQ_DEBUG_LOG_ENTRY_LEN;
	} else {
		count = SQ_DEBUG_LOG_SIZE / SQ_DEBUG_LOG_ENTRY_LEN;
	}
	k_mutex_unlock(&debug_log_mutex);
	return count;
}
```

- [ ] **Step 3: Verify it compiles**

Run: `source scripts/zephyr-env.sh && west build -d build/zephyr/xteink-x4 2>&1 | tail -5`

---

### Task 2: Add protocol opcode

**Files:**
- Modify: `firmware/zephyr/src/protocol.h:58`
- Modify: `firmware/zephyr/src/device_protocol.c:2342-2346,3659-3662`

- [ ] **Step 1: Add opcode to protocol.h**

After `SQ_OPCODE_CONTENT_CHECK = 91,` add:

```c
	SQ_OPCODE_DEBUG_LOG_GET = 92,
```

- [ ] **Step 2: Add include in device_protocol.c**

After existing includes, add:

```c
#include "debug_log.h"
```

- [ ] **Step 3: Add response handler case**

In `repeated_runtime_lines_response`, after the drawlog block (line 2346), add:

```c
	if (request->opcode == SQ_OPCODE_DEBUG_LOG_GET) {
		fixed_lines = (const uint8_t *)sq_debug_log_buf;
		fixed_count = sq_debug_log_line_count();
		fixed_stride = SQ_DEBUG_LOG_ENTRY_LEN;
	}
```

- [ ] **Step 4: Add dispatch case**

In the switch statement, after `SQ_OPCODE_DRAWLOG_GET` case, add:

```c
	case SQ_OPCODE_DEBUG_LOG_GET:
		result = repeated_runtime_lines_response(&frame, context->runtime, NULL, 0,
							 response, response_cap, response_len);
		break;
```

- [ ] **Step 5: Verify compilation**

Run: `source scripts/zephyr-env.sh && west build -d build/zephyr/xteink-x4 2>&1 | tail -5`

---

### Task 3: Rust protocol support

**Files:**
- Modify: `compiler/rust/crates/squid-device-protocol/src/lib.rs`

- [ ] **Step 1: Add opcode to enum (line 75)**

After `ContentCheck = 91,` add:

```rust
    DebugLogGet = 92,
```

- [ ] **Step 2: Add parse case (line 114)**

After `"contentcheck" => Ok(Self::ContentCheck),` add:

```rust
            "debuglogget" => Ok(Self::DebugLogGet),
```

- [ ] **Step 3: Add TryFrom case (line 157)**

After `91 => Ok(Self::ContentCheck),` add:

```rust
            92 => Ok(Self::DebugLogGet),
```

- [ ] **Step 4: Add request function (after line 1102)**

```rust
#[cfg(feature = "alloc")]
pub fn debug_log_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::DebugLogGet, sequence, Vec::new())
}
```

- [ ] **Step 5: Add response decoder (after line 1491)**

```rust
#[cfg(feature = "alloc")]
pub fn debug_log_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::DebugLogGet, 1)
}
```

- [ ] **Step 6: Build Rust crates**

Run: `cargo build -p squid-device-protocol 2>&1 | tail -5`

---

### Task 4: CLI command

**Files:**
- Modify: `compiler/rust/crates/squidc-cli/src/serial.rs:15,272`
- Modify: `compiler/rust/crates/squidc-cli/src/main.rs:167,551,2058`

- [ ] **Step 1: Add imports in serial.rs**

Add to the import list:

```rust
    debug_log_get_request, debug_log_lines,
```

- [ ] **Step 2: Add method in SerialDevice**

After `drawlog_lines`:

```rust
    pub fn debug_log_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&debug_log_get_request(10))?;
        debug_log_lines(&frame).ok_or_else(|| "not a successful debug-log response".to_string())
    }
```

- [ ] **Step 3: Add enum variant in main.rs**

After `Drawlog(DeviceOnlyArgs),` add:

```rust
    DebugLog(DeviceOnlyArgs),
```

- [ ] **Step 4: Add dispatch case**

After `DeviceCommands::Drawlog(args) => drawlog(args.device, human),` add:

```rust
            DeviceCommands::DebugLog(args) => debug_log(args.device, human),
```

- [ ] **Step 5: Add function**

After the `drawlog` function:

```rust
fn debug_log(options: DeviceOnlyOptions, human: bool) -> Result<Value, String> {
    let port = resolve_port(&options)?;
    let mut device = SerialDevice::open(&port)?;
    let lines = device.debug_log_lines()?;
    let response = format_raw_lines(&lines);
    if human {
        print!("{response}");
    }
    Ok(json!({"port": port, "command": "debug-log", "lines": lines}))
}
```

- [ ] **Step 6: Build CLI**

Run: `cargo build -p squidc 2>&1 | tail -5`

---

### Task 5: Instrument button press

**Files:**
- Modify: `firmware/zephyr/src/xteink_x4_button_probe.c:238`
- Modify: `firmware/zephyr/src/vm_runtime_indicator_gpio.c:555`

- [ ] **Step 1: Add include in xteink_x4_button_probe.c**

```c
#include "debug_log.h"
```

- [ ] **Step 2: Instrument X4 button**

Before `return sq_vm_runtime_start(runtime, &runtime->job_backend, event);` (line 238), add:

```c
	sq_debug_log_append("%lld:btn:%s", (long long)k_uptime_get(), event);
```

- [ ] **Step 3: Add include in vm_runtime_indicator_gpio.c**

```c
#include "debug_log.h"
```

- [ ] **Step 4: Instrument GPIO button**

Before `return sq_vm_runtime_start(runtime, &runtime->job_backend, button->event);` (line 555), add:

```c
		sq_debug_log_append("%lld:btn:%s", (long long)k_uptime_get(), button->event);
```

---

### Task 6: Instrument VM dispatch start

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c:1205`

- [ ] **Step 1: Add include**

```c
#include "debug_log.h"
```

- [ ] **Step 2: Instrument dispatch start**

After `runtime->status = SQ_VM_RUNTIME_RUNNING;` (line 1205), add:

```c
	sq_debug_log_append("%lld:dispatch_start:%s", (long long)k_uptime_get(), runtime->event);
```

---

### Task 7: Instrument display flush

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c:531-563`

- [ ] **Step 1: Instrument flush start**

Before `sq_display_backend_flush` call (line 539), add:

```c
		sq_debug_log_append("%lld:flush_start:ops=%d:mode=%d",
				    (long long)k_uptime_get(),
				    sq_vm_runtime_display_active_job.op_count,
				    (int)sq_vm_runtime_display_active_job.refresh_mode);
```

- [ ] **Step 2: Instrument flush end**

After `sq_vm_runtime_last_display_flush_us = ...` (line 543), add:

```c
		sq_debug_log_append("%lld:flush_done:result=%d:us=%llu",
				    (long long)k_uptime_get(), result,
				    (unsigned long long)sq_vm_runtime_last_display_flush_us);
```

---

### Task 8: Instrument composed refresh decision

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:1449`

- [ ] **Step 1: Add include**

```c
#include "debug_log.h"
```

- [ ] **Step 2: Instrument after composed_refresh_decide**

After the existing `LOG_INF("composed decide: ...")` (line 1449), add:

```c
		sq_debug_log_append("%lld:composed_decide:%d:ops=%d:prev=%d",
				    (long long)k_uptime_get(),
				    (int)composed_refresh,
				    (int)op_count,
				    (int)composed_refresh_state.previous_ops_valid);
```

---

### Task 9: Init and test on hardware

**Files:**
- Modify: `firmware/zephyr/src/main.c`

- [ ] **Step 1: Add include and init in main.c**

Add `#include "debug_log.h"` and call `sq_debug_log_init()` before `sq_app_store_mount_target_filesystem()`.

- [ ] **Step 2: Build firmware**

Run: `source scripts/zephyr-env.sh && west build -d build/zephyr/xteink-x4 2>&1 | tail -10`

- [ ] **Step 3: Flash firmware**

Run: `source scripts/zephyr-env.sh && west flash -d build/zephyr/xteink-x4 2>&1 | tail -5`

- [ ] **Step 4: Install and launch grid-cursor**

```bash
cargo run -p squidc -- app install examples/grid-cursor/main.squid
cargo run -p squidc -- app launch grid-cursor
```

- [ ] **Step 5: Press button on device, then retrieve debug log**

Run: `cargo run -p squidc -- device debug-log`

Expected output:
```
1234567:btn:A
1234570:dispatch_start:key.A
1234580:composed_decide:0:ops=32:prev=0
1234585:flush_start:ops=32:mode=3
1236490:flush_done:result=0:us=1905000
```

- [ ] **Step 6: Commit all changes**
