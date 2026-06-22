#!/usr/bin/env python3
"""Generate a minimal but valid .binbook test fixture with all required sections."""
import struct
import sys

HEADER_SIZE = 256
SECTION_ENTRY_SIZE = 40
PAGE_INDEX_ENTRY_SIZE = 76
NAV_INDEX_ENTRY_SIZE = 48
CHAPTER_INDEX_ENTRY_SIZE = 32
DISPLAY_PROFILE_SIZE = 120
LAYOUT_PROFILE_SIZE = 100
READER_REQUIREMENTS_SIZE = 76

REQUIRED_SECTION_IDS = [1, 10, 11, 12, 20, 21, 22, 30, 31, 32, 33, 34, 40, 41, 43, 50]
SECTION_COUNT = len(REQUIRED_SECTION_IDS)

STRING_TABLE = (
    b"\0"                          # 0: empty string
    b"Test Profile\0"              # 1: profile name
    b"xteink\0"                    # 14: family
    b"x4\0"                       # 21: model
    b"Test Book\0"                 # 24: title
    b"Test Author\0"              # 34: author
    b"en\0"                       # 47: language
    b"test.binbook\0"             # 50: filename
    b"generate-test-binbook\0"    # 63: compiler
    b"Pillow\0"                   # 85: renderer
    b"Literata\0"                 # 92: font_name
    b"fonts/Literata.ttf\0"      # 102: font_path
    b"Chapter One\0"              # 122: nav title 1
    b"Chapter Two\0"              # 134: nav title 2
)

# String refs: (offset, length)
REF_EMPTY = (0, 0)
REF_PROFILE = (1, 13)
REF_FAMILY = (14, 7)
REF_MODEL = (21, 3)
REF_TITLE = (24, 10)
REF_AUTHOR = (34, 12)
REF_LANGUAGE = (46, 3)
REF_FILENAME = (49, 13)
REF_COMPILER = (62, 22)
REF_RENDERER = (84, 7)
REF_FONT_NAME = (91, 9)
REF_FONT_PATH = (100, 19)
REF_CH1 = (119, 12)
REF_CH2 = (131, 12)

UNCOMPRESSED_PAGE_BYTES = 96000


def packbits_repeat(value, count):
    out = bytearray()
    while count > 0:
        run = min(count, 128)
        out.extend((0x80 | (run - 1), value))
        count -= run
    return bytes(out)


def string_ref(ref):
    return struct.pack("<II", ref[0], ref[1])


def put_u16(buf, offset, value):
    struct.pack_into("<H", buf, offset, value)


def put_u32(buf, offset, value):
    struct.pack_into("<I", buf, offset, value)


def put_u64(buf, offset, value):
    struct.pack_into("<Q", buf, offset, value)


def write_section(buf, offset, section_id, data_offset, length, entry_size, record_count, crc=0):
    put_u16(buf, offset, section_id)
    put_u64(buf, offset + 4, data_offset)
    put_u64(buf, offset + 12, length)
    put_u32(buf, offset + 20, entry_size)
    put_u32(buf, offset + 24, record_count)
    put_u32(buf, offset + 28, crc)


def build_display_profile():
    """DISPLAY_PROFILE (10): 120 bytes."""
    return b"".join([
        string_ref(REF_PROFILE),
        string_ref(REF_FAMILY),
        string_ref(REF_MODEL),
        struct.pack("<HHHHBhBIIHHHHBHB",
            800, 480,            # logical_width, logical_height
            800, 480,            # physical_width, physical_height
            0, 90,              # logical_orientation, logical_to_physical_rotation
            0,                   # scan_order_hint
            0b110,               # supported_storage_pixel_format_flags (GRAY1|GRAY2)
            0b110,               # required_storage_pixel_format_flags
            2,                   # default_storage_pixel_format (GRAY2)
            0,                   # reserved_pixel_format
            4,                   # native_grayscale_levels
            4,                   # required_grayscale_levels
            2,                   # framebuffer_bits_per_pixel
            1,                   # waveform_hint
            0,                   # dither_mode
        ),
        bytes(32), bytes(32),
    ])


