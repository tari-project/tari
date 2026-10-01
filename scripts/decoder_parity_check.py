#!/usr/bin/env python3
#
# Decoder parity check. Must be run from the repo root. `--self-test` runs the built-in fixtures instead.
#
# 1. Derived decoders on types with a validating constructor.
#
#    A type with a fallible constructor has an invariant that the constructor enforces (the protobuf conversions call
#    it). If the same type derives `Deserialize` or `BorshDeserialize`, the derived decoder skips that check and
#    accepts values the constructor rejects, which is how `EncryptedData` once let a node accept a transaction every
#    peer refused. Such a type must route its serde and borsh decoders through its validation instead (see
#    `tari_max_size::ValidatedDecode`).
#
#    This fails when a type derives one of those decoders (also through `cfg_attr`) and, in the same crate, has
#    - an inherent `from_*`, `try_from_*`, `new`, `new_*`, `try_new*` or `parse*` function of any visibility that returns a
#      `Result` (or a `...Result` alias) or an `Option`, or
#    - an `impl TryFrom<S> for T` where `S` is a primitive, byte or string type, or an `impl FromStr for T`.
#      Conversions from other structured types (protobuf messages, database rows) are not counted: they decode a
#      different encoding field by field.
#    Fieldless enums are exempt: their derived decoders accept exactly the declared variants, so they can not produce
#    a value a constructor would reject. Types that are known to be fine are listed in `ALLOWED`, with a reason.
#
#    A type declared inside a macro (`struct $name`) that derives a decoder can not be matched to its constructors, so
#    it is reported for manual review unless listed in `ALLOWED` as `"<crate dir>:macro@<file>"`.
#
# 2. Node ingress.
#
#    The node is safe against values its peers would reject only because every block or transaction it receives
#    through serde or borsh is round-tripped through the P2P protobuf conversions. This fails when
#    `applications/minotari_node/src` decodes a `Transaction`, `Block`, `BlockHeader`, `AggregateBody` or
#    `NewBlockTemplate` with serde_json, bincode or borsh outside the helpers in `INGRESS_HELPERS` (or a test module).
#
# It is a text scan, not a parser: comments, string literals and char literals are removed first, impl headers may span
# lines, and block bodies are found by matching braces. The decoder parity tests additionally require every type that
# uses `impl_validated_decode!` to keep doing so (see `minotari_app_grpc/tests/decoder_parity.rs`).

import os
import re
import sys

SCAN_DIRS = ["applications", "base_layer", "common", "common_sqlite", "comms", "hashing", "infrastructure"]
SKIP_DIRS = {"target", "node_modules", ".git"}

