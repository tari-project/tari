#!/usr/bin/env python3
# Copyright 2026 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause
#
# Generates base_layer/transaction_components/src/helpers/terminal_safe_table.rs from the Unicode Character Database.
# Stdlib only. Must be run from the repo root.
#
#   mkdir /tmp/ucd && cd /tmp/ucd
#   curl -O https://www.unicode.org/Public/UCD/latest/ucd/extracted/DerivedGeneralCategory.txt
#   curl -O https://www.unicode.org/Public/UCD/latest/ucd/DerivedCoreProperties.txt
#   cd -
#   python3 -I scripts/gen_terminal_safe_table.py /tmp/ucd/DerivedGeneralCategory.txt \
#       /tmp/ucd/DerivedCoreProperties.txt > base_layer/transaction_components/src/helpers/terminal_safe_table.rs
#
# Code points unassigned (Cn) in the chosen UCD version stay escaped until the table is regenerated against a newer
# version; refresh it when Unicode releases, and update the pinned version in terminal_safe.rs tests.

import re
import sys

GENERAL_CATEGORIES = {"Cf", "Co", "Cn"}
CORE_PROPERTIES = {"Default_Ignorable_Code_Point"}
# Hangul fillers and the Braille blank render as blank space
EXTRA = [(0x115F, 0x115F), (0x1160, 0x1160), (0x3164, 0x3164), (0xFFA0, 0xFFA0), (0x2800, 0x2800)]

LINE = re.compile(r"^([0-9A-F]{4,6})(?:\.\.([0-9A-F]{4,6}))?\s*;\s*([A-Za-z_]+)\s*(?:#|$)")
VERSION = re.compile(r"^# \w+-(\d+\.\d+\.\d+)\.txt")


def read_ranges(path, file_name, wanted):
    version = None
    ranges = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            if version is None:
                m = VERSION.match(line)
                if m:
                    if not line.startswith(f"# {file_name}-"):
                        sys.exit(f"{path}: expected {file_name}")
                    version = m.group(1)
            m = LINE.match(line)
            if m and m.group(3) in wanted:
                start = int(m.group(1), 16)
                end = int(m.group(2), 16) if m.group(2) else start
                ranges.append((start, end))
    if version is None:
        sys.exit(f"{path}: no version header")
    if not ranges:
        sys.exit(f"{path}: no ranges for {sorted(wanted)}")
    return version, ranges


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: gen_terminal_safe_table.py DerivedGeneralCategory.txt DerivedCoreProperties.txt")
    gc_version, gc_ranges = read_ranges(sys.argv[1], "DerivedGeneralCategory", GENERAL_CATEGORIES)
    cp_version, cp_ranges = read_ranges(sys.argv[2], "DerivedCoreProperties", CORE_PROPERTIES)
    if gc_version != cp_version:
        sys.exit(f"UCD version mismatch: {gc_version} vs {cp_version}")

    merged = []
    for start, end in sorted(gc_ranges + cp_ranges + EXTRA):
        if merged and start <= merged[-1][1] + 1:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))

    out = sys.stdout
    out.write("// Copyright 2026 The Tari Project\n")
    out.write("// SPDX-License-Identifier: BSD-3-Clause\n\n")
    out.write(f"// Generated from UCD {gc_version} by scripts/gen_terminal_safe_table.py; do not edit by hand.\n")
    out.write("// Covers General_Category Cf, Co, Cn, Default_Ignorable_Code_Point, Hangul fillers and U+2800.\n")
    out.write("// Regenerate (see the script header for downloading the UCD files):\n")
    out.write("//   python3 -I scripts/gen_terminal_safe_table.py DerivedGeneralCategory.txt DerivedCoreProperties.txt \\\n")
    out.write("//       > base_layer/transaction_components/src/helpers/terminal_safe_table.rs\n\n")
    out.write("#[cfg(test)]\n")
    out.write(f'pub const UCD_VERSION: &str = "{gc_version}";\n\n')
    out.write("/// Sorted, non-overlapping, inclusive code point ranges.\n")
    out.write("pub const ESCAPE_RANGES: &[(u32, u32)] = &[\n")
    for start, end in merged:
        out.write(f"    (0x{start:04X}, 0x{end:04X}),\n")
    out.write("];\n")


if __name__ == "__main__":
    main()
