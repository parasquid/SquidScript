#!/usr/bin/env python3
import pathlib
import re
import subprocess
import sys
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]


def fail(message: str) -> int:
    print(f"error: {message}", file=sys.stderr)
    return 1


def symbol_for(path: pathlib.Path) -> str:
    stem = path.stem.replace("-", "_")
    if not re.fullmatch(r"[a-zA-Z_][a-zA-Z0-9_]*", stem):
        raise ValueError(f"invalid fixture stem for C symbol: {path.name}")
    return f"{stem}_sqbc"


def write_array(out, name: str, data: bytes) -> None:
    out.write(f"static const uint8_t {name}[] = {{\n")
    for offset in range(0, len(data), 12):
        chunk = data[offset : offset + 12]
        values = ", ".join(f"0x{byte:02x}" for byte in chunk)
        out.write(f"\t{values},\n")
    out.write("};\n\n")


def main() -> int:
    if len(sys.argv) < 3:
        return fail("usage: generate-zephyr-protocol-fixtures.py <header> <fixture.squid>...")

    header = pathlib.Path(sys.argv[1])
    sources = sorted(pathlib.Path(arg) for arg in sys.argv[2:])
    if not sources:
        return fail("at least one fixture source is required")

    header.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = pathlib.Path(tmp)
        compiled = []
        for source in sources:
            if source.suffix != ".squid":
                return fail(f"fixture source must use .squid extension: {source}")
            sqbc = tmp_path / f"{source.stem}.sqbc"
            result = subprocess.run(
                [
                    "cargo",
                    "run",
                    "-p",
                    "squidc",
                    "--",
                    "app",
                    "build",
                    str(source),
                    "--out",
                    str(sqbc),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )
            if result.returncode != 0:
                sys.stderr.write(result.stdout)
                sys.stderr.write(result.stderr)
                return fail(f"failed to compile {source}")
            compiled.append((source, sqbc.read_bytes()))

    with header.open("w", encoding="utf-8", newline="\n") as out:
        out.write("/* Generated from firmware/zephyr/tests/protocol/fixtures. */\n")
        out.write("#pragma once\n\n")
        out.write("#include <stdint.h>\n\n")
        for source, data in compiled:
            out.write(f"/* {source.name} */\n")
            write_array(out, symbol_for(source), data)

    return 0


if __name__ == "__main__":
    sys.exit(main())
