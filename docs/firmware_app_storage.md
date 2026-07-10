# Firmware App Storage

The native X4 firmware stores installed SquidScript apps and capacity-bounded
content in the internal-flash `squidscript` LittleFS partition. SD FAT storage
is the preferred large removable content volume. When SD is absent, uploads and
content reads use internal flash without changing app-facing file references.

## Logical Model

The installed-app registry contains at most eight apps. Firmware rebuilds it
at boot by scanning valid `main.sqbc` files and validating each app ID and SQBC
structure through the bounded reader API. Malformed stores and registry
overflow are retained errors; firmware keeps framed serial available and does
not hide those failures by launching fallback behavior.

Installed app execution streams SQBC byte ranges from LittleFS. The runtime
does not retain an app-sized installed-bytecode array. `RUN.TEMP` remains
RAM-backed, is absent from the installed registry, and never writes app bytecode
or state to flash.

If installed `main` is valid, firmware launches it as the root foreground app.
Otherwise, after a successful store mount and registry scan, firmware launches
the native X4 fallback app embedded by the firmware build. Fallback SQBC uses
the same VM and service callbacks but is not published in the registry and its
state is volatile.

## Physical Layout

The X4 partition table reserves the internal-flash range documented in
`targets/partitions/xteink-x4.csv`. LittleFS owns these paths:

```text
/apps/<app-id>/main.sqbc
/apps/<app-id>/resources/<resource-path>
/state/<app-id>.state
/lifecycle/resume
/tmp/install-<app-id>/...
/tmp/previous-<app-id>/...
/books/<content-name>
/content-tmp/<content-name>
```

Physical paths are firmware details and are not returned to SquidScript apps.
App IDs are at most 39 bytes. Resource paths are relative, non-empty safe path
segments; absolute paths, empty segments, `.` and `..` are rejected.
Content names are simple ASCII filenames of at most 121 bytes. This leaves room
for the logical `books/` prefix within the 128-byte firmware path budget.

New content uploads prefer SD when it is mounted and use internal LittleFS when
SD is missing. Each upload is pinned to its selected volume from begin through
commit. Reads search SD first and then internal flash. Library enumeration
merges both volumes and suppresses an internal entry when SD has the same
logical name.

## Atomic Installation

The host validates and unpacks `.squid.zip` packages. It streams package
resources first, then `main.sqbc`. Firmware stages all files below the temporary
app directory. Resource commits remain invisible until the main payload has
passed byte-count, protocol CRC, app-ID metadata, and SQBC structure checks.

Publishing renames the prior app directory to a recovery path and renames the
complete staged directory into `/apps/<app-id>`. A failed publish restores the
prior directory. Boot recovery restores an interrupted prior-directory rename
and removes incomplete install directories before rebuilding the registry.

Relative `file.readText(...)` paths in an installed app resolve against that
app's package resources first. Native resource text reads are bounded to 256 bytes;
larger text resources return `too-large`. Target content references containing
a namespace separator continue through the target content backend.

## State And Metrics

`state.save()` writes the active installed app's state to a temporary record
and atomically renames it into `/state`. `state.load()` and `state.reset()` use
that per-app record. Temp and fallback state stay in RAM.

`system.storage("apps")` reports available bytes from the mounted LittleFS
volume. `system.memory()` reports the target SRAM total and current native heap
used/free values supplied by the ESP allocator.

`device storage-format` resets the runtime, formats only the SquidScript
LittleFS partition, recreates its root directories, and clears the resident
registry plus internal content. OTA partitions and SD content are preserved.

## Bounded Access

The LittleFS adapter uses partition-checked NOR operations, fixed filesystem
caches, bounded path buffers, and caller-owned SQBC/resource buffers. App and
resource transfers accept sequential chunks and flush each chunk before it is
acknowledged. The hardware storage spike measurements and recovery evidence are
recorded in the active native X4 parity plan.
