#!/usr/bin/env python3
"""Generate a current, visually distinctive BinBook hardware-test fixture."""

from pathlib import Path
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont


WIDTH = 480
HEIGHT = 800


def load_font(size: int):
    for path in (
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ):
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            pass
    return ImageFont.load_default()


def write_pages(directory: Path) -> None:
    font = load_font(42)
    small_font = load_font(24)

    first = Image.new("L", (WIDTH, HEIGHT), 255)
    draw = ImageDraw.Draw(first)
    draw.rectangle((16, 16, WIDTH - 17, HEIGHT - 17), outline=0, width=8)
    draw.text((48, 80), "HTTP UPLOAD", fill=0, font=font)
    draw.text((48, 150), "PAGE 1", fill=0, font=font)
    for y in range(260, 700, 48):
        draw.line((48, y, WIDTH - 48, y), fill=0, width=8)
    draw.text((48, 730), "SquidScript", fill=0, font=small_font)
    first.save(directory / "page-01.png")

    second = Image.new("L", (WIDTH, HEIGHT), 255)
    draw = ImageDraw.Draw(second)
    draw.rectangle((16, 16, WIDTH - 17, HEIGHT - 17), outline=0, width=8)
    draw.text((48, 80), "HTTP UPLOAD", fill=0, font=font)
    draw.text((48, 150), "PAGE 2", fill=0, font=font)
    for x in range(48, WIDTH - 48, 48):
        draw.line((x, 260, x, 700), fill=0, width=8)
    draw.text((48, 730), "SquidScript", fill=0, font=small_font)
    second.save(directory / "page-02.png")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: generate-test-binbook.py <out.binbook>", file=sys.stderr)
        return 2

    root = Path(__file__).resolve().parents[1]
    binbook_workspace = root.parent / "binbook"
    output = Path(sys.argv[1]).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="squidscript-binbook-") as temp:
        page_dir = Path(temp)
        write_pages(page_dir)
        subprocess.run(
            [
                "cargo",
                "run",
                "--quiet",
                "--manifest-path",
                str(binbook_workspace / "Cargo.toml"),
                "-p",
                "binbook",
                "--",
                "encode",
                str(page_dir),
                "--output",
                str(output),
            ],
            check=True,
            env={**os.environ, "RUSTUP_TOOLCHAIN": "stable"},
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
