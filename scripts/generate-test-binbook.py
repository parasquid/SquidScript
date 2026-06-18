#!/usr/bin/env python3
import struct
import sys

HEADER_SIZE = 256
SECTION_ENTRY_SIZE = 40
PAGE_INDEX_ENTRY_SIZE = 76
NAV_INDEX_ENTRY_SIZE = 48
CHAPTER_INDEX_ENTRY_SIZE = 32
SECTION_COUNT = 5
STRING_TABLE = b"Chapter OneChapter Two"
STRING_TABLE_OFFSET = HEADER_SIZE + SECTION_COUNT * SECTION_ENTRY_SIZE
NAV_INDEX_OFFSET = STRING_TABLE_OFFSET + len(STRING_TABLE)
NAV_INDEX_COUNT = 2
NAV_INDEX_LEN = NAV_INDEX_COUNT * NAV_INDEX_ENTRY_SIZE
CHAPTER_INDEX_OFFSET = NAV_INDEX_OFFSET + NAV_INDEX_LEN
CHAPTER_INDEX_COUNT = 2
CHAPTER_INDEX_LEN = CHAPTER_INDEX_COUNT * CHAPTER_INDEX_ENTRY_SIZE
PAGE_INDEX_OFFSET = CHAPTER_INDEX_OFFSET + CHAPTER_INDEX_LEN
PAGE_INDEX_COUNT = 2
PAGE_INDEX_LEN = PAGE_INDEX_COUNT * PAGE_INDEX_ENTRY_SIZE
PAGE_DATA_OFFSET = PAGE_INDEX_OFFSET + PAGE_INDEX_LEN
UNCOMPRESSED_PAGE_BYTES = 96000


def packbits_repeat(value, count):
    out = bytearray()
    while count > 0:
        run = min(count, 128)
        out.extend((0x80 | (run - 1), value))
        count -= run
    return bytes(out)


PAGE_ONE_DATA = packbits_repeat(0xFF, UNCOMPRESSED_PAGE_BYTES)
PAGE_TWO_DATA = packbits_repeat(0x00, UNCOMPRESSED_PAGE_BYTES)
PAGE_DATA = PAGE_ONE_DATA + PAGE_TWO_DATA
TOTAL_LEN = PAGE_DATA_OFFSET + len(PAGE_DATA)


def put_u16(buf, offset, value):
    struct.pack_into("<H", buf, offset, value)


def put_u32(buf, offset, value):
    struct.pack_into("<I", buf, offset, value)


def put_u64(buf, offset, value):
    struct.pack_into("<Q", buf, offset, value)


def write_section(buf, offset, section_id, data_offset, length, entry_size, record_count):
    put_u16(buf, offset, section_id)
    put_u64(buf, offset + 4, data_offset)
    put_u64(buf, offset + 12, length)
    put_u32(buf, offset + 20, entry_size)
    put_u32(buf, offset + 24, record_count)


