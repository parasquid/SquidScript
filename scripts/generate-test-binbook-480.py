#!/usr/bin/env python3
"""Generate a 2-page .binbook test fixture for 480x480 GRAY2 targets.

Page 0: 20x20 checkerboard (black/white)
Page 1: Dense lorem ipsum text

Usage: generate-test-binbook-480.py <out.binbook>
"""
import struct
import sys
from PIL import Image, ImageDraw, ImageFont

HEADER_SIZE = 256
SECTION_ENTRY_SIZE = 40
PAGE_INDEX_ENTRY_SIZE = 128
NAV_INDEX_ENTRY_SIZE = 48
CHAPTER_INDEX_ENTRY_SIZE = 32

PANEL_WIDTH = 480
PANEL_HEIGHT = 480
ROW_BYTES = PANEL_WIDTH // 4  # 120 bytes per row
UNCOMPRESSED_PAGE_BYTES = ROW_BYTES * PANEL_HEIGHT  # 57600

SECTION_IDS = [1, 10, 11, 12, 20, 21, 22, 30, 31, 32, 33, 34, 40, 41, 43, 50]
SECTION_COUNT = len(SECTION_IDS)

STRING_TABLE = (
    b"\x00"
    b"Test Profile\x00"
    b"xteink\x00"
    b"x4\x00"
    b"Test Book\x00"
    b"Test Author\x00"
    b"en\x00"
    b"test-480.binbook\x00"
    b"generate-test-binbook-480\x00"
    b"Pillow\x00"
    b"Literata\x00"
    b"fonts/Literata.ttf\x00"
    b"Checkerboard\x00"
    b"Text Page\x00"
)

REF_EMPTY = (0, 0)
REF_PROFILE = (1, 13)
REF_FAMILY = (14, 7)
REF_MODEL = (21, 3)
REF_TITLE = (24, 10)
REF_AUTHOR = (34, 12)
REF_LANGUAGE = (46, 3)
REF_FILENAME = (49, 16)
REF_COMPILER = (65, 25)
REF_RENDERER = (90, 7)
REF_FONT_NAME = (97, 9)
REF_FONT_PATH = (106, 19)
REF_CH1 = (125, 12)
REF_CH2 = (137, 9)


def string_ref(ref):
    return struct.pack("<II", ref[0], ref[1])


def put_u16(buf, off, val):
    struct.pack_into("<H", buf, off, val)


def put_u32(buf, off, val):
    struct.pack_into("<I", buf, off, val)


def put_u64(buf, off, val):
    struct.pack_into("<Q", buf, off, val)


def write_section(buf, off, sid, data_off, length, entry_size, count, crc=0):
    put_u16(buf, off, sid)
    put_u64(buf, off + 4, data_off)
    put_u64(buf, off + 12, length)
    put_u32(buf, off + 20, entry_size)
    put_u32(buf, off + 24, count)
    put_u32(buf, off + 28, crc)


def build_display_profile():
    return b"".join([
        string_ref(REF_PROFILE),
        string_ref(REF_FAMILY),
        string_ref(REF_MODEL),
        struct.pack("<HHHHBhBIIHHHHBHB",
            PANEL_WIDTH, PANEL_HEIGHT,
            PANEL_WIDTH, PANEL_HEIGHT,
            0, 90, 0,
            0b110, 0b110, 2, 0, 4, 4, 2, 1, 0,
        ),
        bytes(32), bytes(32),
    ])


def build_layout_profile():
    return struct.pack("<HHHHHHHHHHHHBB2sHHI32s32s",
        PANEL_WIDTH, PANEL_HEIGHT,
        0, 0, 0, 0, 0, 0,
        PANEL_WIDTH, PANEL_HEIGHT,
        PANEL_WIDTH, PANEL_HEIGHT,
        1, 1, bytes(2), 3000, 0, 0, bytes(32), bytes(32),
    )


def build_reader_requirements():
    return struct.pack("<QQIHHIHHII36s",
        (1 << 0) | (1 << 3),
        (1 << 0) | (1 << 2) | (1 << 3) | (1 << 4),
        0b110, 4, 0, 1 << 1,
        PANEL_WIDTH, PANEL_HEIGHT,
        UNCOMPRESSED_PAGE_BYTES,
        UNCOMPRESSED_PAGE_BYTES * 2,
        bytes(36),
    )


