# RFC-0121/Consensus Encoding

## Consensus Encoding

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Stanley Bondi](https://github.com/sdbondi)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2022 The Tari Development Community

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS", AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", 
"NOT RECOMMENDED", "MAY" and "OPTIONAL" in this document are to be interpreted as described in 
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as 
shown here.

## Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community of the
technological merits of the potential system outlined herein.

## Goals

The aim of this Request for Comment (RFC) is to describe the encoding used for various consensus-critical data types, as well as the construction of hash pre-images and 
signature challenges used in base-layer consensus. 

## Related Requests for Comment

* [RFC-0100: Base Layer](RFC-0100_BaseLayer.md)
* [RFC-0120: Consensus](RFC-0120_Consensus.md)

## Description

A Tari base node must validate each block containing a [block header] as well as set of transaction inputs, transaction outputs and transaction kernels,
each containing a number of fields pertinent to their function within the [base layer]. The data contained within these structures needs to be consistently encoded
(represented as bytes) across platforms and implementations so that the network can agree on a single correct state.

This RFC defines the low-level specification for how these data types MUST be encoded to construct a valid hash and signature on the Tari network.

### Consensus Encoding
[Consensus Encoding]: #consensus-encoding "Consensus Encoding"

The primary goal of consensus encoding is to provide a consistent data format that is committed to in hashes and signatures.

Consensus encoding defines what "raw" data is included in the encoding, the order in which it should appear and the length for variable length elements.
To keep encoding as simple as possible, no type information, field names etc. are catered for in the format as this is always statically known. 
This is particularly appropriate for hashes and signatures where many fields must be consistently represented and concatenated together.

The rest of this section defines some encodings for common primitives used in the Tari codebase.

#### Unsigned integer encoding

Varint encoding is used for integer fields greater than 1 byte. Describing varint is out of scope for this RFC but there are many resources online
to understand this fairly basic encoding. The only rule we apply is that the encoding has a limit of 10 bytes, a little more than
what is required to store a 64-bit integer.

#### Dynamically-sized vec encoding

This type refers to a contiguous block of data of any length. Because the size is dynamic, the size is included in the encoding.

```text
|len(data)| data for type | data for type | ...
```

#### Fixed size arrays

If the size of the array is constant (static). The length is omitted and the data is encoded.

```text
| data for type | ...
```

#### Optional or nullable encoding

An optional field starts with a 0x00 byte to indicate the value is not provided (`None`, `null`, `nil` etc) or a 0x01 byte 
to indicate that the value is provided followed by the encoding of the value.

```text
| 0 or 1 | encoding for type |
```

#### Ristretto Keys

`RistrettoPublicKey` and `RistrettoPrivateKey` types defined in the `tari_crypto` crate both have 32-byte canonical formats
and are encoded as a 32-byte fixed array.

The [`tari_crypto`](https://github.com/tari-project/tari-crypto) Rust crate provides an FFI interface that allows
generating of the canonical byte formats in any language that supports FFI.

#### Commitment

A commitment is a [RistrettoPublicKey] and so has identical encoding.

#### Schnorr Signature

See the [TLU on Schnorr Signatures](https://tlu.tarilabs.com/cryptography/introduction-schnorr-signatures)

A Schnorr signature tuple is `<R, s>` where `R` is a [RistrettoPublicKey] and `s` is a the signature scalar wrapped in [RistrettoPrivateKey].

The encoding is fixed at 64-bytes:

```text
| 32-byte public key | 32-byte scalar |
```

#### Signature

A signature tuple consists of a `<R, s>` where `R` is the public nonce and `s` is the signature scalar.

The encoding is fixed at 64-bytes:

```text
| 32-byte commitment (R) | 32-byte scalar (s) |
```

#### Commitment Signature

A commitment signature tuple consists of a `<R, u, v>` where `R` is the [Pederson commitment](./Glossary.md#commitment) \\(r_u.G + r_v.H\\)
for the signature scalars `u` and `v`.

The encoding is fixed at 96-bytes:

```text
| 32-byte commitment (R) | 32-byte scalar (u) | 32-byte scalar (v) |
```

#### Example

Given the following data and types:

```javascript
{
  // Type: Fixed array of 5 bytes
  short_id: [1,2,3,4,5],
  // Type: variable length bytes
  name: Buffer.from("Case"),
  // Type: unsigned integer
  age: 40,
  // Type: struct
  details: {
      // Type: variable length bytes
      kind: Buffer.from("Hacker"),
  },
  // Type: nullable varint
  dob: null
}
```

Encoded (hex) as follows:

| short id |len|  name  | age |len| kind         | null? | dob |
|----------|---|--------|-----|---|--------------|-------|-----|
|0102030405|04 |43617365| 28  |05 | 4861636b6572 | 00    |     |

Note that nested structs are flattened and the order must be preserved to allow decoding.
The `00` null byte is important so that for e.g. the `kind` bytes cannot be manipulated to 
produce the same encoding as non-null `dob`.

### Block Header 
[block header]: #block-header "Block header"

The block hash pre-image is constructed by first constructing the merge mining hash. Each encoding is concatenated in order as follows:

1. `version` - 1 byte
1. `height` - varint
1. `prev_hash` - fixed 32-bytes 
1. `timestamp` - varint
1. `input_mr` - fixed 32-bytes
1. `output_mr` - fixed 32-bytes
1. `output_mmr_size` - varint
1. `witness_mr` - fixed 32-bytes
1. `kernel_mr` - fixed 32-bytes
1. `kernel_mmr_size` - `varint
1. `total_kernel_offset` - 32-byte Scalar, see [RistrettoPrivateKey]
1. `total_script_offset` - 32-byte Scalar, see [RistrettoPrivateKey]

This pre-image is hashed and block hash is constructed, in order, as follows:

1. `merge_mining_hash` - As above
1. `pow_algo` - enumeration of types of PoW as a single unsigned byte, where `Monero = 0x00` and `Sha3 = 0x01`
1. `pow_data` - raw variable bytes (no length varint)
1. `nonce` - the PoW nonce, `u64` converted to a fixed 8-byte array (little endian)

#### Output Features 
[Output Features]: #output-metadata-and-features "Output Features"

```rust,ignore
pub struct OutputFeatures {
  pub version: OutputFeaturesVersion,
  pub maturity: u64,
  pub flags: OutputFlags,
  pub metadata: Vec<u8>,
  pub unique_id: Option<Vec<u8>>,
  pub parent_public_key: Option<PublicKey>,
  pub asset: Option<AssetOutputFeatures>,
  pub mint_non_fungible: Option<MintNonFungibleFeatures>,
  pub sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
}
```

Output features consensus encoding is defined as follows (in order):

1. `version` - 1 unsigned byte. This should always be `0x00` but is reserved for future proofing.
1. `maturity` - [varint]
1. `flags` - 1 unsigned byte
1. `metadata` - [dynamic vector]
1. `unique_id` - [nullable] + [dynamic vector]
1. `parent_public_key` - [nullable] + 32-byte compressed public key 
1. `asset` - [nullable] + [AssetOutputFeatures](#assetoutputfeatures)
1. `mint_non_fungible` - [nullable] + [MintNonFungibleFeatures](#mintnonfungiblefeatures)
1. `sidechain_checkpoint` - [nullable] + [SideChainCheckpointFeatures](#sidechaincheckpointfeatures)

##### AssetOutputFeatures

- `public_key` - [RistrettoPublicKey] 
- `template_ids` - [dynamic vector] + [varint]
- `template_parameters` - [dynamic vector]

##### MintNonFungibleFeatures

- `asset_public_key` - [RistrettoPublicKey] 
- `asset_owner_commitment` - [RistrettoPublicKey] 

##### SideChainCheckpointFeatures

- `merkle_root` - [fixed sized array]
- `committee` - [dynamic vector] + [RistrettoPublicKey]

#### Transaction Output
[Transaction Output]: #transaction-output "Transaction Output"

```rust,ignore
pub struct TransactionOutput {
    pub version: TransactionInputVersion,
    pub features: OutputFeatures,
    pub commitment: Commitment,
    pub proof: RangeProof,
    pub script: TariScript,
    pub sender_offset_public_key: PublicKey,
    pub metadata_signature: ComSignature,
    pub covenant: Covenant,
}
```

The canonical output hash is appended to the output Merkle tree and commits to the common data between an output 
and the input spending that output i.e. `output_hash = Hash(version | features | commitment | script | covenant)`. 

The encoding is defined as follows:

- `version` - 1 byte
- `features` - [OutputFeatures]
- `commitment` - [RistrettoPublicKey]
- `script` - byte length as [varint] + [TariScript]
- `covenant` - byte length as [varint] + [Covenant]

##### Witness hash

The witness hash is appended to the witness Merkle tree.

- `proof` - Raw proof bytes encoded using [dynamic vector] encoding
- `metadata_signature` - [CommitmentSignature]

##### Metadata signature challenge

See [Metadata Signature](./Glossary.md#metadata-signature) for details.

- `public_commitment_nonce` - [RistrettoPublicKey]
- `script` - byte length as [varint] + [TariScript]
- `features` - [OutputFeatures]
- `sender_offset_public_key` - [RistrettoPublicKey]
- `commitment` - [RistrettoPublicKey]
- `covenant`- byte length as [varint] + [Covenant]

### Transaction Input

The following struct represents the full transaction input data for reference. The actual input struct does not duplicate the output data
to optimise storage and transmission of the input. 

```rust,ignore
pub struct TransactionInput {
  pub version: u8,
  pub input_data: ExecutionStack,
  pub script_signature: ComSignature,
  pub output_version: TransactionOutputVersion,
  pub features: OutputFeatures,
  pub commitment: Commitment,
  pub script: TariScript,
  pub sender_offset_public_key: PublicKey,
  pub covenant: Covenant, 
}
```

The transaction input canonical hash pre-image is constructed as follows:

- `input_version` - 1 byte
- `output_hash` - See [TransactionOutput]
- `sender_offset_public_key` - [RistrettoPublicKey]
- `input_data` - [TariScript Stack]
- `script_signature` - [CommitmentSignature]

### Transaction Kernel

The following struct represents the full transaction input data for reference. The actual input struct does not duplicate the output data
to optimise storage and transmission of the input.

```rust,ignore
pub struct TransactionKernel {
    pub version: TransactionKernelVersion,
    pub features: KernelFeatures,
    pub fee: MicroTari,
    pub lock_height: u64,
    pub excess: Commitment,
    pub excess_sig: Signature,
}
```

The transaction kernel is encoded as follows:

- `input_version` - 1 byte
- `features` - [OutputFeatures]
- `fee` - [RistrettoPublicKey]
- `lock_height` - [TariScript Stack]
- `excess` - [Commitment]
- `excess_sig` - [Signature]

The canonical hash pre-image is constructed from this encoding.

#### Script Challenge

For details see [RFC-0201_TariScript.md](./RFC-0201_TariScript.md).

The script challenge is constructed as follows:

- `nonce_commitment` - [Commitment]
- `script` - [TariScript]
- `input_data` - [TariScript Stack]
- `script_public_key` - [RistrettoPublicKey]
- `commitment` - [Commitment]

[varint]: #unsigned-integer-encoding
[Covenant]: RFC-0250_Covenants.md
[nullable]: #optional-or-nullable-encoding
[dynamic vector]: #dynamically-sized-vec-encoding
[RistrettoPublicKey]: #ristretto-keys
[RistrettoPrivateKey]: #ristretto-keys
[Ristretto]: https://docs.rs/curve25519-dalek/3.1.0/curve25519_dalek/ristretto/index.html
[TariScript]: https://github.com/tari-project/tari-crypto/blob/09cc52787272ced3a1a8c9f2edc1e0221f9d8faa/src/script/op_codes.rs#L101
[TariScript Stack]: https://github.com/tari-project/tari-crypto/blob/09cc52787272ced3a1a8c9f2edc1e0221f9d8faa/src/script/stack.rs#L51
[OutputFeatures]: #output-features
[Commitment]: #commitment
[fixed sized array]: #fixed-size-arrays
[block header]: Glossary.md#block-header
