#!/usr/bin/env python3
# Copyright 2026 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause
#
# Generates base_layer/transaction_components/src/helpers/terminal_safe_table.rs from the Unicode Character Database.
# Stdlib only. Must be run from the repo root.
#
#   mkdir /tmp/ucd && cd /tmp/ucd
#   curl -O https://www.unicode.org/Public/18.0.0/ucd/extracted/DerivedGeneralCategory.txt
#   curl -O https://www.unicode.org/Public/18.0.0/ucd/DerivedCoreProperties.txt
#   cd -
#   python3 -I scripts/gen_terminal_safe_table.py /tmp/ucd/DerivedGeneralCategory.txt \
#       /tmp/ucd/DerivedCoreProperties.txt > base_layer/transaction_components/src/helpers/terminal_safe_table.rs
#
# Code points unassigned (Cn) in the chosen UCD version stay escaped until the table is regenerated against a newer
# version; refresh it when Unicode releases: bump the version in both URLs above and the pinned version in
# terminal_safe.rs tests.

import re
import sys

GENERAL_CATEGORIES = {"Cf", "Co", "Cn", "Zs"}
CORE_PROPERTIES = {"Default_Ignorable_Code_Point"}
# Hangul fillers, the Braille blank, the Khitan filler and some font-dependent blank glyphs (null notehead, Tifinagh
# consonant joiner, ideographic half fill space) render as blank space
EXTRA = [
    (0x115F, 0x115F),
    (0x1160, 0x1160),
    (0x3164, 0x3164),
    (0xFFA0, 0xFFA0),
    (0x2800, 0x2800),
    (0x16FE4, 0x16FE4),
    (0x1D159, 0x1D159),
    (0x2D7F, 0x2D7F),
    (0x303F, 0x303F),
]
# Plain space (Zs) stays unescaped
KEEP = 0x20
# One code point from each class; generation fails if any is missing
KNOWN_MEMBERS = [
    0x00AD, 0x061C, 0x200D, 0xFE0F, 0xE0001, 0xE000, 0x0378, 0xFFFE, 0x115F, 0x2800, 0x3000, 0x16FE4,
    0x1D159, 0x2D7F, 0x303F,
]

LINE = re.compile(r"^([0-9A-F]{4,6})(?:\.\.([0-9A-F]{4,6}))?\s*;\s*([A-Za-z_]+)\s*(?:;[^#]*)?(?:#|$)")
DATA_LINE = re.compile(r"^[0-9A-Fa-f]")
VERSION = re.compile(r"^# \w+-(\d+\.\d+\.\d+)\.txt")


def read_ranges(path, file_name, wanted):
    """Returns (version, wanted ranges, code points listed under any property), after checking the file is whole."""
    version = None
    ranges = []
    listed = 0
    complete = False
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.rstrip() == "# EOF":
                complete = True
            if version is None:
                m = VERSION.match(line)
                if m:
                    if not line.startswith(f"# {file_name}-"):
                        sys.exit(f"{path}: expected {file_name}")
                    version = m.group(1)
            m = LINE.match(line)
            if m is None:
                if DATA_LINE.match(line):
                    sys.exit(f"{path}: unrecognised data line: {line.rstrip()}")
                continue
            start = int(m.group(1), 16)
            end = int(m.group(2), 16) if m.group(2) else start
            if start > end or end > 0x10FFFF:
                sys.exit(f"{path}: bad range: {line.rstrip()}")
            listed += end - start + 1
            if m.group(3) not in wanted:
                continue
            if start <= KEEP <= end:
                if start < KEEP:
                    ranges.append((start, KEEP - 1))
                if KEEP < end:
                    ranges.append((KEEP + 1, end))
            else:
                ranges.append((start, end))
    if version is None:
        sys.exit(f"{path}: no version header")
    if not complete:
        sys.exit(f"{path}: no '# EOF' marker; the file is truncated")
    if not ranges:
        sys.exit(f"{path}: no ranges for {sorted(wanted)}")
    return version, ranges, listed


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: gen_terminal_safe_table.py DerivedGeneralCategory.txt DerivedCoreProperties.txt")
    gc_version, gc_ranges, gc_listed = read_ranges(sys.argv[1], "DerivedGeneralCategory", GENERAL_CATEGORIES)
    cp_version, cp_ranges, _ = read_ranges(sys.argv[2], "DerivedCoreProperties", CORE_PROPERTIES)
    # The general category file assigns every code point exactly once, so anything else means lines are missing
    if gc_listed != 0x110000:
        sys.exit(f"{sys.argv[1]}: lists {gc_listed:#x} code points, expected 0x110000; the file is incomplete")
    if gc_version != cp_version:
        sys.exit(f"UCD version mismatch: {gc_version} vs {cp_version}")

    merged = []
    for start, end in sorted(gc_ranges + cp_ranges + EXTRA):
        if merged and start <= merged[-1][1] + 1:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))
    for cp in KNOWN_MEMBERS:
        if not any(start <= cp <= end for start, end in merged):
            sys.exit(f"known member U+{cp:04X} missing from the table")

    out = sys.stdout
    out.write("// Copyright 2026 The Tari Project\n")
    out.write("// SPDX-License-Identifier: BSD-3-Clause\n\n")
    out.write(f"// Generated from UCD {gc_version} by scripts/gen_terminal_safe_table.py; do not edit by hand.\n")
    out.write("// Covers General_Category Cf, Co, Cn, Zs (except U+0020), Default_Ignorable_Code_Point,\n")
    out.write("// Hangul fillers, U+2800, U+16FE4, U+1D159, U+2D7F and U+303F.\n")
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