def build_source_identity():
    return b"".join([
        struct.pack("<HHQ", 0, 0, 0),
        bytes(16), bytes(32),
        string_ref(REF_FILENAME),
        string_ref(REF_EMPTY),
        bytes(32),
    ])


def build_book_metadata():
    return b"".join([
        string_ref(REF_TITLE),
        string_ref(REF_EMPTY),
        string_ref(REF_AUTHOR),
        string_ref(REF_EMPTY),
        string_ref(REF_LANGUAGE),
        string_ref(REF_EMPTY),
        struct.pack("<II", 0, 0),
        bytes(32),
    ])


def build_rendition_identity():
    return b"".join([
        bytes(32 * 8),
        string_ref(REF_COMPILER),
        string_ref(REF_EMPTY),
        struct.pack("<Q", 0),
        bytes(32),
    ])


def build_font_policy():
    return b"".join([
        struct.pack("<HH", 2, 1),
        bytes(32),
        string_ref(REF_FONT_NAME),
        string_ref(REF_FONT_PATH),
        string_ref(REF_RENDERER),
        bytes(32), bytes(32),
    ])


def build_typography_policy():
    return b"".join([
        struct.pack("<HHHHIIHHiiBBBB", 24, 18, 0, 400, 1000, 1250, 0, 8, 0, 0, 1, 1, 1, 0),
        string_ref(REF_EMPTY),
        struct.pack("<I", 0),
        bytes(32), bytes(32),
    ])


def build_image_policy():
    return struct.pack("<HHHHHHHHHHHHI32s32s",
        1, 2, 1, 1, 1, 1, 3, 1000, 0, 0, 0, 0, 0, bytes(32), bytes(32),
    )


def build_compression_policy():
    return struct.pack("<HIHHI32s32s",
        1, 1 << 1, 1, 0, 0, bytes(32), bytes(32),
    )


def build_chrome_policy():
    return struct.pack("<4sHHI32s32s", bytes(4), 0, 0, 0, bytes(32), bytes(32))


def pack_gray2_row(pixels):
    out = bytearray(ROW_BYTES)
    for x in range(PANEL_WIDTH):
        byte_idx = x // 4
        shift = 6 - (x % 4) * 2
        out[byte_idx] |= (pixels[x] & 0x03) << shift
    return bytes(out)


def rle_encode(raw):
    encoded = bytearray()
    i = 0
    while i < len(raw):
        run_start = i
        run_byte = raw[i]
        while i < len(raw) and raw[i] == run_byte and (i - run_start) < 128:
            i += 1
        run_len = i - run_start
        if run_len >= 2:
            encoded.extend((0x80 | (run_len - 1), run_byte))
        else:
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
    return bytes(encoded)


def build_checkerboard(cell_size=20):
    rows = []
    for y in range(PANEL_HEIGHT):
        cy = y // cell_size
        row = []
        for x in range(PANEL_WIDTH):
            cx = x // cell_size
            row.append(3 if (cx + cy) % 2 == 0 else 0)
        rows.append(pack_gray2_row(row))
    return b"".join(rows)


