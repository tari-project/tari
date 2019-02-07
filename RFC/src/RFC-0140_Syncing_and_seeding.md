# RFC-0140/SyncAndSeeding

## Syncing strategies and objectives

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [SW van heerden](https://github.com/SWvheerden)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2018 The Tari Development Community

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", 
"NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in 
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as 
shown here.

## Disclaimer

The purpose of this document and its content is for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

This document describes the process of Syncing, seeding, pruning and cut-through.

## Related RFCs

* [RFC-0110: Base Nodes](RFC-0110_BaseNodes.md)

## Descriptions

### Syncing

When a new node comes online, loses connection or encounters a chain re-organisation that is longer than it can tolerate, it must enter syncing mode. This will allow it to recover its state to the newest up to date state. Syncing can be divided into 2 [SynchronisationStrategy]s, complete sync and sync. Complete sync will mean that the node communicates with an archive node to get the complete history of every single block from genesis block. Sync will involve the node getting every block from its [pruning horizon](pruninghorizon) to [current head](current head), as well as every block header from genesis block. 

#### Complete Sync

Complete sync is only available from archive nodes, as these will be the only nodes that will be able to supply the complete history required to sync every block with every transaction from genesis block up onto [current head](currenthead). 



#### Syncing process

The syncing process MUST be done in the following steps:

1. Set [SynchronisationState] to `Synchronising`.
2. Asks peers for their latest block, so it can get the total proof of work. 
3. Choose the longest chain based on total PoW done on that chain.
4. Selects a connected peer with the logest chain to sync from, this is based on the following criteria teria:
   1. Does the peer have a high enough [pruning horizon](pruninghorizon).
   2. Does the peer allow syncing.
   3. Does the peer have a low latency.
5. Download all headers from genesis block up onto [current head](currenthead), and validate the headers as you receive them.
6. Download [UTXO](utxo) set at [pruning horizon](pruninghorizon). 
7. Download all blocks from  [pruning horizon](pruninghorizon) up to [current head](currenthead), if the node is doing a complete sync, the [pruning horizon](pruninghorizon) will just be infinite, which means you will download all blocks ever created.
8. Validate blocks as if they where just mined and then received, in chronological order. 

After this process the node will be in sync and able to process blocks and transaction normally. 

#### Keeping in sync

The node SHOULD periodically test its peers with ping messages to ensure that they are alive. When a node sends a ping message, it MUST include the current total PoW, hash of the [current head](currenthead) and genesis block hash of its own current longest chain in the ping message. The receiving node MUST reply with a pong message also including the total PoW, [current head](currenthead) and genesis block hash of its longest chain. If the two chains don't match up, the node with the lowest PoW has the responsibility to ask the peer for syncing information and set [SynchronisationState] to `Synchronising`. 

If the genesis block hashes don't match, the node is removed from its peer list as this node is running a different blockchain. 

This will be handled by the node asking for each block header from the [current head](currenthead) going backward for older blocks until a known block is found. If a known block is found, and it has missing blocks it MUST set [SynchronisationState] to `Synchronising` while it is busy catching up those blocks.

If no block is found, the node will enter sync mode and resync. It cannot recover from its state as the fork is older than its [pruning horizon](pruninghorizon).

#### Chain forks

Chain forks can be a problem since in Mimblewimble not all nodes keep the complete transaction history, the design philosophy is more along the lines of only keeping the current [Blockchain state](blockchainstate). However, if such a node only maintains only the current [Blockchain state](blockchainstate) it is not possible for the node to "rewind" its state to handle forks in the chain history. In this case, a mode must re-sync its chain to recover the necessary transaction history up onto its [pruning horizon](pruninghorizon).

To counter this problem we use  [pruning horizon](pruninghorizon), this allows every [node](base node) to be a "light" [archival node](archivenode). This in effect means that the node will keep a full history for a short while. If the node encounters a fork it can easily rewind its state to apply the fork. If the fork is longer than the [pruning horizon](pruninghorizon), the node will enter a sync state where it will resync. 

### Pruning and cut-through

[Pruning and cut-through]: #Pruning-and-cut-through	"Remove already spent outputs from the [utxo]"

In Mimblewimble, the state can be completely verified using the current [UTXO](utxo) set (which contains the output commitments and range proofs), the set of excess signatures (contained in the transaction kernels) and the proof-of-work. The full block and transaction history is not required. This allows base layer nodes to remove old used inputs from the [blockchain] and or the [mempool]. [Cut-through](cut-through) happens in the [mempool] while pruning happens in the [blockchain] with already confirmed transactions. This will remove the spent inputs and outputs, but will retain the excesses of each [transaction]. 

Pruning is only for the benefit of the local base node as it reduces the local blockchain size. Pruning only happens after the block is older than the [pruning horizon](pruninghorizon) height. A Base node will either run in archive mode or prune mode, if the base node is running in archive mode it MUST NOT prune. 

When running in pruning mode, [base node]s have the following responsibilities:

1. MUST remove all spent outputs that is older than the [pruning horizon](pruninghorizon) in it's current stored [UTXO](utxo) when a new block is received from another [base node].



[archivenode]: Glossary.md#archivenode
[blockchainstate]: Glossary.md#blockchainstate
[pruninghorizon]: Glossary.md#pruninghorizon
[tari coin]: Glossary.md#tari-coin
[blockchain]: Glossary.md#blockchain
[currenthead]: Glossary.md#currenthead
[block]: Glossary.md#block
[transaction]: Glossary.md#transaction
[base node]: Glossary.md#base-node
[utxo]: Glossary.md#unspent-transaction-outputs
[mimblewimble]: Glossary.md#mimblewimble
[mempool]: Glossary.md#mempool
[ValidationState]: Glossary.md#validationstate
[BroadcastStrategy]: Glossary.md#broadcaststrategy
[range proof]: Glossary.md#range-proof
[SynchronisationStrategy]: Glossary.md#synchronisationstrategy
[SynchronisationState]: Glossary.md#synchronisationstate
[mining server]: Glossary.md#mining-server
[cut-through]: RFC-0110_BaseNodes.md#Pruning-and-cut-through