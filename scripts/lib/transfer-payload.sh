#!/usr/bin/env bash

create_transfer_payload() {
  local payload="$1"
  local size="$2"
  local meta="${payload}.meta"

  mkdir -p "$(dirname "${payload}")"
  python3 - "${payload}" "${size}" "${meta}" <<'PY'
import pathlib
import sys
import zlib

path = pathlib.Path(sys.argv[1])
size = int(sys.argv[2])
meta = pathlib.Path(sys.argv[3])
if size <= 0:
    raise SystemExit("payload size must be positive")
pattern = bytes(((i * 73 + 19) & 0xff) for i in range(4096))
remaining = size
crc = 0
with path.open("wb") as handle:
    while remaining:
        chunk = pattern[: min(len(pattern), remaining)]
        handle.write(chunk)
        crc = zlib.crc32(chunk, crc)
        remaining -= len(chunk)
crc &= 0xffffffff
meta.write_text(f"SIZE={size}\nCRC32={crc:08x}\n", encoding="utf-8")
PY
}

create_transfer_binbook_payload() {
  local payload="$1"
  local meta="${payload}.meta"

  mkdir -p "$(dirname "${payload}")"
  python3 - "${payload}" "${meta}" <<'PY'
import pathlib
import struct
import sys
import zlib

path = pathlib.Path(sys.argv[1])
meta = pathlib.Path(sys.argv[2])

header_size = 256
section_entry_size = 40
page_index_entry_size = 76
nav_index_entry_size = 48
chapter_index_entry_size = 32
section_count = 5
string_table = b"Chapter OneChapter Two"
nav_count = 2
chapter_count = 2
page_count = 2
page_data_len = 8192

string_offset = header_size + section_count * section_entry_size
nav_offset = string_offset + len(string_table)
chapter_offset = nav_offset + nav_count * nav_index_entry_size
page_index_offset = chapter_offset + chapter_count * chapter_index_entry_size
page_data_offset = page_index_offset + page_count * page_index_entry_size
file_size = page_data_offset + page_data_len

data = bytearray(file_size)
data[0:7] = b"BINBOOK"
struct.pack_into("<HHH", data, 8, 0, 0, header_size)
struct.pack_into("<Q", data, 16, file_size)
struct.pack_into("<Q", data, 24, header_size)
struct.pack_into("<IHHHH", data, 32, section_count * section_entry_size,
                 section_entry_size, section_count, page_index_entry_size,
                 nav_index_entry_size)
struct.pack_into("<QQ", data, 44, page_data_offset, page_data_len)

def section(index, section_id, offset, length, entry_size, record_count):
    start = header_size + index * section_entry_size
    struct.pack_into("<H", data, start, section_id)
    struct.pack_into("<Q", data, start + 4, offset)
    struct.pack_into("<Q", data, start + 12, length)
    struct.pack_into("<II", data, start + 20, entry_size, record_count)

section(0, 1, string_offset, len(string_table), 0, 0)
section(1, 41, nav_offset, nav_count * nav_index_entry_size, nav_index_entry_size, nav_count)
section(2, 43, chapter_offset, chapter_count * chapter_index_entry_size,
        chapter_index_entry_size, chapter_count)
section(3, 40, page_index_offset, page_count * page_index_entry_size,
        page_index_entry_size, page_count)
section(4, 50, page_data_offset, page_data_len, 0, 0)

data[string_offset:string_offset + len(string_table)] = string_table

def write_nav(index, ordinal, title_start, title_len, page_index):
    start = nav_offset + index * nav_index_entry_size
    struct.pack_into("<IHHII", data, start, ordinal, 3, 0, title_start, title_len)
    struct.pack_into("<IIII", data, start + 28, page_index, 0xffffffff,
                     0xffffffff, 0xffffffff)

write_nav(0, 0, 0, 11, 0)
write_nav(1, 1, 11, 11, 1)

def write_chapter(index, ordinal, nav_index, title_start, title_len, page_index):
    start = chapter_offset + index * chapter_index_entry_size
    struct.pack_into("<IIIIIHH", data, start, ordinal, nav_index, title_start,
                     title_len, page_index, 0, 3)

write_chapter(0, 0, 0, 0, 11, 0)
write_chapter(1, 1, 1, 11, 11, 1)

half_page = page_data_len // page_count
for index in range(page_count):
    start = page_index_offset + index * page_index_entry_size
    struct.pack_into("<IHHH", data, start, index, 1, 2, 1)
    struct.pack_into("<Q", data, start + 16, index * half_page)
    struct.pack_into("<II", data, start + 24, half_page, 96000)
    struct.pack_into("<HH", data, start + 36, 800, 480)

pattern = bytes(((i * 17 + 91) & 0xff) for i in range(256))
for offset in range(page_data_len):
    data[page_data_offset + offset] = pattern[offset % len(pattern)]

path.write_bytes(data)
crc = zlib.crc32(data) & 0xffffffff
meta.write_text(f"SIZE={len(data)}\nCRC32={crc:08x}\n", encoding="utf-8")
PY
}

write_transfer_payload_meta() {
  local payload="$1"
  local meta="${payload}.meta"

  if [[ ! -s "${payload}" ]]; then
    printf 'Payload not found or empty: %s\n' "${payload}" >&2
    return 1
  fi
  python3 - "${payload}" "${meta}" <<'PY'
import pathlib
import sys
import zlib

path = pathlib.Path(sys.argv[1])
meta = pathlib.Path(sys.argv[2])
crc = 0
size = 0
with path.open("rb") as handle:
    while True:
        chunk = handle.read(65536)
        if not chunk:
            break
        size += len(chunk)
        crc = zlib.crc32(chunk, crc)
crc &= 0xffffffff
meta.write_text(f"SIZE={size}\nCRC32={crc:08x}\n", encoding="utf-8")
PY
}

read_transfer_payload_meta() {
  local payload="$1"
  local meta="${payload}.meta"

  if [[ ! -s "${meta}" ]]; then
    printf 'Payload metadata not found: %s\n' "${meta}" >&2
    return 1
  fi
  # shellcheck disable=SC1090
  source "${meta}"
  if [[ -z "${SIZE:-}" || -z "${CRC32:-}" ]]; then
    printf 'Payload metadata missing SIZE or CRC32: %s\n' "${meta}" >&2
    return 1
  fi
}