# "crate dir:type name" (or "crate dir:macro@file") -> reason
ALLOWED = {
    "comms/core:IdentitySignature": "`from_bytes` only decodes the protobuf form; it has no invariant beyond the field "
    "decoders, which the serde form (the peer database) shares",
    # Conversions from another type or encoding, not a validating constructor of the serialized fields
    "base_layer/node_components:NewBlockTemplate": "`from_block` converts a block, it does not validate the fields",
    "base_layer/transaction_components:UnblindedOutput": "`from_wallet_output` converts a wallet output using the key "
    "manager, it does not validate the fields",
    "base_layer/common_types:CipherSeed": "`from_enciphered_bytes` decrypts and authenticates the enciphered seed format "
    "(including its version byte); the plain serde form is a different encoding and is not decoded from untrusted input",
    "infrastructure/jellyfish:TreeHash": "`try_from_bytes` only checks the slice length; the derived decoders read a "
    "fixed `[u8; 32]`",
    # Text forms parsed by `FromStr` / `TryFrom<String>` (configuration, CLI arguments, user input); the serde form is
    # a different encoding whose fields are decoded by their own decoders
    "applications/minotari_app_utilities:UniPublicKey": "`FromStr` parses emoji, base58 or hex text forms",
    "applications/minotari_mcp_node:GrpcMethodConfig": "`FromStr` parses a configuration string",
    "base_layer/common_types:VnEpoch": "`FromStr` parses the number; any u64 is a valid epoch",
    "base_layer/p2p:SeedPeer": "`FromStr` / `TryFrom<String>` parse the configuration text form",
    "base_layer/p2p:SocksAuthentication": "`FromStr` parses the configuration text form",
    "base_layer/p2p:TorControlAuthentication": "`FromStr` / `TryFrom<String>` parse the configuration text form",
    "base_layer/transaction_components:MicroMinotari": "`FromStr` parses an amount with units; any u64 is valid",
    "base_layer/transaction_components:TariKeyId": "`FromStr` parses the key id text form",
    "base_layer/transaction_key_manager:LegacyTariKeyId": "`FromStr` parses the legacy key id text form",
    "common:DnsNameServer": "`FromStr` parses the configuration text form",
    "common_sqlite:DbConnectionUrl": "`TryFrom<String>` parses the configuration text form",
    # Fixed size values: the fallible conversion only checks a slice length, the derived decoders read a fixed array
    "base_layer/common_types:FixedHash": "`TryFrom<&[u8]>` / `TryFrom<Vec<u8>>` only check the length",
    "comms/core:NodeId": "`TryFrom<&[u8]>` only checks the length",
    # Fallible for reasons other than validating the serialized fields
    "base_layer/transaction_components:WalletOutput": "the constructors fail on key manager errors while deriving "
    "the commitment and proofs, not on invalid fields; wallet-local data",
    "base_layer/transaction_components:WalletType": "`new_random` fails only on key derivation errors",
    "base_layer/wallet:CompletedTransaction": "`new` rejects the coinbase status for new records; stored rows are "
    "wallet-local and are read back through the same serde form they were written with",
    # Validating constructors of data that is only decoded from the node's own database
    "base_layer/common_types:ChainMetadata": "`new` rejects a zero accumulated difficulty; the P2P protobuf "
    "conversion builds it through `new`, and the serde form is only read from the node's own database",
    "base_layer/mmr:BranchNode": "`new` checks the children of a sparse Merkle tree branch; the tree is only decoded "
    "from the node's own database, never from peers or RPC input",
    "base_layer/mmr:macro@base_layer/mmr/src/sparse_merkle_tree/node.rs": "`hash_type!` declares fixed `[u8; 32]` "
    "hash wrappers with no constructor checks",
    "infrastructure/storage:User": "test fixture",
    "base_layer/transaction_components:AccumulatedDifficulty": "public API kept for p2pool, whose sliding-window "
    "accounting uses `checked_sub_difficulty` (no callers in this repo) and can produce values below MIN_DIFFICULTY that "
    "a caller may serialize; in this repo it is only decoded from the node's own chain database "
    "(BlockHeaderAccumulatedData), never from peers or RPC input, so it is left lenient",
}

INGRESS_DIR = os.path.join("applications", "minotari_node", "src")
INGRESS_HELPERS = {
    "decode_transaction",
    "normalise_block_via_p2p_proto",
    "normalise_transaction_via_p2p_proto",
    "decode_block_blob",
}
CHAIN_TYPES = r"\b(?:Transaction|Block|BlockHeader|AggregateBody|NewBlockTemplate)\b"

CONSTRUCTOR_RE = re.compile(
    r"\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+"
    r"((?:try_)?from_\w+|(?:try_)?new(?:_\w+)?|parse\w*)\s*(?:<[^>(]*>)?\s*\("
)
TYPE_RE = re.compile(r"\b(?:pub(?:\s*\([^)]*\))?\s+)?(struct|enum)\s+(\$?\w+)")
DERIVE_RE = re.compile(r"\bderive\s*\(([^()]*)\)")
IMPL_RE = re.compile(r"\bimpl\b")
TRAIT_IMPL_RE = re.compile(
    r"^\s*(?:<[^{]*?>\s*)?(?:(?:std|core)::(?:convert|str)::)?(TryFrom\s*<(.+)>|FromStr)\s+for\s+([\w:]+)", re.S
)
PRIMITIVE_SOURCE_RE = re.compile(
    r"^&?\s*(?:'\w+\s+)?(?:mut\s+)?(?:u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|bool|char|str|String"
    r"|Vec\s*<\s*u8\s*>|\[\s*u8\s*(?:;\s*[\w:]+\s*)?\])$"
)
DECODERS = ("Deserialize", "BorshDeserialize")
INGRESS_DECODE_RE = re.compile(
    r"serde_json::from_(?:str|slice|value|reader)|bincode::deserialize|borsh::from_slice|borsh::from_reader"
    r"|BorshDeserialize::deserialize|\bdeserialize_reader\b|\btry_from_slice\b"
)


def strip_comments_and_literals(text):
    """Removes comments, string literals and char literals, keeping newlines so line numbers stay correct."""
    out = []
    i = 0
    n = len(text)
    raw_string = re.compile(r'(?:b|c)?r(#*)"')
    while i < n:
        c = text[i]
        prev = text[i - 1] if i > 0 else ""
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
        elif c in "bcr" and not (prev.isalnum() or prev == "_") and raw_string.match(text, i):
            # A raw string, also as a byte (`br"..."`) or C (`cr"..."`) string: no escapes until `"` and the hashes
            m = raw_string.match(text, i)
            hashes = m.group(1)
            end = text.find('"' + hashes, m.end())
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
            # A char (or byte) literal; a lifetime has no closing quote
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


