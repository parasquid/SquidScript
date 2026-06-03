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


def manifest_type_names(manifest: dict) -> set[str]:
    return {
        item["name"] if isinstance(item, dict) else item
        for item in manifest.get("types", [])
    }


def manifest_constant_names(manifest: dict) -> set[str]:
    return {
        item["name"] if isinstance(item, dict) else item
        for item in manifest.get("constants", [])
    }


def callback_items(manifest: dict) -> list[dict]:
    items = manifest.get("callbacks", [])
    valid_policies = {
        "noop",
        "required_vm_error",
        "read_failed",
        "unsupported_result",
        "idle_result",
    }
    errors = []
    for item in items:
        field = item.get("field", "<unknown>")
        for required in ("rust_type", "missing_policy"):
            if not item.get(required):
                errors.append(f"manifest callback {field} is missing {required}")
        policy = item.get("missing_policy")
        if policy and policy not in valid_policies:
            errors.append(f"manifest callback {field} has invalid missing_policy {policy}")
    if errors:
        raise CheckError("; ".join(errors))
    return items


def result_default_items(manifest: dict) -> list[dict]:
    items = manifest.get("result_defaults", [])
    errors = []
    for item in items:
        type_name = item.get("type", "<unknown>")
        if not item.get("type"):
            errors.append("manifest result_default is missing type")
        if not item.get("rust_fields"):
            errors.append(f"manifest result_default {type_name} is missing rust_fields")
        for helper in item.get("c_helpers", []):
            if not helper.get("name"):
                errors.append(f"manifest result_default {type_name} has helper without name")
            if not helper.get("fields"):
                errors.append(
                    f"manifest result_default {type_name} helper {helper.get('name', '<unknown>')} is missing fields"
                )
        for fields in [item.get("rust_fields", [])] + [
            helper.get("fields", []) for helper in item.get("c_helpers", [])
        ]:
            for field in fields:
                if not field.get("field"):
                    errors.append(f"manifest result_default {type_name} has field without name")
    if errors:
        raise CheckError("; ".join(errors))
    return items


def c_definition(item: object, *, label: str) -> str:
    if isinstance(item, dict):
        definition = item.get("definition")
        if definition:
            return str(definition).strip()
        name = item.get("name", "<unknown>")
    else:
        name = item
    raise CheckError(f"manifest {label} {name} is missing a C definition")


def export_prototype(item: dict) -> str:
    prototype = item.get("prototype")
    if not prototype:
        raise CheckError(f"manifest export {item.get('name', '<unknown>')} is missing a C prototype")
    return str(prototype).strip()


def c_result_field_lines(target: str, field: dict) -> list[str]:
    name = field["field"]
    access = f"{target}->{name}"
    len_access = f"{target}->{name}_len"
    if "literal" in field:
        value = str(field["literal"])
        return [
            f'\t{access} = (const uint8_t *)"{value}";',
            f'\t{len_access} = sizeof("{value}") - 1;',
        ]
    if field.get("null"):
        lines = [f"\t{access} = NULL;"]
        if field.get("len", True):
            lines.append(f"\t{len_access} = 0;")
        return lines
    if "value" in field:
        return [f"\t{access} = {field['value']};"]
    raise CheckError(f"manifest result_default field {name} has no value, literal, or null")


def emit_c_result_helpers(manifest: dict) -> str:
    lines = []
    for item in result_default_items(manifest):
        type_name = item["type"]
        for helper in item.get("c_helpers", []):
            lines.append(f"static inline void {helper['name']}({type_name} *out)")
            lines.append("{")
            lines.append("\tif (out == NULL) {")
            lines.append("\t\treturn;")
            lines.append("\t}")
            for field in helper["fields"]:
                lines.extend(c_result_field_lines("out", field))
            lines.append("}")
            lines.append("")
    return "\n".join(lines)


