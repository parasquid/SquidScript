#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${1:-${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/c3-supermini}}"
LIMIT="${2:-30}"

if [[ ! -d "$BUILD_DIR" ]]; then
  printf 'Build directory not found: %s\n' "$BUILD_DIR" >&2
  exit 1
fi

mapfile -t STACK_FILES < <(find "$BUILD_DIR" -name '*.su' -type f | sort)
if [[ "${#STACK_FILES[@]}" -eq 0 ]]; then
  printf 'No .su stack-usage files found under %s\n' "$BUILD_DIR" >&2
  printf 'Build with: SQUID_ZEPHYR_STACK_USAGE=1 scripts/c3-supermini-build.sh\n' >&2
  exit 1
fi

printf 'note: .su rows are per-function static estimates, not cumulative call-chain peaks. Cumulative paths include only direct source-known calls between functions with .su rows.\n' >&2
python3 - "$LIMIT" "${STACK_FILES[@]}" <<'PY'
import re
import sys
from collections import defaultdict
from pathlib import Path

limit = int(sys.argv[1])
stack_files = [Path(path) for path in sys.argv[2:]]
entries = []
su_pattern = re.compile(r"^(.*):([0-9]+):([0-9]+):([^\t]+)\t([0-9]+)\t(.+)$")

for stack_file in stack_files:
    with stack_file.open(encoding="utf-8") as handle:
        for line in handle:
            match = su_pattern.match(line.rstrip("\n"))
            if match is None:
                continue
            source, line_no, column, function, byte_count, mode = match.groups()
            entries.append(
                {
                    "source": source,
                    "line": int(line_no),
                    "column": int(column),
                    "function": function,
                    "base_function": function.split(".", 1)[0],
                    "bytes": int(byte_count),
                    "mode": mode,
                    "location": f"{source}:{line_no}:{column}",
                }
            )

entries.sort(key=lambda entry: (-entry["bytes"], entry["function"], entry["location"]))
top_entries = entries[:limit]

print("bytes\tfunction\tlocation\tmode")
for entry in top_entries:
    print(f"{entry['bytes']}\t{entry['function']}\t{entry['location']}\t{entry['mode']}")

print()
print("top_rows\tmax_bytes\tsum_bytes\tsource_file")
by_file = defaultdict(lambda: {"count": 0, "sum": 0, "max": 0})
for entry in top_entries:
    bucket = by_file[entry["source"]]
    bucket["count"] += 1
    bucket["sum"] += entry["bytes"]
    bucket["max"] = max(bucket["max"], entry["bytes"])
for source, bucket in sorted(
    by_file.items(), key=lambda item: (-item[1]["sum"], -item[1]["max"], item[0])
):
    print(f"{bucket['count']}\t{bucket['max']}\t{bucket['sum']}\t{source}")

entries_by_base = {}
for index, entry in enumerate(entries):
    entries_by_base.setdefault(entry["base_function"], index)

source_cache = {}
comment_pattern = re.compile(r"/\*.*?\*/|//[^\n]*", re.DOTALL)
call_pattern = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")
ignored_calls = {
    "BUILD_ASSERT",
    "IS_ENABLED",
    "MAX",
    "MIN",
    "NULL",
    "if",
    "for",
    "while",
    "switch",
    "return",
    "sizeof",
}


def function_body(entry):
    source = Path(entry["source"])
    if not source.is_file():
        return ""
    if source not in source_cache:
        source_cache[source] = source.read_text(encoding="utf-8", errors="ignore").splitlines()
    lines = source_cache[source]
    index = max(entry["line"] - 1, 0)
    brace_depth = 0
    body = []
    found_open = False
    for line in lines[index:]:
        if not found_open:
            open_at = line.find("{")
            if open_at < 0:
                continue
            found_open = True
            line = line[open_at + 1 :]
            brace_depth = 1
        brace_depth += line.count("{")
        brace_depth -= line.count("}")
        body.append(line)
        if brace_depth <= 0:
            break
    return "\n".join(body)


direct_callees = defaultdict(list)
for index, entry in enumerate(entries):
    body = comment_pattern.sub("", function_body(entry))
    seen = set()
    for call in call_pattern.findall(body):
        if call in ignored_calls or call == entry["base_function"] or call in seen:
            continue
        callee = entries_by_base.get(call)
        if callee is None:
            continue
        direct_callees[index].append(callee)
        seen.add(call)


def cumulative(index, visiting=None):
    if visiting is None:
        visiting = set()
    if index in visiting:
        entry = entries[index]
        return entry["bytes"], [entry["function"]]
    visiting.add(index)
    entry = entries[index]
    best_child_total = 0
    best_child_path = []
    for callee in direct_callees[index]:
        total, path = cumulative(callee, visiting)
        if total > best_child_total:
            best_child_total = total
            best_child_path = path
    visiting.remove(index)
    return entry["bytes"] + best_child_total, [entry["function"], *best_child_path]


cumulative_rows = []
for index, entry in enumerate(entries):
    total, path = cumulative(index)
    cumulative_rows.append((total, entry["bytes"], " -> ".join(path), entry))
cumulative_rows.sort(key=lambda row: (-row[0], -row[1], row[3]["function"], row[3]["location"]))

print()
print("cumulative_bytes\tself_bytes\tcallee_path\tfunction\tlocation")
for total, self_bytes, path, entry in cumulative_rows[:limit]:
    print(f"{total}\t{self_bytes}\t{path}\t{entry['function']}\t{entry['location']}")
PY
