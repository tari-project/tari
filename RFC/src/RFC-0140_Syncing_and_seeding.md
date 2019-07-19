# RFC-0140/SyncAndSeeding

## Syncing Strategies and Objectives

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [S W van Heerden](https://github.com/SWvheerden)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2018 The Tari Development Community

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

The aim of this Request for Comment (RFC) is to describe the syncing, seeding, pruning and cut-through process.

## Related Requests for Comment

* [RFC-0110: Base Nodes](RFC-0110_BaseNodes.md)

## Descriptions

### Syncing

When a new node comes online, loses connection or encounters a chain reorganization that is longer than it can tolerate, 
it must enter syncing mode. This will allow it to recover its state to the newest up-to-date state. Syncing can be 
divided into two [SynchronizationStrategy]s: complete sync and sync. Complete sync means that the node communicates with 
an archive node to get the complete history of every single block from genesis block. Sync involves the node getting 
every block from its [pruning horizon] to [current head], as well as every block header 
from genesis block. 

#### Complete Sync

Complete sync is only available from archive nodes, as these will be the only nodes that will be able to supply the 
complete history required to sync every block with every transaction from genesis block up onto [current head]. 



#### Syncing Process

The syncing process MUST be done in the following steps:

1. Set [SynchronizationState] to `Synchronizing`.
2. Ask peers for their latest block, so it can get the total Proof-of-Work (PoW). 
3. Choose the longest chain based on total PoW done on that chain.
4. Select a connected peer with the longest chain to sync from. This is based on the following criteria:
   - Does the peer have a high enough [pruning horizon]?
   - Does the peer allow syncing?
   - Does the peer have a low latency?
5. Download all headers from genesis block up onto [current head], and validate the headers as you receive them.
6. Download Unspent Transaction Output ([UTXO]) set at [pruning horizon]. 
7. Download all blocks from [pruning horizon] up to [current head]. If the node is doing a 
complete sync, the [pruning horizon] will be infinite, which means you will download all blocks ever 
created.
8. Validate blocks as if they were just mined and then received, in chronological order. 

After this process, the node will be in sync, and will be able to process blocks and transactions normally. 

#### Keeping in Sync

The node SHOULD periodically test its peers with ping messages to ensure that they are alive. When a node sends a ping 
message, it MUST include the current total PoW, hash of the [current head] and genesis block hash of its 
own current longest chain in the ping message. The receiving node MUST reply with a pong message, also including the total 
PoW, [current head] and genesis block hash of its longest chain. If the two chains do not match up, the node 
with the lowest PoW is responsible for asking the peer for syncing information and set [SynchronizationState] to `Synchronizing`. 

If the genesis block hashes do not match, the node is removed from its peer list, as this node is running a different 
blockchain. 

This will be handled by the node asking for each block header from the [current head], going backward for 
older blocks, until a known block is found. If a known block is found, and if it has missing blocks, it MUST set 
[SynchronizationState] to `Synchronizing` while it is busy catching up those blocks.

If no block is found, the node will enter sync mode and resync. It cannot recover from its state, as the fork is older 
than its [pruning horizon].

#### Chain Forks

Chain forks can be a problem, since in Mimblewimble not all nodes keep the complete transaction history. The design 
philosophy is more along the lines of only keeping the current [Blockchain state]. However, if such a 
node only maintains the current [Blockchain state], it is not possible for the node to "rewind" its 
state to handle forks in the chain history. In this case, a mode must resync its chain to recover the necessary 
transaction history up onto its [pruning horizon].

To counter this problem, we use [pruning horizon]. This allows every node ([Base Node]) to be a "light" 
[archival node](archivenode). This in effect means that the node will keep a full history for a short while. If the node 
encounters a fork, it can easily rewind its state to apply the fork. If the fork is longer than the [pruning horizon], 
the node will enter a sync state, where it will resync. 

### Pruning and Cut-through

In Mimblewimble, the state can be completely verified using the current [UTXO] set 
(which contains the output commitments and range proofs), the set of excess signatures (contained in the transaction kernels) 
and the PoW. The full block and transaction history is not required. This allows base layer nodes to remove old used 
inputs from the [blockchain] and or the [mempool]. [Cut-through] happens in the [mempool] while pruning 
happens in the [blockchain] with already confirmed transactions. This will remove the spent inputs and outputs, but will 
retain the excesses of each [transaction]. 

Pruning is only for the benefit of the local Base Node, as it reduces the local blockchain size. Pruning only happens 
after the block is older than the [pruning horizon] height. A Base Node will either run in archive mode 
or prune mode. If the Base Node is running in archive mode, it MUST NOT prune. 

When running in pruning mode, [Base Node]s MUST remove all spent outputs that are older than the 
[pruning horizon]in their current stored [UTXO] when a new block is received from another [Base Node].



[archivenode]: Glossary.md#archive-node
[blockchainstate]: Glossary.md#blockchain-state
[pruning horizon]: Glossary.md#pruning-horizon
[tari coin]: Glossary.md#tari-coin
[blockchain]: Glossary.md#blockchain
[current head]: Glossary.md#current-head
[block]: Glossary.md#block
[transaction]: Glossary.md#transaction
[base node]: Glossary.md#base-node
[utxo]: Glossary.md#unspent-transaction-outputs
[mimblewimble]: Glossary.md#mimblewimble
[mempool]: Glossary.md#mempool
[ValidationState]: Glossary.md#validationstate
[BroadcastStrategy]: Glossary.md#broadcaststrategy
[range proof]: Glossary.md#range-proof
[SynchronizationStrategy]: Glossary.md#synchronisationstrategy
[SynchronizationState]: Glossary.md#synchronisationstate
[mining server]: Glossary.md#mining-server
[cut-through]: RFC-0110_BaseNodes.md#Pruning-and-cut-through