def emit_header(manifest: dict) -> str:
    header = manifest.get("header", {})
    guard = header.get("guard", "SQUIDSCRIPT_SQUIDVM_FFI_H")
    includes = header.get("includes", ["stddef.h", "stdbool.h", "stdint.h"])

    lines = [
        "/* Generated by scripts/check-squidvm-ffi-abi.py --write-header. Do not edit. */",
        f"#ifndef {guard}",
        f"#define {guard}",
        "",
    ]
    for include in includes:
        lines.append(f"#include <{include}>")
    lines.extend(
        [
            "",
            "#ifdef __cplusplus",
            'extern "C" {',
            "#endif",
            "",
        ]
    )

    for item in manifest.get("constants", []):
        lines.append(c_definition(item, label="constant"))
    if manifest.get("constants"):
        lines.append("")

    for item in manifest.get("types", []):
        lines.append(c_definition(item, label="type"))
        lines.append("")

    c_result_helpers = emit_c_result_helpers(manifest)
    if c_result_helpers:
        lines.append(c_result_helpers)

    for item in manifest.get("exports", []):
        lines.append(export_prototype(item))

    lines.extend(
        [
            "",
            "#ifdef __cplusplus",
            "}",
            "#endif",
            "",
            "#endif",
            "",
        ]
    )
    return "\n".join(lines)


def rust_result_field_value(field: dict) -> str:
    if "literal" in field:
        return f'b"{field["literal"]}".as_ptr()'
    if field.get("null"):
        return "ptr::null()"
    if "value" in field:
        return str(field["value"])
    raise CheckError(
        f"manifest result_default field {field.get('field', '<unknown>')} has no value, literal, or null"
    )


def rust_result_field_lines(field: dict) -> list[str]:
    name = field["field"]
    if "literal" in field:
        value = str(field["literal"])
        return [
            f"            {name}: {rust_result_field_value(field)},",
            f'            {name}_len: b"{value}".len(),',
        ]
    if field.get("null"):
        lines = [f"            {name}: ptr::null(),"]
        if field.get("len", True):
            lines.append(f"            {name}_len: 0,")
        return lines
    return [f"            {name}: {rust_result_field_value(field)},"]


def emit_rust_result_defaults(manifest: dict) -> str:
    lines = [
        "// Generated by scripts/check-squidvm-ffi-abi.py --write-generated. Do not edit.",
        "",
        "use super::*;",
        "",
    ]
    for item in result_default_items(manifest):
        lines.append(f"impl Default for {item['type']} {{")
        lines.append("    fn default() -> Self {")
        lines.append("        Self {")
        for field in item["rust_fields"]:
            lines.extend(rust_result_field_lines(field))
        lines.extend(["        }", "    }", "}", ""])
    return "\n".join(lines)