def build_text_page():
    img = Image.new("L", (PANEL_WIDTH, PANEL_HEIGHT), 255)
    draw = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf", 16)
    except (IOError, OSError):
        try:
            font = ImageFont.truetype("/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf", 16)
        except (IOError, OSError):
            font = ImageFont.load_default()
    text = (
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
        "officia deserunt mollitia animi, id est laborum et dolorum fuga.\n\n"
        "Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, "
        "adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et "
        "dolore magnam aliquam quaerat voluptatem."
    )
    draw.text((20, 16), text, fill=0, font=font)
    pixels = list(img.tobytes())
    rows = []
    for y in range(PANEL_HEIGHT):
        row = [max(0, min(3, pixels[y * PANEL_WIDTH + x] * 3 // 255)) for x in range(PANEL_WIDTH)]
        rows.append(pack_gray2_row(row))
    return b"".join(rows)


def build():
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

    page_names = ["Checkerboard", "Text Page"]
    name_str = STRING_TABLE
    name_offsets = []
    for name in page_names:
        name_offsets.append(len(name_str))
        name_str += name.encode("utf-8") + b"\x00"
    nav_title_offsets = []
    for i, name in enumerate(page_names):
        nav_title_offsets.append(len(name_str))
        title = f"Page {i + 1}: {name}"
        name_str += title.encode("utf-8") + b"\x00"
    string_table_final = name_str

    nav_data = bytearray()
    chapter_data = bytearray()
    for i in range(2):
        nav_data.extend(struct.pack("<IHH", i, 3, 0))
        nav_data.extend(struct.pack("<II", nav_title_offsets[i], len(f"Page {i + 1}: {page_names[i]}".encode())))
        nav_data.extend(string_ref(REF_EMPTY))
        nav_data.extend(struct.pack("<I", 0xFFFFFFFF))
        nav_data.extend(struct.pack("<I", i))
        nav_data.extend(struct.pack("<III", 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF))
        nav_data.extend(struct.pack("<I", 0))

        chapter_data.extend(struct.pack("<II", i, i))
        chapter_data.extend(struct.pack("<II", nav_title_offsets[i], len(f"Page {i + 1}: {page_names[i]}".encode())))
        chapter_data.extend(struct.pack("<I", i))
        chapter_data.extend(struct.pack("<HH", 0, 3))
        chapter_data.extend(struct.pack("<II", 0xFFFFFFFF, 0))

    raw_pages = [build_checkerboard(), build_text_page()]
    encoded_pages = [rle_encode(raw) for raw in raw_pages]

    page_data = b"".join(encoded_pages)

    page_index = bytearray()
    blob_offset = 0
    for i, enc in enumerate(encoded_pages):
        page_index.extend(struct.pack("<IHHHH I I HHHH II II",
                                      i, 1, 2, 1, 0,
                                      0, 0,
                                      PANEL_WIDTH, PANEL_HEIGHT,
                                      0, 0,
                                      0xFFFFFFFF, i,
                                      i * 250000, (i + 1) * 250000))
        plane_bitmap = 0x01
        plane_compression = bytes([1, 0, 0, 0])
        page_index.extend(bytes([plane_bitmap]))
        page_index.extend(plane_compression)
        page_index.extend(bytes(3))
        page_index.extend(struct.pack("<4I", blob_offset, 0, 0, 0))
        page_index.extend(struct.pack("<4I", len(enc), 0, 0, 0))
        page_index.extend(bytes(44))
        blob_offset += len(enc)

    section_data = [
        string_table_final, display_profile, layout_profile, reader_requirements,
        source_identity, book_metadata, rendition_identity, font_policy,
        typography_policy, image_policy, compression_policy, chrome_policy,
        bytes(page_index), bytes(nav_data), bytes(chapter_data),
    ]

    section_table_end = HEADER_SIZE + SECTION_COUNT * SECTION_ENTRY_SIZE
    cursor = section_table_end
    section_offsets = []
    for data in section_data:
        section_offsets.append(cursor)
        cursor += len(data)

    page_data_offset = cursor
    total_len = page_data_offset + len(page_data)

    out = bytearray(total_len)
    out[0:8] = b"BINBOOK\x00"
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

    section_table = HEADER_SIZE
    for i, (sid, data) in enumerate(zip(SECTION_IDS, section_data)):
        entry_size = 0
        count = 0
        if sid == 40:
            entry_size = PAGE_INDEX_ENTRY_SIZE
            count = 2
        elif sid == 41:
            entry_size = NAV_INDEX_ENTRY_SIZE
            count = 2
        elif sid == 43:
            entry_size = CHAPTER_INDEX_ENTRY_SIZE
            count = 2
        write_section(out, section_table + i * SECTION_ENTRY_SIZE, sid,
                      section_offsets[i], len(data), entry_size, count)
    write_section(out, section_table + (SECTION_COUNT - 1) * SECTION_ENTRY_SIZE, 50,
                  page_data_offset, len(page_data), 0, 0)

    for i, data in enumerate(section_data):
        out[section_offsets[i]:section_offsets[i] + len(data)] = data
    out[page_data_offset:page_data_offset + len(page_data)] = page_data

    return bytes(out)


def main():
    if len(sys.argv) != 2:
        print("usage: generate-test-binbook-480.py <out.binbook>", file=sys.stderr)
        return 2
    with open(sys.argv[1], "wb") as handle:
        handle.write(build())
    print(f"Wrote {sys.argv[1]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