def attributes_before(text, index):
    """The `#[...]` attributes directly before `index`."""
    attributes = []
    j = index
    while True:
        before = text[:j].rstrip()
        if not before.endswith("]"):
            return attributes
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
            return attributes
        attributes.append(before[k - 1 :])
        j = k - 1


def is_fieldless_enum(text, index):
    open_brace = text.find("{", index)
    semicolon = text.find(";", index)
    if open_brace == -1 or (semicolon != -1 and semicolon < open_brace):
        return False
    body = text[open_brace + 1 : matching_brace(text, open_brace)]
    return "(" not in body.split("=")[0] and "{" not in body and "(" not in re.sub(r"=[^,]*", "", body)


def scan_file(path, derived, constructors, macro_types):
    with open(path, encoding="utf-8", errors="replace") as f:
        text = strip_comments_and_literals(f.read())
    krate = crate_dir(path)

    # Types deriving a decoder
    for m in TYPE_RE.finditer(text):
        decoders = set()
        for attribute in attributes_before(text, m.start()):
            for derive in DERIVE_RE.findall(attribute):
                for name in derive.split(","):
                    name = name.strip().split("::")[-1]
                    if name in DECODERS:
                        decoders.add(name)
        if not decoders:
            continue
        kind, name = m.group(1), m.group(2)
        if name.startswith("$"):
            macro_types.append((krate, path, line_of(text, m.start()), sorted(decoders)))
            continue
        if kind == "enum" and is_fieldless_enum(text, m.end()):
            continue
        derived.setdefault((krate, name), []).append((path, line_of(text, m.start()), sorted(decoders)))

    for m in IMPL_RE.finditer(text):
        open_brace = text.find("{", m.end())
        semicolon = text.find(";", m.end())
        if open_brace == -1 or (semicolon != -1 and semicolon < open_brace):
            continue
        header = text[m.end() : open_brace]
        # `impl TryFrom<primitive> for T` and `impl FromStr for T`
        trait_impl = TRAIT_IMPL_RE.match(header)
        if trait_impl:
            source = trait_impl.group(2)
            if source is None or PRIMITIVE_SOURCE_RE.match(source.strip()):
                type_name = trait_impl.group(3).split("::")[-1]
                what = "FromStr" if source is None else f"TryFrom<{source.strip()}>"
                constructors.setdefault((krate, type_name), []).append((path, line_of(text, m.start()), what))
            continue
        if re.search(r"\bfor\s+(?!<)", header):
            continue  # another trait impl
        # Inherent impls with a fallible constructor
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


def allowed_regions(text):
    """The spans of the ingress helper functions and of `#[cfg(test)]` modules"""
    regions = []
    for m in re.finditer(r"\bfn\s+(\w+)", text):
        if m.group(1) in INGRESS_HELPERS:
            open_brace = text.find("{", m.end())
            regions.append((m.start(), matching_brace(text, open_brace)))
    for m in re.finditer(r"#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{", text):
        regions.append((m.start(), matching_brace(text, m.end() - 1)))
    return regions


def scan_ingress_file(path, findings):
    with open(path, encoding="utf-8", errors="replace") as f:
        text = strip_comments_and_literals(f.read())
    regions = allowed_regions(text)
    for m in INGRESS_DECODE_RE.finditer(text):
        if any(start <= m.start() <= end for start, end in regions):
            continue
        # The statement around the call
        start = max(text.rfind(c, 0, m.start()) for c in ";{}") + 1
        ends = [i for i in (text.find(";", m.end()), text.find("{", m.end())) if i != -1]
        end = min(ends) if ends else len(text)
        statement = text[start:end]
        untyped_borsh = "Borsh" in m.group(0) or "borsh" in m.group(0) or "deserialize_reader" in m.group(0)
        if re.search(CHAIN_TYPES, statement) or (untyped_borsh and "::<" not in statement):
            findings.append(f"{path}:{line_of(text, m.start())}: `{m.group(0)}` outside the ingress helpers")


def scan_tree():
    derived = {}
    constructors = {}
    macro_types = []
    ingress = []
    for top in SCAN_DIRS:
        for root, dirs, files in os.walk(top):
            dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
            for name in sorted(files):
                if name.endswith(".rs"):
                    path = os.path.join(root, name)
                    scan_file(path, derived, constructors, macro_types)
                    if path.startswith(INGRESS_DIR + os.sep):
                        scan_ingress_file(path, ingress)
    return findings(derived, constructors, macro_types) + ingress


