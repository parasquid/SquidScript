#!/usr/bin/env python3
"""Validate the SquidScript Zephyr VM host ABI manifest."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BEGIN_MARKER = "<!-- BEGIN SQUIDVM_FFI_ABI_MANIFEST -->"
END_MARKER = "<!-- END SQUIDVM_FFI_ABI_MANIFEST -->"


RUST_EXPORT_RE = re.compile(
    r'#\[no_mangle\]\s*pub\s+(?:unsafe\s+)?extern\s+"C"\s+fn\s+'
    r"([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
HEADER_PROTO_RE = re.compile(
    r"(?m)^(?:[A-Za-z_][A-Za-z0-9_]*|void|size_t)\s+"
    r"((?:sqvm|sqdp|sqdc)_[A-Za-z0-9_]+)\s*\("
)
TYPEDEF_NAME_RE = re.compile(r"}\s*([A-Za-z_][A-Za-z0-9_]*)\s*;")
CALLBACK_TYPEDEF_RE = re.compile(
    r"typedef\s+[^;()]+\(\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*\(",
    re.MULTILINE,
)
CONSTANT_RE = re.compile(r"(?m)^#define\s+((?:SQVM|SQDP|SQDC)_[A-Za-z0-9_]+)\b")


def rel(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT.resolve()))
    except ValueError:
        return str(path)


def load_manifest(path: Path) -> dict:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise CheckError(f"manifest not found: {rel(path)}") from None
    if manifest.get("format") != "squidscript-squidvm-ffi-abi-v1":
        raise CheckError("manifest format must be squidscript-squidvm-ffi-abi-v1")
    return manifest


def unique(values: list[str]) -> list[str]:
    return list(dict.fromkeys(values))


def parse_rust_exports(rust: str) -> set[str]:
    return set(RUST_EXPORT_RE.findall(rust))


def parse_header_prototypes(header: str) -> set[str]:
    return set(HEADER_PROTO_RE.findall(header))


def parse_header_types(header: str) -> set[str]:
    return set(unique(TYPEDEF_NAME_RE.findall(header) + CALLBACK_TYPEDEF_RE.findall(header)))


def parse_header_constants(header: str) -> set[str]:
    return set(CONSTANT_RE.findall(header))


def extract_rust_callbacks(rust: str) -> set[str]:
    match = re.search(
        r"pub\s+struct\s+SqvmCallbacks\s*\{(?P<body>.*?)\n\}",
        rust,
        flags=re.DOTALL,
    )
    if not match:
        return set()
    return set(re.findall(r"\bpub\s+([A-Za-z_][A-Za-z0-9_]*)\s*:", match.group("body")))


def extract_header_callbacks(header: str) -> dict[str, str]:
    matches = list(re.finditer(r"typedef\s+struct\s*\{(?P<body>.*?)\}\s*SqvmCallbacks\s*;", header, re.DOTALL))
    if not matches:
        return {}
    body = matches[-1].group("body")
    callbacks: dict[str, str] = {}
    for match in re.finditer(
        r"\b(?P<typedef>Sqvm[A-Za-z0-9_]*Callback)\s+(?P<field>[A-Za-z_][A-Za-z0-9_]*)\s*;",
        body,
    ):
        callbacks[match.group("field")] = match.group("typedef")
    return callbacks


def extract_runtime_callbacks(runtime: str) -> set[str]:
    match = re.search(
        r"static\s+const\s+SqvmCallbacks\s+runtime_callbacks\s*=\s*\{(?P<body>.*?)\n\}\s*;",
        runtime,
        flags=re.DOTALL,
    )
    if not match:
        return set()
    return set(re.findall(r"\.([A-Za-z_][A-Za-z0-9_]*)\s*=", match.group("body")))


def manifest_export_names(manifest: dict, *, require_rust: bool | None = None) -> set[str]:
    names = set()
    for item in manifest.get("exports", []):
        direction = item.get("direction", "rust_to_c")
        if require_rust is True and direction != "rust_to_c":
            continue
        if require_rust is False and direction == "rust_to_c":
            continue
        names.add(item["name"])
    return names


def emit_markdown(manifest: dict) -> str:
    exports = manifest.get("exports", [])
    callbacks = manifest.get("callbacks", [])
    coverage = manifest.get("coverage", [])

    lines = [
        BEGIN_MARKER,
        "## Manifest-Checked ABI Inventory",
        "",
        "This section is generated from `compiler/rust/crates/squidvm-ffi/abi/manifest.json`.",
        "Run `python3 scripts/check-squidvm-ffi-abi.py --write-doc` after changing the FFI ABI.",
        "The checker validates Rust exports, the Zephyr C header, runtime callback wiring,",
        "and this generated documentation section against the manifest.",
        "",
        f"- Exports: {len(exports)}",
        f"- Callback fields: {len(callbacks)}",
        f"- Public ABI types: {len(manifest.get('types', []))}",
        f"- Public ABI constants: {len(manifest.get('constants', []))}",
        "",
        "### Export Families",
        "",
        "| Family | Direction | Symbols |",
        "| --- | --- | --- |",
    ]

    export_groups: dict[tuple[str, str], list[str]] = {}
    for item in exports:
        key = (item.get("family", "uncategorized"), item.get("direction", "rust_to_c"))
        export_groups.setdefault(key, []).append(item["name"])
    for (family, direction), names in sorted(export_groups.items()):
        lines.append(f"| {family} | {direction} | {len(names)} |")

    lines.extend(
        [
            "",
            "### Callback Coverage Expectations",
            "",
        "| Family | Callbacks | Rust coverage | Zephyr coverage | Evidence checks |",
        "| --- | --- | --- | --- | ---: |",
    ]
    )
    for item in coverage:
        callbacks_text = ", ".join(item.get("callbacks", []))
        evidence_count = len(item.get("rust_tests", [])) + len(item.get("zephyr_tests", []))
        lines.append(
            f"| {item['family']} | {callbacks_text} | {item['rust']} | {item['zephyr']} | {evidence_count} |"
        )

    lines.append(END_MARKER)
    return "\n".join(lines) + "\n"


def replace_generated_section(doc: str, generated: str) -> str:
    pattern = re.compile(
        re.escape(BEGIN_MARKER) + r".*?" + re.escape(END_MARKER) + r"\n?",
        re.DOTALL,
    )
    if not pattern.search(doc):
        if doc.endswith("\n"):
            return doc + "\n" + generated
        return doc + "\n\n" + generated
    return pattern.sub(generated, doc)


class CheckError(Exception):
    pass


def validate(args: argparse.Namespace) -> tuple[list[str], str]:
    manifest_path = Path(args.manifest)
    rust_path = Path(args.rust)
    header_path = Path(args.header)
    runtime_path = Path(args.runtime)
    rust_tests_path = Path(args.rust_tests)
    zephyr_tests_path = Path(args.zephyr_tests)
    coverage_doc_path = Path(args.coverage_doc)

    manifest = load_manifest(manifest_path)
    rust = rust_path.read_text(encoding="utf-8")
    header = header_path.read_text(encoding="utf-8")
    runtime = runtime_path.read_text(encoding="utf-8")
    rust_tests = rust_tests_path.read_text(encoding="utf-8")
    zephyr_tests = zephyr_tests_path.read_text(encoding="utf-8")

    rust_exports = parse_rust_exports(rust)
    header_prototypes = parse_header_prototypes(header)
    rust_callback_fields = extract_rust_callbacks(rust)
    header_callbacks = extract_header_callbacks(header)
    runtime_callbacks = extract_runtime_callbacks(runtime)
    header_types = parse_header_types(header)
    header_constants = parse_header_constants(header)

    export_names = manifest_export_names(manifest)
    rust_export_names = manifest_export_names(manifest, require_rust=True)
    header_only_export_names = manifest_export_names(manifest, require_rust=False)
    callback_items = manifest.get("callbacks", [])
    manifest_callbacks = {item["field"]: item["typedef"] for item in callback_items}

    errors: list[str] = []

    def add_missing(label: str, values: set[str]) -> None:
        if values:
            errors.append(f"{label}: {', '.join(sorted(values))}")

    add_missing("manifest exports missing from Rust", rust_export_names - rust_exports)
    add_missing("manifest exports missing from C header", export_names - header_prototypes)
    add_missing("unlisted Rust exports", rust_exports - rust_export_names)
    add_missing("unlisted C header prototypes", header_prototypes - export_names)
    add_missing(
        "manifest header-only exports unexpectedly present in Rust",
        header_only_export_names & rust_exports,
    )
    add_missing("manifest callbacks missing from Rust", set(manifest_callbacks) - rust_callback_fields)
    add_missing("manifest callbacks missing from C header", set(manifest_callbacks) - set(header_callbacks))
    add_missing(
        "manifest callbacks missing from Zephyr runtime wiring",
        set(manifest_callbacks) - runtime_callbacks,
    )

    mismatched_typedefs = {
        field
        for field, typedef in manifest_callbacks.items()
        if field in header_callbacks and header_callbacks[field] != typedef
    }
    add_missing("manifest callback typedef mismatches", mismatched_typedefs)
    add_missing("unlisted Rust callback fields", rust_callback_fields - set(manifest_callbacks))
    add_missing("unlisted C header callback fields", set(header_callbacks) - set(manifest_callbacks))
    add_missing("unlisted Zephyr runtime callback fields", runtime_callbacks - set(manifest_callbacks))
    add_missing("manifest types missing from C header", set(manifest.get("types", [])) - header_types)
    add_missing(
        "unlisted C header ABI types",
        {name for name in header_types if name.startswith(("Sqvm", "Sqdp", "Sqdc"))}
        - set(manifest.get("types", [])),
    )
    add_missing(
        "manifest constants missing from C header",
        set(manifest.get("constants", [])) - header_constants,
    )
    add_missing(
        "unlisted C header ABI constants",
        header_constants - set(manifest.get("constants", [])),
    )

    missing_coverage = set()
    for item in manifest.get("coverage", []):
        family = item.get("family", "unknown")
        for test_name in item.get("rust_tests", []):
            if test_name not in rust_tests:
                missing_coverage.add(f"{family}.rust:{test_name}")
        for test_name in item.get("zephyr_tests", []):
            if test_name not in zephyr_tests:
                missing_coverage.add(f"{family}.zephyr:{test_name}")
    add_missing("manifest coverage evidence missing", missing_coverage)

    generated = emit_markdown(manifest)
    if coverage_doc_path.exists():
        current_doc = coverage_doc_path.read_text(encoding="utf-8")
        expected_doc = replace_generated_section(current_doc, generated)
        if current_doc != expected_doc:
            errors.append("coverage doc generated section is stale")
    else:
        errors.append(f"coverage doc not found: {rel(coverage_doc_path)}")

    return errors, generated


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        default=ROOT / "compiler/rust/crates/squidvm-ffi/abi/manifest.json",
        type=Path,
        help="path to the ABI manifest JSON",
    )
    parser.add_argument(
        "--rust",
        default=ROOT / "compiler/rust/crates/squidvm-ffi/src/lib.rs",
        type=Path,
        help="path to squidvm-ffi Rust source",
    )
    parser.add_argument(
        "--header",
        default=ROOT / "firmware/zephyr/src/squidvm_ffi.h",
        type=Path,
        help="path to the Zephyr C FFI header",
    )
    parser.add_argument(
        "--runtime",
        default=ROOT / "firmware/zephyr/src/vm_runtime.c",
        type=Path,
        help="path to the Zephyr VM runtime callback wiring",
    )
    parser.add_argument(
        "--rust-tests",
        default=ROOT / "compiler/rust/crates/squidvm-ffi/tests/ffi_dispatch.rs",
        type=Path,
        help="path to Rust FFI dispatch tests used for manifest coverage evidence",
    )
    parser.add_argument(
        "--zephyr-tests",
        default=ROOT / "firmware/zephyr/tests/protocol/src/main.c",
        type=Path,
        help="path to Zephyr protocol ztests used for manifest coverage evidence",
    )
    parser.add_argument(
        "--coverage-doc",
        default=ROOT / "docs/zephyr_vm_host_abi_coverage.md",
        type=Path,
        help="path to the ABI coverage doc",
    )
    parser.add_argument("--check", action="store_true", help="validate files without writing")
    parser.add_argument("--emit-markdown", action="store_true", help="print generated doc section")
    parser.add_argument("--write-doc", action="store_true", help="refresh generated coverage doc section")
    return parser


def main(argv: list[str]) -> int:
    parser = build_arg_parser()
    args = parser.parse_args(argv)

    try:
        errors, generated = validate(args)
    except CheckError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    if args.emit_markdown:
        print(generated, end="")

    if args.write_doc:
        doc_path = Path(args.coverage_doc)
        current_doc = doc_path.read_text(encoding="utf-8") if doc_path.exists() else ""
        doc_path.write_text(replace_generated_section(current_doc, generated), encoding="utf-8")
        errors = [
            error
            for error in errors
            if error
            not in {
                "coverage doc generated section is stale",
                f"coverage doc not found: {rel(doc_path)}",
            }
        ]

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    if not args.emit_markdown:
        print("squidvm FFI ABI manifest check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
