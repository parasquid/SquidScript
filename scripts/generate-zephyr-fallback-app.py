#!/usr/bin/env python3
import pathlib
import sys


def c_bytes(data: bytes) -> str:
    chunks = []
    for index in range(0, len(data), 12):
        chunk = data[index : index + 12]
        chunks.append("\t" + ", ".join(f"0x{byte:02x}" for byte in chunk) + ",")
    return "\n".join(chunks)


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: generate-zephyr-fallback-app.py <sqbc> <app-id> <header> <source>", file=sys.stderr)
        return 2

    sqbc_path = pathlib.Path(sys.argv[1])
    app_id = sys.argv[2]
    header_path = pathlib.Path(sys.argv[3])
    source_path = pathlib.Path(sys.argv[4])
    data = sqbc_path.read_bytes()

    header_path.parent.mkdir(parents=True, exist_ok=True)
    source_path.parent.mkdir(parents=True, exist_ok=True)

    header_path.write_text(
        """#ifndef SQUIDSCRIPT_GENERATED_FALLBACK_APP_H
#define SQUIDSCRIPT_GENERATED_FALLBACK_APP_H

#include "fallback_app.h"

extern const struct sq_firmware_fallback_app sq_zephyr_fallback_app;

#endif
""",
        encoding="utf-8",
    )
    source_path.write_text(
        f"""#include "squidscript_fallback_app.h"

static const unsigned char sq_zephyr_fallback_app_sqbc[] = {{
{c_bytes(data)}
}};

const struct sq_firmware_fallback_app sq_zephyr_fallback_app = {{
\t.app_id = "{app_id}",
\t.sqbc = sq_zephyr_fallback_app_sqbc,
\t.sqbc_len = sizeof(sq_zephyr_fallback_app_sqbc),
}};
""",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
