#!/usr/bin/env python3
#
# Decoder parity check. Must be run from the repo root.
#
# A type with a fallible `pub fn from_bytes(..) -> Result<..>` has an invariant that its byte decoder enforces (the
# protobuf conversions call `from_bytes`). If the same type derives `Deserialize` or `BorshDeserialize`, the derived
# decoder skips that check and accepts values `from_bytes` rejects, which is how `EncryptedData` once let a node accept
# a transaction every peer refused. Such a type must route its serde and borsh decoders through its validation instead
# (see `tari_max_size::ValidatedDecode`).
#
# This script fails when a type both derives one of those decoders and has an inherent fallible `pub fn from_bytes`
# in the same crate. Types that are known to be fine can be listed in `ALLOWED`, with a reason.

import os
import re
import sys

SCAN_DIRS = ["applications", "base_layer", "common", "common_sqlite", "comms", "hashing", "infrastructure"]
SKIP_DIRS = {"target", "node_modules", ".git"}

# "crate dir:type name" -> reason
ALLOWED = {
    "comms/core:IdentitySignature": "`from_bytes` only decodes the protobuf form; it has no invariant beyond the field "
    "decoders, which the serde form (the peer database) shares",
}

DERIVE_RE = re.compile(r"#\[derive\(([^\]]*)\)\]")
TYPE_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)")
IMPL_RE = re.compile(r"^\s*impl\b(?:\s*<[^{]*?>)?\s+([\w:]+)(?:<[^{]*>)?\s*(?:where\b[^{]*)?\{")
FROM_BYTES_RE = re.compile(r"\bpub\s+fn\s+from_bytes\b")


def strip_line(line):
    """Removes string literals and line comments, so that braces in them are not counted."""
    line = re.sub(r'"(?:\\.|[^"\\])*"', '""', line)
    line = re.sub(r"'(?:\\.|[^'\\])'", "''", line)
    return line.split("//", 1)[0]


def crate_dir(path):
    d = os.path.dirname(path)
    while d and d != ".":
        if os.path.isfile(os.path.join(d, "Cargo.toml")):
            return d
        d = os.path.dirname(d)
    return "."


def scan_file(path, derived, from_bytes):
    with open(path, encoding="utf-8", errors="replace") as f:
        lines = [strip_line(line) for line in f]
    krate = crate_dir(path)

    # Derives: collect the attributes stacked above each struct / enum.
    pending_derives = []
    attr_buffer = ""
    for number, line in enumerate(lines, 1):
        stripped = line.strip()
        if attr_buffer or stripped.startswith("#["):
            attr_buffer += " " + stripped
            if attr_buffer.count("[") > attr_buffer.count("]"):
                continue
            for derive in DERIVE_RE.findall(attr_buffer):
                pending_derives.extend(name.strip().split("::")[-1] for name in derive.split(","))
            attr_buffer = ""
            continue
        type_match = TYPE_RE.match(line)
        if type_match:
            decoders = sorted({d for d in pending_derives if d in ("Deserialize", "BorshDeserialize")})
            if decoders:
                derived.setdefault((krate, type_match.group(1)), []).append((path, number, decoders))
        if stripped:
            pending_derives = []

    # Inherent impls with a fallible `pub fn from_bytes`.
    depth = 0
    impl_stack = []
    for number, line in enumerate(lines, 1):
        impl_match = IMPL_RE.match(line)
        if impl_match and " for " not in line.split("{", 1)[0]:
            impl_stack.append((impl_match.group(1).split("::")[-1], depth))
        if FROM_BYTES_RE.search(line) and impl_stack:
            signature = ""
            for sig_line in lines[number - 1 : number + 10]:
                signature += sig_line
                if "{" in sig_line or ";" in sig_line:
                    break
            signature = signature.split("{", 1)[0]
            returns = signature.split("->", 1)[1] if "->" in signature else ""
            if "Result" in returns:
                from_bytes.setdefault((krate, impl_stack[-1][0]), []).append((path, number))
        depth += line.count("{") - line.count("}")
        while impl_stack and depth <= impl_stack[-1][1]:
            impl_stack.pop()


def main():
    derived = {}
    from_bytes = {}
    for top in SCAN_DIRS:
        for root, dirs, files in os.walk(top):
            dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
            for name in sorted(files):
                if name.endswith(".rs"):
                    scan_file(os.path.join(root, name), derived, from_bytes)

    failures = []
    for key in sorted(set(derived) & set(from_bytes)):
        allow_key = f"{key[0]}:{key[1]}"
        if allow_key in ALLOWED:
            continue
        for path, number, decoders in derived[key]:
            for fb_path, fb_number in from_bytes[key]:
                failures.append(
                    f"{path}:{number}: `{key[1]}` derives {', '.join(decoders)} but has a fallible "
                    f"`from_bytes` at {fb_path}:{fb_number}"
                )

    if failures:
        print("Decoder parity check failed. These types derive a serde or borsh decoder that skips the checks of")
        print("their fallible `from_bytes`. Route the decoders through the validation instead (see")
        print("`tari_max_size::ValidatedDecode`), or add the type to ALLOWED in scripts/decoder_parity_check.py")
        print("with a reason.\n")
        for failure in failures:
            print(failure)
        return 1
    print("Decoder parity check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