def emit_markdown(manifest: dict) -> str:
    exports = manifest.get("exports", [])
    callbacks = callback_items(manifest)
    coverage = manifest.get("coverage", [])

    lines = [
        BEGIN_MARKER,
        "## Manifest-Checked ABI Inventory",
        "",
        "This section is generated from `compiler/rust/crates/squidvm-ffi/abi/manifest.json`.",
        "Run `python3 scripts/check-squidvm-ffi-abi.py --write-header --write-doc --write-generated` after changing the FFI ABI.",
        "The checker validates Rust exports, the generated Zephyr C header, generated",
        "Rust callback/test artifacts, generated result-default helpers, runtime",
        "callback wiring, and this generated documentation section against the",
        "manifest.",
        "",
        f"- Exports: {len(exports)}",
        f"- Callback fields: {len(callbacks)}",
        f"- Generated result-default records: {len(result_default_items(manifest))}",
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


def emit_rust_callbacks(manifest: dict) -> str:
    lines = [
        "// Generated by scripts/check-squidvm-ffi-abi.py --write-generated. Do not edit.",
        "",
        "use super::*;",
        "",
        "#[repr(C)]",
        "#[derive(Clone, Copy)]",
        "pub struct SqvmCallbacks {",
    ]
    for item in callback_items(manifest):
        lines.append(f"    pub {item['field']}: {item['rust_type']},")
    lines.extend(
        [
            "}",
            "",
            "impl Default for SqvmCallbacks {",
            "    fn default() -> Self {",
            "        Self {",
        ]
    )
    for item in callback_items(manifest):
        lines.append(f"            {item['field']}: None,")
    lines.extend(["        }", "    }", "}", ""])
    return "\n".join(lines)


def emit_runtime_callbacks(manifest: dict) -> str:
    lines = [
        "/* Generated by scripts/check-squidvm-ffi-abi.py --write-generated. Do not edit. */",
    ]
    for item in callback_items(manifest):
        symbol = item.get("zephyr_symbol") or f"runtime_{item['field']}"
        lines.append(f"\t.{item['field']} = {symbol},")
    lines.append("")
    return "\n".join(lines)


def callback_label(field: str) -> str:
    return field.replace("_", " ")


def emit_rust_dispatch_cases(manifest: dict) -> str:
    missing_noop = []
    missing_required = []
    callback_errors = []
    all_policies = []
    for item in callback_items(manifest):
        field = item["field"]
        policy = item["missing_policy"]
        fixture = item.get("test_fixture")
        failing = item.get("failing_callback")
        all_policies.append((field, policy))
        if fixture and policy == "noop":
            missing_noop.append((field, fixture))
        if fixture and policy in {"required_vm_error", "read_failed"}:
            missing_required.append((field, fixture))
        if fixture and failing:
            callback_errors.append((field, fixture, failing))

    lines = [
        "// Generated by scripts/check-squidvm-ffi-abi.py --write-generated. Do not edit.",
        "",
        "use super::*;",
        "",
        "pub(super) type CallbackCase = (&'static str, Vec<u8>, fn(&mut SqvmCallbacks));",
        "",
        "pub(super) fn callback_error_cases() -> Vec<CallbackCase> {",
        "    vec![",
    ]
    for field, fixture, failing in callback_errors:
        lines.append(
            f'        ("{callback_label(field)}", {fixture}(), |callbacks| callbacks.{field} = Some({failing})),'
        )
    lines.extend(["    ]", "}", ""])

    lines.extend(
        [
            "pub(super) fn missing_noop_cases() -> Vec<CallbackCase> {",
            "    vec![",
        ]
    )
    for field, fixture in missing_noop:
        lines.append(f'        ("{callback_label(field)}", {fixture}(), |callbacks| callbacks.{field} = None),')
    lines.extend(["    ]", "}", ""])

    lines.extend(
        [
            "pub(super) fn missing_required_cases() -> Vec<CallbackCase> {",
            "    vec![",
        ]
    )
    for field, fixture in missing_required:
        lines.append(f'        ("{callback_label(field)}", {fixture}(), |callbacks| callbacks.{field} = None),')
    lines.extend(["    ]", "}", ""])

    lines.extend(
        [
            "pub(super) fn callback_policy_cases() -> &'static [(&'static str, &'static str)] {",
            "    &[",
        ]
    )
    for field, policy in all_policies:
        lines.append(f'        ("{field}", "{policy}"),')
    lines.extend(["    ]", "}", ""])
    return "\n".join(lines)


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


def validate(args: argparse.Namespace) -> tuple[list[str], str, str, str, str, str, str]:
    manifest_path = Path(args.manifest)
    rust_path = Path(args.rust)
    header_path = Path(args.header)
    runtime_path = Path(args.runtime)
    rust_tests_path = Path(args.rust_tests)
    zephyr_tests_path = Path(args.zephyr_tests)
    coverage_doc_path = Path(args.coverage_doc)
    generated_rust_callbacks_path = Path(args.generated_rust_callbacks)
    generated_rust_result_defaults_path = Path(args.generated_rust_result_defaults)
    generated_rust_dispatch_cases_path = Path(args.generated_rust_dispatch_cases)
    generated_runtime_callbacks_path = Path(args.generated_runtime_callbacks)

    manifest = load_manifest(manifest_path)
    generated_header = emit_header(manifest)
    generated_rust_callbacks = emit_rust_callbacks(manifest)
    generated_rust_result_defaults = emit_rust_result_defaults(manifest)
    generated_rust_dispatch_cases = emit_rust_dispatch_cases(manifest)
    generated_runtime_callbacks = emit_runtime_callbacks(manifest)
    rust = rust_path.read_text(encoding="utf-8")
    if generated_rust_callbacks_path.exists():
        rust = rust + "\n" + generated_rust_callbacks_path.read_text(encoding="utf-8")
    elif args.write_generated:
        rust = rust + "\n" + generated_rust_callbacks
    if generated_rust_result_defaults_path.exists():
        rust = rust + "\n" + generated_rust_result_defaults_path.read_text(encoding="utf-8")
    elif args.write_generated:
        rust = rust + "\n" + generated_rust_result_defaults
    if header_path.exists():
        header = header_path.read_text(encoding="utf-8")
    elif args.write_header or args.emit_header:
        header = generated_header
    else:
        header = ""
    runtime = runtime_path.read_text(encoding="utf-8")
    rust_tests = rust_tests_path.read_text(encoding="utf-8")
    zephyr_tests = zephyr_tests_path.read_text(encoding="utf-8")

    rust_exports = parse_rust_exports(rust)
    header_prototypes = parse_header_prototypes(header)
    rust_callback_fields = extract_rust_callbacks(rust)
    header_callbacks = extract_header_callbacks(header)
    runtime_callbacks = extract_runtime_callbacks(runtime)
    if generated_runtime_callbacks_path.exists():
        runtime_callbacks |= set(
            re.findall(
                r"\.([A-Za-z_][A-Za-z0-9_]*)\s*=",
                generated_runtime_callbacks_path.read_text(encoding="utf-8"),
            )
        )
    elif args.write_generated:
        runtime_callbacks |= set(
            re.findall(r"\.([A-Za-z_][A-Za-z0-9_]*)\s*=", generated_runtime_callbacks)
        )
    header_types = parse_header_types(header)
    header_constants = parse_header_constants(header)

    export_names = manifest_export_names(manifest)
    rust_export_names = manifest_export_names(manifest, require_rust=True)
    header_only_export_names = manifest_export_names(manifest, require_rust=False)
    manifest_callbacks = {item["field"]: item["typedef"] for item in callback_items(manifest)}

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
    manifest_types = manifest_type_names(manifest)
    add_missing("manifest types missing from C header", manifest_types - header_types)
    add_missing(
        "unlisted C header ABI types",
        {name for name in header_types if name.startswith(("Sqvm", "Sqdp", "Sqdc"))}
        - manifest_types,
    )
    manifest_constants = manifest_constant_names(manifest)
    add_missing(
        "manifest constants missing from C header",
        manifest_constants - header_constants,
    )
    add_missing(
        "unlisted C header ABI constants",
        header_constants - manifest_constants,
    )
    if header_path.exists() and header != generated_header:
        errors.append("generated C header is stale")
    elif not header_path.exists() and not (args.write_header or args.emit_header):
        errors.append(f"C header not found: {rel(header_path)}")

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

    generated_files = [
        (
            generated_rust_callbacks_path,
            generated_rust_callbacks,
            "generated Rust callback module is stale",
            "generated Rust callback module not found",
        ),
        (
            generated_rust_dispatch_cases_path,
            generated_rust_dispatch_cases,
            "generated Rust dispatch cases are stale",
            "generated Rust dispatch cases not found",
        ),
        (
            generated_rust_result_defaults_path,
            generated_rust_result_defaults,
            "generated Rust result defaults module is stale",
            "generated Rust result defaults module not found",
        ),
        (
            generated_runtime_callbacks_path,
            generated_runtime_callbacks,
            "generated Zephyr runtime callback initializer is stale",
            "generated Zephyr runtime callback initializer not found",
        ),
    ]
    for path, expected, stale_error, missing_label in generated_files:
        if path.exists():
            if path.read_text(encoding="utf-8") != expected:
                errors.append(stale_error)
        elif not args.write_generated:
            errors.append(f"{missing_label}: {rel(path)}")

    return (
        errors,
        generated,
        generated_header,
        generated_rust_callbacks,
        generated_rust_result_defaults,
        generated_rust_dispatch_cases,
        generated_runtime_callbacks,
    )


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
    parser.add_argument(
        "--generated-rust-callbacks",
        default=ROOT / "compiler/rust/crates/squidvm-ffi/src/generated_callbacks.rs",
        type=Path,
        help="path to the generated Rust callback module",
    )
    parser.add_argument(
        "--generated-rust-result-defaults",
        default=ROOT / "compiler/rust/crates/squidvm-ffi/src/generated_result_defaults.rs",
        type=Path,
        help="path to the generated Rust result defaults module",
    )
    parser.add_argument(
        "--generated-rust-dispatch-cases",
        default=ROOT
        / "compiler/rust/crates/squidvm-ffi/tests/support/generated_ffi_dispatch_cases.rs",
        type=Path,
        help="path to the generated Rust FFI dispatch case module",
    )
    parser.add_argument(
        "--generated-runtime-callbacks",
        default=ROOT / "firmware/zephyr/src/generated_runtime_callbacks.inc",
        type=Path,
        help="path to the generated Zephyr runtime callback initializer",
    )
    parser.add_argument("--check", action="store_true", help="validate files without writing")
    parser.add_argument("--emit-markdown", action="store_true", help="print generated doc section")
    parser.add_argument("--emit-header", action="store_true", help="print generated C header")
    parser.add_argument("--write-doc", action="store_true", help="refresh generated coverage doc section")
    parser.add_argument("--write-header", action="store_true", help="refresh generated C header")
    parser.add_argument("--write-generated", action="store_true", help="refresh generated callback artifacts")
    return parser


def main(argv: list[str]) -> int:
    parser = build_arg_parser()
    args = parser.parse_args(argv)

    if args.emit_header and not (args.check or args.write_doc or args.write_header or args.emit_markdown):
        try:
            print(emit_header(load_manifest(Path(args.manifest))), end="")
        except CheckError as error:
            print(f"error: {error}", file=sys.stderr)
            return 1
        return 0

    try:
        (
            errors,
            generated,
            generated_header,
            generated_rust_callbacks,
            generated_rust_result_defaults,
            generated_rust_dispatch_cases,
            generated_runtime_callbacks,
        ) = validate(args)
    except CheckError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    if args.emit_markdown:
        print(generated, end="")
    if args.emit_header:
        print(generated_header, end="")

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
    if args.write_header:
        header_path = Path(args.header)
        header_path.write_text(generated_header, encoding="utf-8")
        errors = [
            error
            for error in errors
            if error
            not in {
                "generated C header is stale",
                f"C header not found: {rel(header_path)}",
            }
        ]
    if args.write_generated:
        generated_paths = [
            (
                Path(args.generated_rust_callbacks),
                generated_rust_callbacks,
                "generated Rust callback module is stale",
                "generated Rust callback module not found",
            ),
            (
                Path(args.generated_rust_result_defaults),
                generated_rust_result_defaults,
                "generated Rust result defaults module is stale",
                "generated Rust result defaults module not found",
            ),
            (
                Path(args.generated_rust_dispatch_cases),
                generated_rust_dispatch_cases,
                "generated Rust dispatch cases are stale",
                "generated Rust dispatch cases not found",
            ),
            (
                Path(args.generated_runtime_callbacks),
                generated_runtime_callbacks,
                "generated Zephyr runtime callback initializer is stale",
                "generated Zephyr runtime callback initializer not found",
            ),
        ]
        for path, contents, stale_error, missing_label in generated_paths:
            path.write_text(contents, encoding="utf-8")
            errors = [
                error
                for error in errors
                if error not in {stale_error, f"{missing_label}: {rel(path)}"}
            ]

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    if not (args.emit_markdown or args.emit_header):
        print("squidvm FFI ABI manifest check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