def findings(derived, constructors, macro_types):
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
    for krate, path, number, decoders in macro_types:
        if f"{krate}:macro@{path}" in ALLOWED:
            continue
        failures.append(
            f"{path}:{number}: a type declared in a macro derives {', '.join(decoders)}; review it and allowlist it as "
            f'"{krate}:macro@{path}"'
        )
    return failures


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
struct D { x: u8 }
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
    "byte_raw_string": r"""
const X: &[u8] = br"\";
const Y: &[u8] = br#"\"#;
const Z: &str = r"\";
#[derive(Deserialize)]
pub struct G(Vec<u8>);
impl G {
    pub fn from_bytes(b: &[u8]) -> Result<Self, E> { todo!() }
}
""",
    "fallible_new": """
#[derive(Deserialize)]
pub struct H(u8);
impl H {
    pub fn new(b: u8) -> Result<Self, E> { todo!() }
}
""",
    "fallible_parse": """
#[derive(BorshDeserialize)]
pub struct I(u8);
impl I {
    pub fn parse_hex(s: &str) -> Option<Self> { todo!() }
}
""",
    "try_from_primitive": """
#[derive(Deserialize)]
pub struct J(u8);
impl TryFrom<&[u8]> for J {
    type Error = E;
    fn try_from(b: &[u8]) -> Result<Self, E> { todo!() }
}
""",
    "from_str": """
#[derive(Deserialize)]
pub struct K { x: u8 }
impl std::str::FromStr for K {
    type Err = E;
    fn from_str(s: &str) -> Result<Self, E> { todo!() }
}
""",
    "macro_type": """
macro_rules! make {
    ($name:ident) => {
        #[derive(Clone, Deserialize)]
        pub struct $name(Vec<u8>);
    };
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
    pub fn new(a: u8) -> Self { todo!() }
}
impl TryFrom<proto::F> for F {
    type Error = E;
    fn try_from(f: proto::F) -> Result<Self, E> { todo!() }
}
#[derive(Deserialize, BorshDeserialize)]
pub enum Fieldless { A = 1, B = 2 }
impl Fieldless {
    pub fn from_byte(b: u8) -> Option<Self> { todo!() }
}
// impl F { pub fn from_bytes(b: &[u8]) -> Result<Self, E> {} }
"""

SELF_TEST_INGRESS = {
    # Each must be reported
    "typed_serde": "fn handler(v: Value) { let tx = serde_json::from_value::<Transaction>(v); }",
    "annotated_serde": "fn handler(s: &str) { let block: Block = serde_json::from_str(s).unwrap(); }",
    "untyped_borsh": "fn handler(mut b: &[u8]) { let header = BorshDeserialize::deserialize(&mut b)?; }",
    "bincode_template": "fn handler(b: &[u8]) { let t = bincode::deserialize::<NewBlockTemplate>(b); }",
}

SELF_TEST_INGRESS_CLEAN = """
fn decode_transaction(v: Value) -> Transaction { serde_json::from_value::<Transaction>(v).unwrap() }
fn decode_block_blob(mut h: &[u8]) { let header = BorshDeserialize::deserialize(&mut h)?; }
fn config(s: &str) { let c: ConsensusConstants = serde_json::from_str(s).unwrap(); }
#[cfg(test)]
mod test {
    fn t(b: &[u8]) { let block = borsh::from_slice::<Block>(b).unwrap(); }
}
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
            macro_types = []
            scan_file(path, derived, constructors, macro_types)
            found = bool(findings(derived, constructors, macro_types))
            if found != (name != "clean"):
                print(f"self-test {name}: expected {'a finding' if name != 'clean' else 'no finding'}")
                failed = True
        for name, source in list(SELF_TEST_INGRESS.items()) + [("ingress_clean", SELF_TEST_INGRESS_CLEAN)]:
            path = os.path.join(d, name + ".rs")
            with open(path, "w") as f:
                f.write(source)
            ingress = []
            scan_ingress_file(path, ingress)
            if bool(ingress) != (name != "ingress_clean"):
                print(f"self-test {name}: expected {'a finding' if name != 'ingress_clean' else 'no finding'}")
                failed = True
    print("Self-test failed." if failed else "Self-test passed.")
    return 1 if failed else 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    failures = scan_tree()
    if failures:
        print("Decoder parity check failed. Types that derive a serde or borsh decoder must not skip the checks of a")
        print("fallible constructor: route the decoders through the validation instead (see")
        print("`tari_max_size::ValidatedDecode`), or add the type to ALLOWED in scripts/decoder_parity_check.py with a")
        print("reason. Blocks and transactions decoded by the node must go through its P2P proto round-trip helpers.\n")
        for failure in failures:
            print(failure)
        return 1
    print("Decoder parity check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
