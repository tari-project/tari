# RFC-0132/MergeMiningMonero

## Tari protocol for Merge Mining with Monero

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Stanley Bondi](https://github.com/sdbondi)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2020 The Tari Development Community

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

This document describes the specific protocol Tari uses to merge mine with Monero.

## Related Requests for Comment

* [RFC-0131: Mining](RFC-0131_Mining.md)

## Introduction

Tari employs a hybrid mining strategy, accepting 2 mining algorithms whose difficulties are independent of each other as discussed in [RFC-0131_Mining.html](/RFC-0131_Mining.html).
This RFC details a protocol to enable Tari to accept Monero proof of work, enabling participating miners a chance to produce a valid block for either or both chain without additional mining effort.

The protocol must enable a Tari base node to make the following assertions:

**REQ 1.** The achieved mining difficulty exceeds target difficulty as dictated by Tari consensus, 

**REQ 2.** The Monero block was constructed after the current Tari tip block. This is to prevent a miner from submitting blocks from the parent chain that satisfy the auxiliary chain's difficulty without doing new work.
   
It's worth noting that a Tari base node never has to contact or download data from Monero to make these assertions.

## Merge Mining on Tari

A new Tari block template is obtained from a Tari base node by calling the `get_new_block_template` gRPC method, setting `Monero` as the chosen PoW algorithm.
The `Monero` algorithm must be selected so that the correct mining difficulty for the Monero algorithm is returned. Remember, that Monero and SHA difficulties 
are independent (See [RFC-0131_Mining.html](/RFC-0131_Mining.html)). Next, a coinbase transaction is requested from a Tari Wallet for a give height by calling 
the `get_coinbase` gRPC function. 

Next, the coinbase transaction is added to the new block template and passed back to the base node for the new MMR roots to be calculated.
Furthermore, the base node constructs a _Blake256_ hash of _some_ of the Tari header fields. We'll call this hash the merge mining hash \\( h_m \\) that commits to 
the following header fields in order: `version`, `height`,`prev_hash`,`timestamp`,`output_mr`,`range_proof_mr`,`output_mmr_size`,`kernel_mr`,
`kernel_mmr_size`,`total_kernel_offset`,`total_script_offset`. Note, this hash does not include the `pow` and `nonce` fields, as these fields are set as part of mining.

To have the chance of mining a Monero block as well as a Tari block, we must obtain a new valid monero block template, by calling [get_block_template].
This returns a `blocktemplate_blob`, that is, a serialized Monero block containing the Monero block header, coinbase and a list of hashes referencing the 
transactions included in the block. Additionally, a `blockhashing_blob` is a fixed size blob containing `serialized_monero_header`, `merkle_tree_root` and 
`txn_count` concatenated together. The `merkle_tree_root` is a _merkle root_ of the coinbase + the transaction hashes contained in the block.

```rust,ignore
pub struct Block {
    /// The block header
    pub header: BlockHeader,
    /// Coinbase transaction a.k.a miner transaction
    pub miner_tx: Transaction,
    /// References to the transactions included in this block
    pub tx_hashes: Vec<hash::Hash>,
}
```
_fig 1. The Monero block struct_

Next, modify the Monero block template by including the merge mining hash \\( h_m \\) in the extra fields of the coinbase transaction. Monero has a [merge mining subfield] 
to accommodate this data. Importantly, the extra field data part of the coinbase transaction hash and therefore the `merkle_tree_root`, the `blockhashing_blob` must be 
reconstructed. A rust port of [Monero's tree hash] algorithm is needed to achieve this. The coinbase hash MUST be the first element to be hashed when constructing the `merkle_tree_root`.
This satisfies **REQ 2**, proving that the proof-of-work was performed for the Tari block.

The block may now be mined. Once a solution is found that satisfies the Tari difficulty, the miner must include enough data to allow the Tari blockchain to assert **REQ 1** and **REQ 2**.
Concretely, A miner must serialize `MoneroPowData` using Monero consensus encoding and add it to the `pow_data` field in the Tari header.

```rust,ignore
pub struct MoneroPowData {
    /// Monero header fields
    header: MoneroBlockHeader,
    /// randomX vm key
    randomx_key: FixedByteArray, // Fixed 64 bytes
    /// transaction count
    transaction_count: u16,
    /// transaction root
    transaction_root: Hash,
    /// Coinbase merkle proof hashes
    coinbase_merkle_proof: MerkleProof,
    /// Coinbase tx from Monero
    coinbase_tx: MoneroTransaction,
}
```
_fig 2. Monero PoW data struct serialized in Tari blocks_

```rust,ignore
pub struct MerkleProof {
   branch: Vec<Hash>,
   depth: u16,
   path: u32,
}
```
_fig 3. Merkle proof struct_

A verifier may now check that the `coinbase_tx` contains the merge mining hash \\( h_m \\), and validate the `coinbase_merkle_proof` against the `transaction_root`.
The `coinbase_merkle_proof` contains the minimal proof required to construct the `transaction_root`.

For example, a proof for a merkle tree of 4 hashes will require 2 hashes (h_1, h_23) of 32 bytes each, 4 bytes for the path bitmap and 2 bytes for the depth.
```text
           Root*
         /      \
       h_c1*     h_23
      /    \       
     h_c*     h_1
 
 * Not included in proof
```

## Serialisation

For Monero proof-of-work, Monero consensus encoding MUST be used to serialize the `MoneroPowData` struct. Given the same inputs, 
this encoding will byte-for-byte the same. The encoding uses VarInt for all integer types, allowing byte-savings, in particular
for fields that typically contain small values. Importantly, extra bytes that a miner _could_ tack onto the end of the `pow_data` field
are expressly disallowed.

## Merge Mining Proxy 

The [Tari merge mining proxy] proxies the [Monero daemon RPC] interface. It behaves as a middleware that implements the merge 
mining protocol detailed above. This allows existing Monero miners to merge mine with Tari without having to make changes 
to mining software. 

The proxy must be configured to connect to a `monerod` instance, a Tari base node, and a Tari console wallet. Most requests
are forwarded "as is" to `monerod`, however some are intercepted and augmented before being returned to the miner.

### `get_block_template` 

Once `monerod` has provided the block template response, the proxy retrieves a Tari block template and coinbase, 
and assembles the Tari block. The merge mining hash \\( h_m \\) is generated and added to the Monero coinbase. The modified
`blockhashing_blob` and `blocktemplate_blob` are returned to the miner. The difficulty is set to `min(monero_difficulty, tari_difficulty)`
so that the miner submits the found block at either chain's difficulty. The Tari block template is cached for later submission.
  
### `submit_block` 

The miner submits a solved Monero block (at a difficulty of `min(monero_difficulty, tari_difficulty)`) to the proxy. The cached
Tari block is retrieved, enriched with the `MoneroPowData` struct and submitted to the Tari base node.

[Monero's tree hash]: https://github.com/monero-project/monero/blob/1c8e598172bd2eddba2607cae0804db2e685813b/src/crypto/tree-hash.c
[merge mining subfield]: https://docs.rs/monero/0.13.0/monero/blockdata/transaction/enum.SubField.html#variant.MergeMining
[Tari merge mining proxy]: https://github.com/tari-project/tari/blob/development/applications/tari_merge_mining_proxy
[get_block_template]: https://ww.getmonero.org/resources/developer-guides/daemon-rpc.html#get_block_template
[bincode]: https://docs.rs/bincode/1.3.3/bincode/
[Monero daemon RPC]: https://www.getmonero.org/resources/developer-guides/daemon-rpc.html
