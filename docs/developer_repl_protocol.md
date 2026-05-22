# Developer Device Protocol

Status: host and Zephyr codecs established; minimal framed hello implemented
and verified on ESP32-C3 Zephyr firmware.

The current real firmware path is Zephyr. The developer device protocol is the
Zephyr-owned command surface used by `squidc run`, `squidc app`, `squidc repl`,
and `squidc device`.

The old ESP32-C3 Rust firmware line protocol is obsolete reference material.
Do not preserve its command names, response markers, or storage behavior unless
the same shape is deliberately implemented as the current Zephyr protocol.

## Required Commands

The Zephyr command surface must cover:

- firmware hello/identity and target diagnostics
- app install, temp run, launch, app list, and reset
- key and generic event dispatch
- output, trace, draw log, state, errors, and resources
- storage format
- Wi-Fi status, scan, AP, station, and profile provisioning

## Frame Format

The current protocol is binary framed. The old text-line protocol markers such
as `INSTALL.APP`, `RUN.TEMP`, `APP.LIST`, `BEGIN`, `END`, `OK`, and `ERR` are
not current wire compatibility requirements.

Every frame starts with a 20-byte little-endian header:

| Offset | Size | Field |
| --- | ---: | --- |
| 0 | 4 | magic bytes `SQDP` |
| 4 | 1 | frame kind: `1` request, `2` response, `3` event |
| 5 | 1 | opcode |
| 6 | 1 | status: `0` ok, `1` error, `2` pending |
| 7 | 1 | reserved, currently `0` |
| 8 | 4 | sequence |
| 12 | 4 | payload length |
| 16 | 4 | payload CRC32 |

The payload is a compact TLV stream. Each field uses one byte for tag, one byte
for type, two little-endian bytes for value length, then the value bytes. Field
types are `0` bytes, `1` UTF-8 string, `3` bool, `4` signed 64-bit integer, `5`
unsigned 64-bit integer, and `32` nested record. Repeated records are preserved
as repeated TLV fields with the same tag.

Large writes use begin/chunk/commit opcode groups for installed apps, temp
apps, and resources. Chunk payloads must carry explicit byte lengths and
integrity through frame length and payload CRC32. Credential provisioning must
report profile names and byte lengths only; it must not echo SSIDs, passwords,
BSSIDs, MAC addresses, local IP addresses, or other environment-identifying
values unless the user explicitly asks for raw identifiers.

## Host Tooling

Normal workflows should use grouped `squidc` commands. Raw protocol access is
only for low-level protocol troubleshooting. `squidc protocol raw` builds one
binary request frame from an opcode name plus typed TLV fields and prints hex
request/response data.

The current implemented Zephyr command handler covers framed `hello` identity,
framed installed-app begin/chunk/commit, framed temp-run begin/chunk/commit,
framed app launch, framed `app-list`, and framed `output-get` over the UART
serial transport. App install and temp run begin with field tag `1` app ID
string, tag `2` total SQBC byte length, and tag `3` CRC32 encoded as an
unsigned 64-bit integer. Each chunk uses tag `1` byte offset and tag `2` byte
payload. Install commit verifies byte count and CRC32 before publishing
`/sq/apps/<app-id>/main.sqbc`; temp-run commit verifies byte count and CRC32
before launching the temporary foreground app from `/sq/tmp/temp-run.sqbc.tmp`.

`app-list` responses use repeated record fields: response field tag `1` is one
app record, record field tag `1` is the app ID string, and record field tag `2`
is the SQBC length as an unsigned 64-bit integer.

`output-get` responses use repeated string fields with tag `1`, one field per
debug output line. The current Zephyr runtime returns an empty output stream
until the debug-output service is wired through the VM host.

Key, trace, draw log, state, errors, resources, storage, and service commands
remain migration work.

## Diagnostics

Resource diagnostics should report RAM numbers separately from flash storage
numbers. When the user asks for "memory" without qualification, report RAM by
default.

Wi-Fi diagnostics should distinguish internal firmware/driver state from
external RF proof. A successful Zephyr Wi-Fi status record does not by itself
prove that another device can see or join an AP.
