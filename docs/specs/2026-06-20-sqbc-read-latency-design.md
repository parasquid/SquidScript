# SQBC Read Latency Design

## Purpose

Foreground event dispatch must not repeatedly pay filesystem open and close
costs for every SQBC read. The XTEINK X4 grid-cursor workload performs more
than sixty logical SQBC reads while rendering one screen, so filesystem handle
management dominates input-to-render latency even though the program is small.

The implementation target is a grid-cursor dispatch below 100 ms on XTEINK X4
hardware without reserving an app-sized static buffer. Logical SQBC read counts
and byte counts remain diagnostic measures of VM access patterns; reducing
physical filesystem operations must not hide those logical counters.

## Constraints

- ESP32-C3 linker DRAM and runtime heap are constrained resources. The storage
  optimization must not reserve the maximum 65,536-byte app size in RAM.
- SQBC remains file-backed. Apps larger than an optional cache limit must run
  through the same storage backend without different language or VM semantics.
- The VM storage interface continues to use caller-owned transfer buffers.
- A storage object is accessed by only one completed or active VM dispatch at a
  time. Storage replacement must wait for the runtime to become idle.
- App install, temp-run replacement, app switching, reset, and storage format
  must not leave a stale file handle or cached view of replaced SQBC data.
- Filesystem and allocation failures retain their specific errno values.

## Stage 1: Reusable SQBC File Handle

`vm_fs_storage` owns one initialized `fs_file_t` shared by serialized SQBC
storage sessions. Each `sq_vm_fs_storage` object carries a session identifier
without growing beyond its existing fixed-buffer budget. The first SQBC read
for a session opens its configured path. Later reads seek and read through the
same handle instead of reopening the file. Reading from a different session
closes the prior handle before opening the new path.

The storage module exposes an explicit release operation. Lifecycle code calls
it only after the runtime is idle and before it clears a storage object,
changes its SQBC path, replaces an installed file, removes temp-run staging, or
formats the backing volume. Release is idempotent. A seek or read failure
closes the handle while returning the original operation error, allowing a
later request to reopen cleanly.

The app-store and temp-run storage owners remain responsible for their path
buffers. The module handle tracks an owner pointer plus session identifier so a
reused object address cannot inherit a stale handle. The generic VM storage
callback does not allocate app-sized memory or infer app lifecycle from path
strings.

### Stage 1 tests

- Two SQBC requests through one filesystem storage object return the requested
  bytes and perform one filesystem open.
- Releasing the storage closes the handle; a later read reopens it and returns
  current file contents.
- Releasing unopened storage succeeds.
- A failed seek, short read, or filesystem read returns the specific failure
  and leaves the storage able to reopen.
- Existing state load, save, and reset behavior is unchanged.
- App switch, reset, install replacement, temp-run replacement, and storage
  format paths release active SQBC storage after waiting for VM idle.

## Stage 1 Hardware Acceptance

Build and flash the canonical XTEINK X4 firmware, install the canonical
`examples/grid-cursor` package, and drive a cursor transition that redraws the
screen. Record dispatch duration, logical SQBC reads, logical SQBC bytes, heap
usage, and both stack high-water values.

Stage 1 is sufficient when repeated valid cursor transitions complete below
100 ms without new device errors or reduced stack guardrail compliance. If the
result remains at or above 100 ms, proceed to Stage 2.

## Stage 2: Optional Small-App Byte Cache

Stage 2 adds a named 4 KiB default cache limit. The app or temp-run storage
owner may allocate a buffer with Zephyr's system-heap allocator only when the
SQBC file length fits the limit, then pass that caller-owned buffer and its
capacity to `sq_vm_fs_storage`. The storage backend never allocates the buffer.
The entire file is loaded once, validated for exact length, and subsequent
logical SQBC requests copy from that buffer. Allocation failure is not an
app-launch failure; the owner configures the reusable file-handle path instead.

Apps larger than the limit always use the reusable file handle. At each
lifecycle invalidation boundary, the storage owner first detaches and clears
the cache view, then frees its allocation. No cache pointer crosses the Rust
FFI boundary, and the existing caller-owned SQBC completion buffer remains the
only transfer buffer used by VM requests.

Stage 2 must expose cache-active and cache-size data through the existing typed
resource metric protocol before hardware verification. Host tooling translates
those metric identifiers to readable names.

### Stage 2 tests

- An app at the cache limit is read once and later SQBC requests use cached
  bytes.
- An app above the limit uses the reusable file handle without launch failure.
- Allocation failure falls back to file-backed reads.
- Short initial reads fail cache population and do not expose partial data.
- Release clears the cache before a replaced SQBC file is read.

## Documentation

If Stage 1 is sufficient, update `docs/firmware_app_storage.md` to describe the
reusable file-backed read behavior. If Stage 2 is required, document the cache
limit, fallback behavior, lifecycle invalidation, and resource metrics there as
well. Runtime limits documentation changes only if the cache limit becomes a
public target/runtime contract.

## Out of Scope

- Changing SQBC encoding or adding a compatibility/version field.
- Pre-decoding a complete program into a second in-memory representation.
- Changing display refresh semantics, input debounce, or state persistence.
- Reserving a maximum-app-size static or stack buffer.
