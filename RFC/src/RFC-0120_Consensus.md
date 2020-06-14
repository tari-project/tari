# RFC-0120/Consensus

## Base Layer Full Nodes (Base Nodes)

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77) and [S W van heerden](https://github.com/SWvheerden)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019 The Tari Development Community

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

The aim of this Request for Comment (RFC) is to describe the fields that a block should contain as well as all consensus rules that will determine the validity of a block.

## Related Requests for Comment

* [RFC-0100: Base Layer](RFC-0100_BaseLayer.md)
* [RFC-0130: Mining](./RFC-0130_Mining.md)
* [RFC-0140: SyncAndSeeding](RFC-0140_Syncing_and_seeding.md)

## Description

### Blocks

Every [block] MUST conform to the following:

* Have a _single_ valid coinbase [UTXO] and kernel
* have a _single_ valid blockheader;
* [cut-through] MUST have been applied where possible;
* every [UTXO] has a valid [range proof].

If a [block] does not confirm to the above, the block should be rejected as invalid and the peer from which it was received marked as a malicious peer.

### Blockheaders

Every [block header] MUST contain the following fields:

* version;
* height;
* prev_hash;
* timestamp;
* output_mr;
* range_proof_mr;
* kernel_mr;
* total_kernel_offset;
* nonce;
* pow.

The [block header] MUST confirm to the following:

* The nonce and [PoW](#pow) must be valid for the [block header].
* All the merkle roots must be valid for the states _after_ the [block] was applied to the local state.

If the [block header] does not confirm to any of the above, the [block] MUST be rejected.
If a peer rejects multiple blocks from a given peer, it MAY denylist the peer and ignore further communication from it.

#### Version

This is the version currently running on the chain.

The version MUST confirm to the following:

* It is represented as an unsigned 16-bit integer.
* Version numbers MUST be incremented whenever there is a change in the blockchain schema starting from 1.

#### Height

A counter indicating how many blocks have passed since the genesis block (inclusive).

The height MUST confirm to the following:

* Represented as an unsigned 64-bit integer.
* The height MUST be exactly 1 more than the block referenced in the `prev_hash` block header field.
* The genesis block MUST have a height of 0.

#### Prev_hash

This is the hash of the previous block's header.

The prev_hash MUST confirm to the following:

* represented as an array of unsigned 8-bit integers (bytes) in little-endian format.
* MUST be a hash of the entire contents of the previous block's header.

#### Timestamp

This is the timestamp at which the block was mined.

The timestamp MUST confirm to the following:

* Must be transmitted as UNIX timestamp.
* MUST be less than [FTL].
* MUST be higher than the [MTP].

#### Output_mr

This is the merkle root of the outputs. This is calculated in the following way: Hash (txo MMR root  || roaring bitmap hash of UTXO indices).

The output_mr MUST confirm to the following:

* Represented as an array of unsigned 8-bit integers (bytes) in little-endian format.
* The hashing function used MUST be blake2b with a 256 bit digest.

#### Range_proof_mr

This is the merkle root of the range proofs.

The range_proof_mr MUST confirm to the following:

* Represented as an array of unsigned 8-bit integers (bytes) in little-endian format.
* The hashing function used must be blake2b with a 256 bit digest.

#### Kernel_mr

This is the merkle root of the outputs.

The kernel_mr MUST confirm to the following:.

* Must be transmitted as an array of unsigned 8-bit integers (bytes) in little-endian format.
* The hashing function used must be blake2b with a 256 bit digest.

#### Total_kernel_offset

This is total summed offset of all the transactions contained in this block.

The total_kernel_offset MUST confirm to the following:

* Must be transmitted as an array of unsigned 8-bit integers (bytes) in little-endian format

#### Total_difficulty

This is the total accumulated difficulty of the mined chained.

The total_difficulty MUST confirm to the following:

* Must be transmitted as unsigned 64-bit integer.
* MUST be larger than the previous block's `total_difficulty`.
* meet the difficulty target for the block as determined by the consensus difficulty algorithm.

#### Nonce

This is the nonce used in solving the Proof of Work.

The nonce MUST confirm to the following:

* Must be transmitted as unsigned 64-bit integer;

#### PoW

This is Proof of Work algorithm that was used to solve the Proof of Work. This is used in conjunction with the Nonce

The [PoW] MUST contain the following:

* accumulated_monero_difficulty as unsigned 64-bit integer.
* accumulated_blake_difficulty as unsigned 64-bit integer.
* pow_algo as an enum (0 for monero, 1 for blake).
* pow_data as array of unsigned 8-bit integers (bytes) in little-endian format.

### FTL

The Future Time Limit. This is how far into the future a time is accepted as a valid time. Any time that is more than the FTL is rejected until such a time that it is not less than the FTL.
The FTL is calculated as (T*N)/20 with T and N defined as:
T: Target time - This is the ideal time that should pass between blocks which have been mined.
N: Block window - This is the amount of blocks used when calculating difficulty adjustments.

### MTP

The Median Time Past. This is the lower limit of a time. Any time that is less than the MTP is rejected.
THe MTP is calculated as the median timestamp of the previous 11 blocks.

### Total accumulated proof of work

This is defined as the total accumulated proof of work done on a single block chain. Because we use two proof of work algorithms which are rated at different difficulties and we would like to weight them as equal we need to compare them. To compare them we use a [Geometric mean](https://en.wikipedia.org/wiki/Geometric_mean). Because we only have two algorithms this is calculated as the ceil of SQRT(accumulated_monero_difficulty*accumulated_blake_difficulty). This number is never transmitted and is simply calculated from the tip of the chain.


[block]: Glossary.md#block
[block header]: Glossary.md#block-header
[utxo]: Glossary.md#unspent-transaction-outputs
[range proof]: Glossary.md#range-proof
[cut-through]: RFC-0140_Syncing_and_seeding.md#pruning-and-cut-through
[FTL]: RFC-0120_Consensus.md#FTL
[MTP]: RFC-0120_Consensus.md#MTP