def build_layout_profile():
    """LAYOUT_PROFILE (11): 100 bytes."""
    return struct.pack("<HHHHHHHHHHHHBB2sHHI32s32s",
        800, 480,    # full_width, full_height
        0, 0,       # header_height, footer_height
        0, 0, 0, 0, # margins
        0, 0,       # content_x, content_y
        800, 480,   # content_width, content_height
        1, 1,       # content_alignment, page_layout_mode
        bytes(2),   # reserved
        3000, 0,    # line_spacing_milli_em, paragraph_spacing_milli_em
        0,          # layout_flags
        bytes(32), bytes(32),
    )


def build_reader_requirements():
    """READER_REQUIREMENTS (12): 76 bytes."""
    page_bytes = (800 * 480 * 2 + 7) // 8
    return struct.pack("<QQIHHIHHII36s",
        (1 << 0) | (1 << 3),    # feature_flags
        (1 << 0) | (1 << 2) | (1 << 3) | (1 << 4),  # required_features
        0b110,                   # required_storage_pixel_format_flags
        4,                       # required_grayscale_levels
        0,                       # reserved0
        1 << 1,                  # required_compression_method_flags (RLE)
        800, 480,                # max_page_width, max_page_height
        page_bytes,              # max_uncompressed_page_bytes
        page_bytes * 2,          # recommended_working_buffer_bytes
        bytes(36),
    )


def build_source_identity():
    """SOURCE_IDENTITY (20)."""
    return b"".join([
        struct.pack("<HHQ", 0, 0, 0),  # source_type, reserved, file_size
        bytes(16),                       # md5
        bytes(32),                       # sha256
        string_ref(REF_FILENAME),
        string_ref(REF_EMPTY),          # package_identifier
        bytes(32),
    ])


def build_book_metadata():
    """BOOK_METADATA (21)."""
    return b"".join([
        string_ref(REF_TITLE),
        string_ref(REF_EMPTY),    # subtitle
        string_ref(REF_AUTHOR),
        string_ref(REF_EMPTY),    # publisher
        string_ref(REF_LANGUAGE),
        string_ref(REF_EMPTY),    # series_name
        struct.pack("<II", 0, 0), # series_index_milli, reserved
        bytes(32),
    ])


def build_rendition_identity():
    """RENDITION_IDENTITY (22)."""
    return b"".join([
        bytes(32 * 8),            # 8 empty string refs
        string_ref(REF_COMPILER),
        string_ref(REF_EMPTY),    # compiler_version
        struct.pack("<Q", 0),     # build_timestamp
        bytes(32),
    ])


def build_font_policy():
    """FONT_POLICY (30)."""
    return b"".join([
        struct.pack("<HH", 2, 1), # font_mode_force, force_custom_font flag
        bytes(32),                # font sha256
        string_ref(REF_FONT_NAME),
        string_ref(REF_FONT_PATH),
        string_ref(REF_RENDERER),
        bytes(32), bytes(32),
    ])


def build_typography_policy():
    """TYPOGRAPHY_POLICY (31)."""
    return b"".join([
        struct.pack("<HHHHIIHHiiBBBB", 24, 18, 0, 400, 1000, 1250, 0, 8, 0, 0, 1, 1, 1, 0),
        string_ref(REF_EMPTY),    # custom_css
        struct.pack("<I", 0),
        bytes(32), bytes(32),
    ])


def build_image_policy():
    """IMAGE_POLICY (32)."""
    return struct.pack("<HHHHHHHHHHHHI32s32s",
        1, 2, 1, 1,              # version, pixel_format, scaling, alignment
        1, 1, 3, 1000,           # dither, sharpen, white_value, contrast_ppm
        0, 0, 0, 0, 0,           # reserved
        bytes(32), bytes(32),
    )


