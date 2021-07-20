# RFC-0160/BlockSerialization

## Tari Block Binary Serialization

![status: draft](https://github.com/tari-project/tari/raw/master/RFC/src/theme/images/status-draft.svg)

**Maintainer(s)**: [Byron Hambly](https://github.com/delta1)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2021 The Tari Development Community

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

The aim of this Request for Comment (RFC) is to specify the binary serialization of:

1. a mined Tari block
1. a Tari block mining template

This is to facilitate interoperability of mining software and hardware.

## Related Requests for Comment

- [RFC-0131: Full-node Mining on Tari Base Layer](RFC-0131_Mining.md)

## Specification

By reviewing the [block and mining template fields](#tari-block-and-mining-template-data-types) below, we have the following underlying data types for serialization:

1. `bool`
1. `u8`
1. `u16`
1. `u32`
1. `Vec<u8>`

For 1. to 4. and all numbers, [Base 128 Varint] encoding MUST be used.

From the Protocol Buffers documentation:

> Varints are a method of serializing integers using one or more bytes. Smaller numbers take a smaller number of bytes. Each byte in a varint, except the last byte, has the most significant bit (msb) set – this indicates that there are further bytes to come. The lower 7 bits of each byte are used to store the two's complement representation of the number in groups of 7 bits, least significant group first.

For 5., the dynamically sized `Vec` array type, the encoded array MUST be preceded by a number indicating the length of the array. This length MUST also be encoded as a varint. By prepending the length of the array, the decoder knows how many elements to decode as part of the sequence.

[base 128 varint]: https://developers.google.com/protocol-buffers/docs/encoding#varints

## Block field ordering

Using this varint encoding, all fields of the complete block MUST be encoded in the following order:

1. Version
2. Height
3. Previous block hash
4. Timestamp
5. Output Merkle root
6. Witness Merkle root
7. Output Merkle mountain range size
8. Kernel Merkle root
9. Kernel Merkle mountain range size
10. Input Merkle root
11. Total kernel offset
12. Total script offset
13. Nonce
14. Proof of work algorithm
15. Proof of work supplemental data
16. Transaction inputs - for each input:
    - Flags
    - Maturity
    - Commitment
    - Script
    - Input data
    - Script signature
    - Sender Offset
17. Transaction outputs - for each output:
    - Flags
    - Maturity
    - Commitment
    - Range proof
    - Script
    - Sender Offset
    - Signature
18. Transaction kernels - for each kernel:
    - Features
    - Fee
    - Lock height
    - Excess
    - Excess signature public nonce
    - Excess signature

## Mining template field ordering

The [new block template](#mining-template-header) is provided to miners to complete. Its fields MUST also be encoded using varints, in the following order:

1. Version
2. Height
3. Previous block hash
4. Total kernel offset
5. Total script offset
6. Proof of work algorithm
7. Proof of work supplemental data
8. Target difficulty
9. Reward
10. Total fees
11. Transaction inputs - for each input:
    - Flags
    - Maturity
    - Commitment
    - Script
    - Input data
    - Script signature
    - Sender Offset
12. Transaction outputs - for each output:
    - Flags
    - Maturity
    - Commitment
    - Range proof
    - Script
    - Sender Offset
    - Signature
13. Transaction kernels - for each kernel:
    - Features
    - Fee
    - Lock height
    - Excess
    - Excess signature public nonce
    - Excess signature

## Tari Block and Mining Template - Data Types

A Tari block is comprised of the [block header] and [aggregate body].

Here we describe the respective Rust types of these fields in the [tari codebase], and their underlying data types:

[tari codebase]: https://github.com/tari-project/tari/blob/development/base_layer/core/src/blocks/block.rs#L68

### Block Header

| Field                   | Abstract Type    | Data Type | Description                                                                                                                            |
| ----------------------- | ---------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Version                 | `u16`            | `u16`     | The Tari protocol version number, used for soft/hard forks                                                                             |
| Height                  | `u64`            | `u64`     | Height of this block since the genesis block                                                                                           |
| Previous Block Hash     | `BlockHash`      | `[u8;32]` | Hash of the previous block in the chain                                                                                                |
| Timestamp               | `EpochTime`      | `u64`     | Timestamp at which the block was built (number of seconds since Unix epoch)                                                            |
| Output Merkle Root      | `BlockHash`      | `[u8;32]` | Merkle Root of the unspent transaction ouputs                                                                                          |
| Witness Merkle Root     | `BlockHash`      | `[u8;32]` | MMR root of the witness proofs                                                                                                         |
| Output MMR Size         | `u64`            | `u64`     | The size (number of leaves) of the output and range proof MMRs at the time of this header                                              |
| Kernel Merkle Root      | `BlockHash`      | `[u8;32]` | MMR root of the transaction kernels                                                                                                    |
| Kernel MMR Size         | `u64`            | `u64`     | Number of leaves in the kernel MMR                                                                                                     |
| Input Merkle Root       | `BlockHash`      | `[u8;32]` | Merkle Root of the transaction inputs in this block                                                                                    |
| Total Kernel Offset     | `BlindingFactor` | `[u8;32]` | Sum of kernel offsets for all transaction kernels in this block                                                                        |
| Total Script Offset     | `BlindingFactor` | `[u8;32]` | Sum of script offsets for all transaction kernels in this block                                                                        |
| Nonce                   | `u64`            | `u64`     | Nonce increment used to mine this block                                                                                                |
| Proof of Work Algorithm | `PowAlgorithm`   | `u8`      | Proof of Work Algorithm used to mine this block (Monero or SHA3 )                                                                      |
| Proof of Work Data      | `Vec<u8>`        | `Vec<u8>` | Supplemental proof of work data. For Sha3 this would be empty, but for a Monero block we need the Monero header and RandomX seed hash. |

`[u8;32]` indicates an array of 32 unsigned 8-bit integers

### Block Body

| Field               | Abstract Type            | Data Type           | Description                                                            |
| ------------------- | ------------------------ | ------------------- | ---------------------------------------------------------------------- |
| Transaction Inputs  | `Vec<TransactionInput>`  | `TransactionInput`  | List of inputs spent                                                   |
| Transaction Outputs | `Vec<TransactionOutput>` | `TransactionOutput` | List of outputs produced                                               |
| Transaction Kernels | `Vec<TransactionKernel>` | `TransactionKernel` | Kernels contain the excesses and their signatures for the transactions |

A further breakdown of the body fields is described below:

#### TransactionInput

| Field            | Abstract Type        | Data Type                             | Description                                                                     |
| ---------------- | -------------------- | ------------------------------------- | ------------------------------------------------------------------------------- |
| Features         | `OutputFeatures`     | See [OutputFeatures](#OutputFeatures) | The features of the output being spent. We will check maturity for all outputs. |
| Commitment       | `PedersenCommitment` | `[u8;32]`                             | The commitment referencing the output being spent.                              |
| Script           | `TariScript`         | `Vec<u8>`                             | The serialised script, maximum size is 512                                      |
| Input Data       | `ExecutionStack`     | `Vec<u8>`                             | The script input data, maximum size is 512                                      |
| Script Signature | `ComSignature`       | See [ComSignature](#ComSignature)     | A signature with $k_s$, signing the script, input data, and mined height        |
| Sender Offset    | `PublicKey`          | `[u8;32]`                             | The offset public key, $K_O$                                                    |

##### OutputFeatures

| Field    | Abstract Type | Data Type | Description                                                                           |
| -------- | ------------- | --------- | ------------------------------------------------------------------------------------- |
| Flags    | `OutputFlags` | `u8`      | Feature flags that differentiate the output, for example to specify a coinbase output |
| Maturity | `u64`         | `u64`     | The block height at which the output can be spent                                     |

##### ComSignature

| Field        | Abstract Type        | Data Type | Description                                                                             |
| ------------ | -------------------- | --------- | --------------------------------------------------------------------------------------- |
| Public Nonce | `PedersenCommitment` | `[u8;32]` | public (Pedersen) commitment nonce created with the two random nonces                   |
| `u`          | `SecretKey`          | `[u8;32]` | the first publicly known private key of the signature signing with the value            |
| `v`          | `SecretKey`          | `[u8;32]` | the second publicly known private key of the signature signing with the blinding factor |

Find out more about Commitment signatures:

- [Simple Schnorr Signature with Pedersen Commitment as Key](https://eprint.iacr.org/2020/061.pdf)
- [A New and Efficient Signature on Commitment Values](https://documents.uow.edu.au/~wsusilo/ZCMS_IJNS08.pdf).

#### TransactionOutput

| Field         | Abstract Type        | Data Type                             | Description                                                |
| ------------- | -------------------- | ------------------------------------- | ---------------------------------------------------------- |
| Features      | `OutputFeatures`     | See [OutputFeatures](#OutputFeatures) | Options for the output's structure or use                  |
| Commitment    | `PedersenCommitment` | `[u8;32]`                             | The homomorphic commitment representing the output amount  |
| Range Proof   | `RangeProof`         | `Vec<u8>`                             | A proof that the commitment is in the right range          |
| Script        | `TariScript`         | `Vec<u8>`                             | The script that will be executed when spending this output |
| Sender Offset | `PublicKey`          | `[u8;32]`                             | Tari script offset pubkey, K_O                             |
| Signature     | `ComSignature`       | See [ComSignature](#ComSignature)     | UTXO signature with the script offset private key, k_O     |

#### TransactionKernel

| Field            | Abstract Type        | Data Type                                 | Description                                                                                                                                                                        |
| ---------------- | -------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Features         | `KernelFeatures`     | `u8`                                      | Options for a kernel's structure or use                                                                                                                                            |
| Fee              | `MicroTari`          | `u64`                                     | Fee originally included in the transaction this proof is for.                                                                                                                      |
| Lock Height      | `u64`                | `u64`                                     | This kernel is not valid earlier than this height. The max maturity of all inputs to this transaction                                                                              |
| Excess           | `PedersenCommitment` | `[u8;32]`                                 | Remainder of the sum of all transaction commitments (minus an offset). If the transaction is well-formed, amounts plus fee will sum to zero, and the excess is a valid public key. |
| Excess Signature | `RistrettoSchnorr`   | See [RistrettoSchnorr](#RistrettoSchnorr) | An aggregated signature of the metadata in this kernel, signed by the individual excess values and the offset excess of the sender.                                                |

#### RistrettoSchnorr

| Field        | Abstract Type | Data Type | Description                               |
| ------------ | ------------- | --------- | ----------------------------------------- |
| Public nonce | `PublicKey`   | `[u8;32]` | The public nonce of the Schnorr signature |
| Signature    | `SecretKey`   | `[u8;32]` | The signature of the Schnorr signature    |

### Mining Template Header

| Field                   | Abstract Type    | Data Type | Description                                                                                                                            |
| ----------------------- | ---------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Version                 | `u16`            | `u16`     | The Tari protocol version number, used for soft/hard forks                                                                             |
| Height                  | `u64`            | `u64`     | Height of this block since the genesis block                                                                                           |
| Previous Hash           | `BlockHash`      | `[u8;32]` | Hash of the previous block in the chain                                                                                                |
| Total Kernel Offset     | `BlindingFactor` | `[u8;32]` | Sum of kernel offsets for all transaction kernels in this block                                                                        |
| Total Script Offset     | `BlindingFactor` | `[u8;32]` | Sum of script offsets for all transaction kernels in this block                                                                        |
| Proof of Work Algorithm | `PowAlgorithm`   | `u8`      | Proof of Work Algorithm used to mine this block (Monero or SHA3 )                                                                      |
| Proof of Work Data      | `Vec<u8>`        | `Vec<u8>` | Supplemental proof of work data. For Sha3 this would be empty, but for a Monero block we need the Monero header and RandomX seed hash. |
| Target Difficulty       | `Difficulty`     | `u64`     | The minimum difficulty required to satisfy the Proof of Work for the block                                                             |
| Reward                  | `MicroTari`      | `u64`     | The value of the emission for the coinbase output for the block                                                                        |
| Total Fees              | `MicroTari`      | `u64`     | The sum of all transaction fees in this block                                                                                          |

### Mining Template Body

| Field               | Abstract Type            | Data Type           | Description                                                            |
| ------------------- | ------------------------ | ------------------- | ---------------------------------------------------------------------- |
| Transaction Inputs  | `Vec<TransactionInput>`  | `TransactionInput`  | List of inputs spent                                                   |
| Transaction Outputs | `Vec<TransactionOutput>` | `TransactionOutput` | List of outputs produced                                               |
| Transaction Kernels | `Vec<TransactionKernel>` | `TransactionKernel` | Kernels contain the excesses and their signatures for the transactions |

[vector]: https://doc.rust-lang.org/rust-by-example/std/vec.html
[block header]: Glossary.md#block-header
[aggregate body]: Glossary.md#block-body