def build():
    out = bytearray(TOTAL_LEN)
    out[0:8] = b"BINBOOK\0"
    put_u16(out, 12, HEADER_SIZE)
    put_u64(out, 16, TOTAL_LEN)
    put_u64(out, 24, HEADER_SIZE)
    put_u32(out, 32, SECTION_COUNT * SECTION_ENTRY_SIZE)
    put_u16(out, 36, SECTION_ENTRY_SIZE)
    put_u16(out, 38, SECTION_COUNT)
    put_u16(out, 40, PAGE_INDEX_ENTRY_SIZE)
    put_u16(out, 42, NAV_INDEX_ENTRY_SIZE)
    put_u64(out, 44, PAGE_DATA_OFFSET)
    put_u64(out, 52, len(PAGE_DATA))

    section = HEADER_SIZE
    write_section(out, section, 1, STRING_TABLE_OFFSET, len(STRING_TABLE), 0, 0)
    write_section(out, section + SECTION_ENTRY_SIZE, 41, NAV_INDEX_OFFSET, NAV_INDEX_LEN, NAV_INDEX_ENTRY_SIZE, NAV_INDEX_COUNT)
    write_section(out, section + 2 * SECTION_ENTRY_SIZE, 43, CHAPTER_INDEX_OFFSET, CHAPTER_INDEX_LEN, CHAPTER_INDEX_ENTRY_SIZE, CHAPTER_INDEX_COUNT)
    write_section(out, section + 3 * SECTION_ENTRY_SIZE, 40, PAGE_INDEX_OFFSET, PAGE_INDEX_LEN, PAGE_INDEX_ENTRY_SIZE, PAGE_INDEX_COUNT)
    write_section(out, section + 4 * SECTION_ENTRY_SIZE, 50, PAGE_DATA_OFFSET, len(PAGE_DATA), 0, 0)

    out[STRING_TABLE_OFFSET:STRING_TABLE_OFFSET + len(STRING_TABLE)] = STRING_TABLE

    put_u32(out, NAV_INDEX_OFFSET, 0)
    put_u16(out, NAV_INDEX_OFFSET + 4, 3)
    put_u16(out, NAV_INDEX_OFFSET + 6, 0)
    put_u32(out, NAV_INDEX_OFFSET + 8, 0)
    put_u32(out, NAV_INDEX_OFFSET + 12, 11)
    put_u32(out, NAV_INDEX_OFFSET + 28, 0)
    put_u32(out, NAV_INDEX_OFFSET + 32, 0xFFFFFFFF)
    put_u32(out, NAV_INDEX_OFFSET + 36, 0xFFFFFFFF)
    put_u32(out, NAV_INDEX_OFFSET + 40, 0xFFFFFFFF)

    nav_two = NAV_INDEX_OFFSET + NAV_INDEX_ENTRY_SIZE
    put_u32(out, nav_two, 1)
    put_u16(out, nav_two + 4, 3)
    put_u16(out, nav_two + 6, 0)
    put_u32(out, nav_two + 8, 11)
    put_u32(out, nav_two + 12, 11)
    put_u32(out, nav_two + 28, 1)
    put_u32(out, nav_two + 32, 0xFFFFFFFF)
    put_u32(out, nav_two + 36, 0xFFFFFFFF)
    put_u32(out, nav_two + 40, 0xFFFFFFFF)

    put_u32(out, CHAPTER_INDEX_OFFSET, 0)
    put_u32(out, CHAPTER_INDEX_OFFSET + 4, 0)
    put_u32(out, CHAPTER_INDEX_OFFSET + 8, 0)
    put_u32(out, CHAPTER_INDEX_OFFSET + 12, 11)
    put_u32(out, CHAPTER_INDEX_OFFSET + 16, 0)
    put_u16(out, CHAPTER_INDEX_OFFSET + 20, 0)
    put_u16(out, CHAPTER_INDEX_OFFSET + 22, 3)

    chapter_two = CHAPTER_INDEX_OFFSET + CHAPTER_INDEX_ENTRY_SIZE
    put_u32(out, chapter_two, 1)
    put_u32(out, chapter_two + 4, 11)
    put_u32(out, chapter_two + 8, 11)
    put_u32(out, chapter_two + 12, 11)
    put_u32(out, chapter_two + 16, 1)
    put_u16(out, chapter_two + 20, 0)
    put_u16(out, chapter_two + 22, 3)

    put_u32(out, PAGE_INDEX_OFFSET, 0)
    put_u16(out, PAGE_INDEX_OFFSET + 4, 1)
    put_u16(out, PAGE_INDEX_OFFSET + 6, 2)
    put_u16(out, PAGE_INDEX_OFFSET + 8, 1)
    put_u64(out, PAGE_INDEX_OFFSET + 16, 0)
    put_u32(out, PAGE_INDEX_OFFSET + 24, len(PAGE_ONE_DATA))
    put_u32(out, PAGE_INDEX_OFFSET + 28, UNCOMPRESSED_PAGE_BYTES)
    put_u16(out, PAGE_INDEX_OFFSET + 36, 800)
    put_u16(out, PAGE_INDEX_OFFSET + 38, 480)

    page_two = PAGE_INDEX_OFFSET + PAGE_INDEX_ENTRY_SIZE
    put_u32(out, page_two, 1)
    put_u16(out, page_two + 4, 1)
    put_u16(out, page_two + 6, 2)
    put_u16(out, page_two + 8, 1)
    put_u64(out, page_two + 16, len(PAGE_ONE_DATA))
    put_u32(out, page_two + 24, len(PAGE_TWO_DATA))
    put_u32(out, page_two + 28, UNCOMPRESSED_PAGE_BYTES)
    put_u16(out, page_two + 36, 800)
    put_u16(out, page_two + 38, 480)

    out[PAGE_DATA_OFFSET:PAGE_DATA_OFFSET + len(PAGE_DATA)] = PAGE_DATA
    return out


def main():
    if len(sys.argv) != 2:
        print("usage: generate-test-binbook.py <out.binbook>", file=sys.stderr)
        return 2
    with open(sys.argv[1], "wb") as handle:
        handle.write(build())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
