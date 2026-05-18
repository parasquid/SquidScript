# SQBC Binary Format

Status: Minimal v1 container

SQBC is the production executable path for firmware. The current implementation is a small versioned container so the loader boundary can be exercised before the final bytecode instruction format is designed.

## v1 Container

```text
offset  size  field
0       4     magic: "SQBC"
4       4     little-endian u32 version: 1
8       4     little-endian u32 payload length
12      n     payload
```

The temporary v1 payload is versioned IR JSON. This is acceptable only as an intermediate implementation step while the real SQBC instruction stream is defined.

Firmware must continue to reject browser-only `entry.type = "ir"` manifests. When firmware SQBC execution starts, the payload should become real bytecode instructions rather than IR JSON.