def build_compression_policy():
    """COMPRESSION_POLICY (33)."""
    return struct.pack("<HIHHI32s32s",
        1,          # default_compression_method (RLE)
        1 << 1,     # supported_compression_methods
        1, 0, 0,    # min_ratio, reserved, reserved
        bytes(32), bytes(32),
    )


def build_chrome_policy():
    """CHROME_POLICY (34)."""
    return struct.pack("<4sHHI32s32s", bytes(4), 0, 0, 0, bytes(32), bytes(32))


def build():
    # Build all section data
    display_profile = build_display_profile()
    layout_profile = build_layout_profile()
    reader_requirements = build_reader_requirements()
    source_identity = build_source_identity()
    book_metadata = build_book_metadata()
    rendition_identity = build_rendition_identity()
    font_policy = build_font_policy()
    typography_policy = build_typography_policy()
    image_policy = build_image_policy()
    compression_policy = build_compression_policy()
    chrome_policy = build_chrome_policy()

    nav_data = bytearray()
    # Nav entry 0: "Chapter One", page 0
    nav_data.extend(struct.pack("<IHH", 0, 3, 0))  # nav_index, nav_type, level
    nav_data.extend(string_ref(REF_CH1))            # title
    nav_data.extend(string_ref(REF_EMPTY))          # source_href
    nav_data.extend(struct.pack("<I", 0xFFFFFFFF))  # source_spine_index
    nav_data.extend(struct.pack("<I", 0))           # target_page_number
    nav_data.extend(struct.pack("<III", 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF))  # parent, first_child, next_sibling
    nav_data.extend(struct.pack("<I", 0))           # nav_flags
    # Nav entry 1: "Chapter Two", page 1
    nav_data.extend(struct.pack("<IHH", 1, 3, 0))
    nav_data.extend(string_ref(REF_CH2))
    nav_data.extend(string_ref(REF_EMPTY))
    nav_data.extend(struct.pack("<I", 0xFFFFFFFF))
    nav_data.extend(struct.pack("<I", 1))
    nav_data.extend(struct.pack("<III", 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF))
    nav_data.extend(struct.pack("<I", 0))

    chapter_data = bytearray()
    # Chapter 0: index=0, nav_index=0, title="Chapter One", page=0
    chapter_data.extend(struct.pack("<II", 0, 0))       # chapter_index, nav_index
    chapter_data.extend(string_ref(REF_CH1))             # title
    chapter_data.extend(struct.pack("<I", 0))            # target_page_number
    chapter_data.extend(struct.pack("<HH", 0, 3))        # level, nav_type
    chapter_data.extend(struct.pack("<II", 0xFFFFFFFF, 0))  # source_spine_index, chapter_flags
    # Chapter 1: index=1, nav_index=1, title="Chapter Two", page=1
    chapter_data.extend(struct.pack("<II", 1, 1))
    chapter_data.extend(string_ref(REF_CH2))
    chapter_data.extend(struct.pack("<I", 1))
    chapter_data.extend(struct.pack("<HH", 0, 3))
    chapter_data.extend(struct.pack("<II", 0xFFFFFFFF, 0))

    page_one_data = packbits_repeat(0xFF, UNCOMPRESSED_PAGE_BYTES)
    page_two_data = packbits_repeat(0x00, UNCOMPRESSED_PAGE_BYTES)
    page_data = page_one_data + page_two_data

    # Build page index
    page_index = bytearray()
    page_index.extend(struct.pack("<IHHHHI", 0, 1, 2, 1, 0, 0))  # number, kind, pixel_fmt, compress, hint, flags
    page_index.extend(struct.pack("<Q", 0))                        # relative_blob_offset
    page_index.extend(struct.pack("<III", len(page_one_data), UNCOMPRESSED_PAGE_BYTES, 0))  # compressed, uncompressed, crc
    page_index.extend(struct.pack("<HH", 800, 480))               # stored_width, stored_height
    page_index.extend(struct.pack("<HH", 0, 0))                   # placement_x, placement_y
    page_index.extend(struct.pack("<II", 0xFFFFFFFF, 0xFFFFFFFF)) # source_spine, chapter_nav
    page_index.extend(struct.pack("<II", 0, 500000))              # progress_start_ppm, progress_end_ppm
    page_index.extend(bytes(16))                                    # reserved

    page_index.extend(struct.pack("<IHHHHI", 1, 1, 2, 1, 0, 0))
    page_index.extend(struct.pack("<Q", len(page_one_data)))
    page_index.extend(struct.pack("<III", len(page_two_data), UNCOMPRESSED_PAGE_BYTES, 0))
    page_index.extend(struct.pack("<HH", 800, 480))
    page_index.extend(struct.pack("<HH", 0, 0))
    page_index.extend(struct.pack("<II", 0xFFFFFFFF, 0xFFFFFFFF))
    page_index.extend(struct.pack("<II", 500000, 1000000))
    page_index.extend(bytes(16))

    # Section data in order
    section_data = [
        STRING_TABLE,            # 1
        display_profile,         # 10
        layout_profile,          # 11
        reader_requirements,     # 12
        source_identity,         # 20
        book_metadata,           # 21
        rendition_identity,      # 22
        font_policy,             # 30
        typography_policy,       # 31
        image_policy,            # 32
        compression_policy,      # 33
        chrome_policy,           # 34
        bytes(page_index),       # 40
        bytes(nav_data),         # 41
        bytes(chapter_data),     # 43
    ]
    # PAGE_DATA (50) handled separately

    # Calculate offsets
    section_table_end = HEADER_SIZE + SECTION_COUNT * SECTION_ENTRY_SIZE
    cursor = section_table_end
    section_offsets = []
    for data in section_data:
        section_offsets.append(cursor)
        cursor += len(data)

    page_data_offset = cursor
    total_len = page_data_offset + len(page_data)

    # Build output
    out = bytearray(total_len)
    out[0:8] = b"BINBOOK\0"
    put_u16(out, 12, HEADER_SIZE)
    put_u64(out, 16, total_len)
    put_u64(out, 24, HEADER_SIZE)
    put_u32(out, 32, SECTION_COUNT * SECTION_ENTRY_SIZE)
    put_u16(out, 36, SECTION_ENTRY_SIZE)
    put_u16(out, 38, SECTION_COUNT)
    put_u16(out, 40, PAGE_INDEX_ENTRY_SIZE)
    put_u16(out, 42, NAV_INDEX_ENTRY_SIZE)
    put_u64(out, 44, page_data_offset)
    put_u64(out, 52, len(page_data))

    # Write section table
    section_table = HEADER_SIZE
    for i, (sid, data) in enumerate(zip(REQUIRED_SECTION_IDS, section_data)):
        entry_size = 0
        record_count = 0
        if sid == 40:
            entry_size = PAGE_INDEX_ENTRY_SIZE
            record_count = 2
        elif sid == 41:
            entry_size = NAV_INDEX_ENTRY_SIZE
            record_count = 2
        elif sid == 43:
            entry_size = CHAPTER_INDEX_ENTRY_SIZE
            record_count = 2
        write_section(out, section_table + i * SECTION_ENTRY_SIZE, sid,
                      section_offsets[i], len(data), entry_size, record_count)
    # PAGE_DATA (50) section entry
    write_section(out, section_table + (SECTION_COUNT - 1) * SECTION_ENTRY_SIZE, 50,
                  page_data_offset, len(page_data), 0, 0)

    # Write section data
    for i, data in enumerate(section_data):
        out[section_offsets[i]:section_offsets[i] + len(data)] = data

    # Write page data
    out[page_data_offset:page_data_offset + len(page_data)] = page_data
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
