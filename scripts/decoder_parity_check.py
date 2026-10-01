#!/usr/bin/env python3
#
# Decoder parity check. Must be run from the repo root.
#
# A type with a fallible constructor such as `from_bytes` has an invariant that the constructor enforces (the protobuf
# conversions call it). If the same type derives `Deserialize` or `BorshDeserialize`, the derived decoder skips that
# check and accepts values the constructor rejects, which is how `EncryptedData` once let a node accept a transaction
# every peer refused. Such a type must route its serde and borsh decoders through its validation instead (see
# `tari_max_size::ValidatedDecode`).
#
# This script fails when a type derives one of those decoders (also through `cfg_attr`) and has, in the same crate, an
# inherent `from_*` or `try_from_*` function of any visibility that returns a `Result` (or a `...Result` alias) or an
# `Option`. Types that are known to be fine are listed in `ALLOWED`, with a reason.
#
# It is a text scan, not a parser: comments, string literals and char literals are removed first, impl headers may span
# lines, and block bodies are found by matching braces.

import os
import re
import sys

SCAN_DIRS = ["applications", "base_layer", "common", "common_sqlite", "comms", "hashing", "infrastructure"]
SKIP_DIRS = {"target", "node_modules", ".git"}

# "crate dir:type name" -> reason
ALLOWED = {
    "comms/core:IdentitySignature": "`from_bytes` only decodes the protobuf form; it has no invariant beyond the field "
    "decoders, which the serde form (the peer database) shares",
    # Fieldless enums: `from_byte` / `from_u8` / `from_u16` map exactly the declared variants, which is the set the
    # derived decoders accept
    "applications/minotari_ledger_wallet/common:LedgerKeyBranch": "fieldless enum, from_byte maps every variant",
    "base_layer/sidechain:QuorumDecision": "fieldless enum, from_u8 maps every variant",
    "base_layer/transaction_components:BlockVersion": "fieldless enum, from_u16 maps every variant",
    "base_layer/transaction_components:OutputField": "fieldless enum, from_byte maps every variant",
    "base_layer/transaction_components:OutputType": "fieldless enum, from_byte maps every variant",
    "base_layer/transaction_components:RangeProofType": "fieldless enum, from_byte maps every variant",
    # Conversions from another type or encoding, not a validating constructor of the serialized fields
    "base_layer/node_components:NewBlockTemplate": "`from_block` converts a block, it does not validate the fields",
    "base_layer/transaction_components:UnblindedOutput": "`from_wallet_output` converts a wallet output using the key "
    "manager, it does not validate the fields",
    "base_layer/common_types:CipherSeed": "`from_enciphered_bytes` decrypts and authenticates the enciphered seed format "
    "(including its version byte); the plain serde form is a different encoding and is not decoded from untrusted input",
    "infrastructure/jellyfish:TreeHash": "`try_from_bytes` only checks the slice length; the derived decoders read a "
    "fixed `[u8; 32]`",
    "base_layer/transaction_components:AccumulatedDifficulty": "not an invariant: `checked_sub_difficulty` can produce "
    "values below MIN_DIFFICULTY, and the value is stored in the chain database, so a stricter decoder could make an "
    "existing database unreadable",
}

CONSTRUCTOR_RE = re.compile(
    r"\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+((?:try_)?from_\w+)\b"
)
TYPE_RE = re.compile(r"\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)")
DERIVE_RE = re.compile(r"\bderive\s*\(([^()]*)\)")
IMPL_RE = re.compile(r"\bimpl\b")
DECODERS = ("Deserialize", "BorshDeserialize")


