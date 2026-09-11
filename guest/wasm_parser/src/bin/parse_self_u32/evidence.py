#!/usr/bin/env python3
"""Derive the fixed fixture's evidence using WABT, independently of the guest parser."""

from collections import Counter
from pathlib import Path
import re
import struct
import subprocess


def objdump(*arguments: str) -> str:
    return subprocess.check_output(["wasm-objdump", *arguments], text=True)


def section_counts(header: str) -> dict[str, int]:
    counts = {"section_count": 0, "custom_section_count": 0}
    for line in header.splitlines():
        if " start=" not in line:
            continue

        section = line.split()[0]
        if section not in {
            "Type", "Import", "Function", "Table", "Memory", "Global", "Export",
            "Start", "Elem", "DataCount", "Code", "Data", "Custom",
        }:
            raise ValueError(f"unsupported WABT section: {section}")

        counts["section_count"] += 1
        if section == "Custom":
            counts["custom_section_count"] += 1
        elif section != "Start":
            count = re.search(r"\bcount: (\d+)", line)
            if count is None:
                raise ValueError(f"missing WABT section count: {line}")
            counts[section] = int(count[1])

    return counts


def imported_functions(details: str) -> int:
    imports = False
    count = 0
    for line in details.splitlines():
        if line.startswith("Import["):
            imports = True
        elif imports and line.startswith(" - "):
            count += line.startswith(" - func[")
        elif imports:
            break
    return count


def operator_class(mnemonic: str) -> str:
    if mnemonic in {
        "unreachable", "nop", "block", "loop", "if", "else", "end",
        "br", "br_if", "br_table", "return",
    }:
        return "control"
    if mnemonic.startswith("call"):
        return "call"
    if mnemonic in {"drop", "select"}:
        return "parametric"
    if mnemonic.startswith(("local.", "global.")):
        return "variable"
    if (mnemonic.startswith("memory.") or mnemonic == "data.drop"
            or ".load" in mnemonic or ".store" in mnemonic):
        return "memory"
    if mnemonic.startswith(("table.", "ref.")) or mnemonic == "elem.drop":
        return "reference_table"
    if mnemonic.startswith(("i32.", "i64.", "f32.", "f64.")):
        return "numeric"
    raise ValueError(f"unsupported WABT operator: {mnemonic}")


def operator_counts(disassembly: str) -> Counter:
    counts = Counter()
    for line in disassembly.splitlines():
        operation = re.match(r"^\s*[0-9a-f]+:.*\|\s*(\S+)", line)
        if operation is None:
            continue

        mnemonic = operation[1]
        # WABT also prints local-variable declarations here; they are not instructions.
        if not mnemonic.startswith("local["):
            counts[operator_class(mnemonic)] += 1
    return counts


def fnv1a32(data: bytes) -> int:
    value = 0x811C9DC5  # FNV-1a 32-bit offset basis.
    for byte in data:
        value = ((value ^ byte) * 0x01000193) & 0xFFFFFFFF
    return value


def main() -> None:
    # Establish the independent decoder and inspect the retained input without rebuilding it.
    version = objdump("--version").strip()
    if version != "1.0.39":
        raise ValueError(f"expected reviewed WABT 1.0.39, found {version}")

    module = Path(__file__).resolve().with_name("input.wasm")
    sections = section_counts(objdump("-h", str(module)))
    imports = imported_functions(objdump("-x", str(module)))
    operators = operator_counts(objdump("-d", str(module)))

    # Declaration order is the fixture's 22-word result format, not WABT's section order.
    facts = {
        "input_length": module.stat().st_size,
        "section_count": sections["section_count"],
        "custom_section_count": sections["custom_section_count"],
        "type_count": sections.get("Type", 0),
        "imported_function_count": imports,
        "defined_function_count": sections.get("Function", 0),
        "table_count": sections.get("Table", 0),
        "memory_count": sections.get("Memory", 0),
        "global_count": sections.get("Global", 0),
        "export_count": sections.get("Export", 0),
        "element_count": sections.get("Elem", 0),
        "data_segment_count": sections.get("Data", 0),
        "code_body_count": sections.get("Code", 0),
        "operator_count": sum(operators.values()),
    }
    for category in ("control", "call", "parametric", "variable", "memory", "numeric", "reference_table"):
        facts[f"{category}_operator_count"] = operators[category]

    # The final word hashes the preceding 21 little-endian words, not the WASM file.
    fact_bytes = struct.pack("<21I", *facts.values())
    facts["facts_fnv1a32"] = fnv1a32(fact_bytes)
    expected = fact_bytes + struct.pack("<I", facts["facts_fnv1a32"])

    for name, value in facts.items():
        print(f"{name}={value}")
    print(f"expected_hex={expected.hex()}")


if __name__ == "__main__":
    main()
