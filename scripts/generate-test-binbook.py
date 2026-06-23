#!/usr/bin/env python3
"""Generate a minimal but valid .binbook test fixture with all required sections."""
import struct
import sys
from PIL import Image, ImageDraw, ImageFont

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
    chapter_data = bytearray()
    page_entries = []

    # Page 0: "White" — all white
    # Page 1: "Black" — all black
    # Page 2: "Stripes" — vertical stripes (white/black columns)
    # Page 3: "Bands" — horizontal bands (white/black rows)

    page_names = ["Black", "Checkerboard", "Diagonals", "Lorem Ipsum"]

    # Build string refs for page names
    name_offsets = []
    name_str = STRING_TABLE
    for name in page_names:
        name_offsets.append(len(name_str))
        name_str += name.encode("utf-8") + b"\0"
    # Also add "Page N" nav titles
    nav_title_offsets = []
    for i, name in enumerate(page_names):
        nav_title_offsets.append(len(name_str))
        title = f"Page {i + 1}: {name}"
        name_str += title.encode("utf-8") + b"\0"

    string_table_final = name_str

    # Build nav entries (4 pages)
    for i in range(4):
        nav_data.extend(struct.pack("<IHH", i, 3, 0))
        nav_data.extend(struct.pack("<II", nav_title_offsets[i], len(f"Page {i + 1}: {page_names[i]}".encode())))
        nav_data.extend(string_ref(REF_EMPTY))
        nav_data.extend(struct.pack("<I", 0xFFFFFFFF))
        nav_data.extend(struct.pack("<I", i))
        nav_data.extend(struct.pack("<III", 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF))
        nav_data.extend(struct.pack("<I", 0))

    # Build chapter entries (4 pages)
    for i in range(4):
        chapter_data.extend(struct.pack("<II", i, i))
        chapter_data.extend(struct.pack("<II", nav_title_offsets[i], len(f"Page {i + 1}: {page_names[i]}".encode())))
        chapter_data.extend(struct.pack("<I", i))
        chapter_data.extend(struct.pack("<HH", 0, 3))
        chapter_data.extend(struct.pack("<II", 0xFFFFFFFF, 0))

    ROW_BYTES = 800 // 4  # 200 bytes per row for GRAY2 packed

    def pack_gray2_row(pixels):
        """Pack 800 pixel values (0-3) into 200 bytes of GRAY2."""
        out = bytearray(ROW_BYTES)
        for x in range(800):
            byte_idx = x // 4
            shift = 6 - (x % 4) * 2
            out[byte_idx] |= (pixels[x] & 0x03) << shift
        return bytes(out)

    def gray2_fill(value):
        """Fill entire page with a single GRAY2 byte pattern."""
        if value == 0:
            return b"\x00" * UNCOMPRESSED_PAGE_BYTES
        if value == 3:
            return b"\xff" * UNCOMPRESSED_PAGE_BYTES
        b = 0
        for i in range(4):
            b |= (value & 0x03) << (6 - i * 2)
        return bytes([b]) * UNCOMPRESSED_PAGE_BYTES

    def make_page_row_pixels(row_y, gen_fn):
        """Build one row of 800 pixels via gen_fn(x, y)."""
        return [gen_fn(x, row_y) for x in range(800)]

    # Page 0: all black
    page0_raw = gray2_fill(0)

    # Page 1: checkerboard 40x24
    cell_w = 20
    cell_h = 20
    page1_rows = []
    for y in range(480):
        cy = y // cell_h
        row = []
        for x in range(800):
            cx = x // cell_w
            row.append(3 if (cx + cy) % 2 == 0 else 0)
        page1_rows.append(pack_gray2_row(row))
    page1_raw = b"".join(page1_rows)

    # Page 2: 40 diagonal stripes
    page2_rows = []
    for y in range(480):
        row = []
        for x in range(800):
            stripe_idx = (x + y) // 20
            row.append(3 if stripe_idx % 2 == 0 else 0)
        page2_rows.append(pack_gray2_row(row))
    page2_raw = b"".join(page2_rows)

    # Page 3: lorem ipsum text rendered with Pillow
    img = Image.new("L", (800, 480), 255)
    draw = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf", 18)
    except (IOError, OSError):
        try:
            font = ImageFont.truetype("/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf", 18)
        except (IOError, OSError):
            font = ImageFont.load_default()
    lorem = (
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod "
        "tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, "
        "quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo "
        "consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse "
        "cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat "
        "non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.\n\n"
        "Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium "
        "doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore "
        "veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim "
        "ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia "
        "consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt.\n\n"
        "At vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis "
        "praesentium voluptatum deleniti atque corrupti quos dolores et quas molestias "
        "excepturi sint occaecati cupiditate non provident, similique sunt in culpa qui "
        "officia deserunt mollitia animi, id est laborum et dolorum fuga."
    )
    draw.text((40, 20), lorem, fill=0, font=font)
    page3_pixels = list(img.tobytes())
    page3_rows = []
    for y in range(480):
        row = [max(0, min(3, page3_pixels[y * 800 + x] * 3 // 255)) for x in range(800)]
        page3_rows.append(pack_gray2_row(row))
    page3_raw = b"".join(page3_rows)

    page_raws = [page0_raw, page1_raw, page2_raw, page3_raw]

    # RLE-encode each page
    page_encoded = []
    for raw in page_raws:
        encoded = bytearray()
        i = 0
        while i < len(raw):
            # Check for a run of identical bytes
            run_start = i
            run_byte = raw[i]
            while i < len(raw) and raw[i] == run_byte and (i - run_start) < 128:
                i += 1
            run_len = i - run_start
            if run_len >= 2:
                # Literal run (packbits repeat)
                encoded.extend((0x80 | (run_len - 1), run_byte))
            else:
                # Check for a literal sequence
                lit_start = run_start
                while i < len(raw):
                    if i + 1 < len(raw) and raw[i] == raw[i + 1]:
                        break
                    i += 1
                    if (i - lit_start) >= 128:
                        break
                lit_len = i - lit_start
                encoded.extend((lit_len - 1,))
                encoded.extend(raw[lit_start:lit_start + lit_len])
        page_encoded.append(bytes(encoded))

    # Build page blob
    page_data = b""
    for enc in page_encoded:
        page_data += enc

    # Build page index
    page_index = bytearray()
    blob_offset = 0
    for i, enc in enumerate(page_encoded):
        page_index.extend(struct.pack("<IHHHHI", i, 1, 2, 1, 0, 0))
        page_index.extend(struct.pack("<Q", blob_offset))
        page_index.extend(struct.pack("<III", len(enc), UNCOMPRESSED_PAGE_BYTES, 0))
        page_index.extend(struct.pack("<HH", 800, 480))
        page_index.extend(struct.pack("<HH", 0, 0))
        page_index.extend(struct.pack("<II", 0xFFFFFFFF, 0xFFFFFFFF))
        page_index.extend(struct.pack("<II", i * 250000, (i + 1) * 250000))
        page_index.extend(bytes(16))
        blob_offset += len(enc)

    # Section data in order
    section_data = [
        string_table_final,      # 1
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
            record_count = 4
        elif sid == 41:
            entry_size = NAV_INDEX_ENTRY_SIZE
            record_count = 4
        elif sid == 43:
            entry_size = CHAPTER_INDEX_ENTRY_SIZE
            record_count = 4
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