def strip_comments_and_literals(text):
    """Removes comments, string literals and char literals, keeping newlines so line numbers stay correct."""
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            end = text.find("\n", i)
            i = n if end == -1 else end
        elif text.startswith("/*", i):
            # Block comments nest in Rust
            depth = 0
            while i < n:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                    if depth == 0:
                        break
                else:
                    if text[i] == "\n":
                        out.append("\n")
                    i += 1
        elif c == "r" and re.match(r'r#*"', text[i:]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = re.match(r"r(#*)\"", text[i:]).group(1)
            end = text.find('"' + hashes, i + 2 + len(hashes))
            end = n if end == -1 else end + 1 + len(hashes)
            out.append('""' + "\n" * text.count("\n", i, end))
            i = end
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append('""' + "\n" * text.count("\n", i, j))
            i = j + 1
        elif c == "'" and re.match(r"'(?:\\.[^']*|[^'\\])'", text[i:]):
            # A char literal; a lifetime has no closing quote
            m = re.match(r"'(?:\\.[^']*|[^'\\])'", text[i:])
            out.append("''")
            i += m.end()
        else:
            out.append(c)
            i += 1
    return "".join(out)


def matching_brace(text, open_index):
    depth = 0
    for i in range(open_index, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    return len(text)


def skip_generics(text, i):
    """Returns the index after a balanced `<...>` starting at `i` (or `i` if there is none)."""
    while i < len(text) and text[i].isspace():
        i += 1
    if i >= len(text) or text[i] != "<":
        return i
    depth = 0
    while i < len(text):
        if text[i] == "<":
            depth += 1
        elif text[i] == ">" and text[i - 1] != "-":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return i


def line_of(text, index):
    return text.count("\n", 0, index) + 1


def crate_dir(path):
    d = os.path.dirname(path)
    while d and d != "." and d != os.path.dirname(d):
        if os.path.isfile(os.path.join(d, "Cargo.toml")):
            return d
        d = os.path.dirname(d)
    return "."


def scan_file(path, derived, constructors):
    with open(path, encoding="utf-8", errors="replace") as f:
        text = strip_comments_and_literals(f.read())
    krate = crate_dir(path)

    # Types deriving a decoder: the attributes directly above a struct or enum
    for m in TYPE_RE.finditer(text):
        attributes = []
        j = m.start()
        while True:
            before = text[:j].rstrip()
            if not before.endswith("]"):
                break
            # Walk back to the `#[` that opens this attribute
            depth = 0
            k = len(before) - 1
            while k >= 0:
                if before[k] == "]":
                    depth += 1
                elif before[k] == "[":
                    depth -= 1
                    if depth == 0:
                        break
                k -= 1
            if k <= 0 or before[k - 1] != "#":
                break
            attributes.append(before[k - 1 :])
            j = k - 1
        decoders = set()
        for attribute in attributes:
            for derive in DERIVE_RE.findall(attribute):
                for name in derive.split(","):
                    name = name.strip().split("::")[-1]
                    if name in DECODERS:
                        decoders.add(name)
        if decoders:
            derived.setdefault((krate, m.group(1)), []).append((path, line_of(text, m.start()), sorted(decoders)))

    # Inherent impls with a fallible `from_*` / `try_from_*` function
    for m in IMPL_RE.finditer(text):
        open_brace = text.find("{", m.end())
        semicolon = text.find(";", m.end())
        if open_brace == -1 or (semicolon != -1 and semicolon < open_brace):
            continue
        header = text[m.end() : open_brace]
        if re.search(r"\bfor\s+(?!<)", header):
            continue  # a trait impl
        rest = header[skip_generics(header, 0) :]
        type_match = re.match(r"\s*(?:dyn\s+)?([\w:]+)", rest)
        if not type_match:
            continue
        type_name = type_match.group(1).split("::")[-1]
        close_brace = matching_brace(text, open_brace)
        body = text[open_brace + 1 : close_brace]
        for fn in CONSTRUCTOR_RE.finditer(body):
            signature_end = len(body)
            for terminator in ("{", ";"):
                k = body.find(terminator, fn.end())
                if k != -1:
                    signature_end = min(signature_end, k)
            signature = body[fn.end() : signature_end]
            returns = signature.split("->", 1)[1] if "->" in signature else ""
            returns = re.split(r"\bwhere\b", returns)[0]
            if re.search(r"\w*Result\b|\bOption\b", returns):
                constructors.setdefault((krate, type_name), []).append(
                    (path, line_of(text, open_brace + 1 + fn.start()), fn.group(1))
                )


SELF_TEST_CASES = {
    # Each must be reported
    "multi_line_where": """
#[derive(Deserialize)]
pub struct A(Vec<u8>);
impl<T>
    A
where
    T: Clone,
{
    pub fn from_bytes(b: &[u8]) -> Result<Self, E> { todo!() }
}
""",
    "crate_visibility_const": """
#[derive(Clone, BorshDeserialize)]
pub(crate) struct B(u8);
impl B {
    pub(crate) const fn from_byte(b: u8)
        -> Option<Self> { todo!() }
}
""",
    "result_alias_and_private": """
#[derive(serde::Deserialize)]
struct C(u8);
impl C {
    fn try_from_slice(b: &[u8]) -> io::Result<Self> { todo!() }
}
""",
    "cfg_attr_derive": """
#[cfg_attr(feature = "x", derive(Debug, serde::Deserialize))]
enum D { X }
impl D {
    pub fn from_bits(b: u8) -> DecodeResult<Self> { todo!() }
}
""",
    "block_comment_braces": """
#[derive(Deserialize)]
pub struct E(u8);
/* } } */
impl E {
    /* { */
    pub fn from_hex(s: &str) -> Result<Self, E> { let x = "}"; let y = '}'; todo!() }
}
""",
}

SELF_TEST_CLEAN = """
#[derive(Deserialize)]
pub struct F(u8);
impl Trait for F {
    fn from_bytes(b: &[u8]) -> Result<Self, E> { todo!() }
}
impl F {
    pub fn from_parts(a: u8) -> Self { todo!() }
}
// impl F { pub fn from_bytes(b: &[u8]) -> Result<Self, E> {} }
"""


def self_test():
    import tempfile

    failed = False
    with tempfile.TemporaryDirectory() as d:
        for name, source in list(SELF_TEST_CASES.items()) + [("clean", SELF_TEST_CLEAN)]:
            path = os.path.join(d, name + ".rs")
            with open(path, "w") as f:
                f.write(source)
            derived = {}
            constructors = {}
            scan_file(path, derived, constructors)
            found = bool(set(derived) & set(constructors))
            if found != (name != "clean"):
                print(f"self-test {name}: expected {'a finding' if name != 'clean' else 'no finding'}")
                failed = True
    print("Self-test failed." if failed else "Self-test passed.")
    return 1 if failed else 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    derived = {}
    constructors = {}
    for top in SCAN_DIRS:
        for root, dirs, files in os.walk(top):
            dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
            for name in sorted(files):
                if name.endswith(".rs"):
                    scan_file(os.path.join(root, name), derived, constructors)

    failures = []
    for key in sorted(set(derived) & set(constructors)):
        if f"{key[0]}:{key[1]}" in ALLOWED:
            continue
        for path, number, decoders in derived[key]:
            for fn_path, fn_number, fn_name in constructors[key]:
                failures.append(
                    f"{path}:{number}: `{key[1]}` derives {', '.join(decoders)} but has a fallible "
                    f"`{fn_name}` at {fn_path}:{fn_number}"
                )

    if failures:
        print("Decoder parity check failed. These types derive a serde or borsh decoder that skips the checks of")
        print("their fallible constructor. Route the decoders through the validation instead (see")
        print("`tari_max_size::ValidatedDecode`), or add the type to ALLOWED in scripts/decoder_parity_check.py")
        print("with a reason.\n")
        for failure in failures:
            print(failure)
        return 1
    print("Decoder parity check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
