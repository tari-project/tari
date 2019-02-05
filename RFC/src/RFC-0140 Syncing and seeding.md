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

When a new node comes online, loses connection or encounters a chain re-organisation that is longer than it can tolerate, it must enter syncing mode. This will allow it to recover its state to the newest up to date state. Syncing can be divided into 2 [SynchronisationStrategy]s, complete sync and sync. Complete sync will involve that the node communicates with an archive node to get the complete history of every single block from genesis block. Sync will involve the node getting every block from its [pruning horizon](pruninghorizon) to current head, as well as every block header from genesis block. 

#### Complete Sync

Complete sync is only available from archive nodes, as these will be the only nodes that will be able to supply the complet history required to sync every block with every transaction from genesis block up onto [current head](currenthead). 



#### Syncing process

The syncing process is done in the following steps:

1. Set its [SynchronisationState] to `Synchronising`.
2. Load a bootstrap list of peers from a configuration file, or a cached list.
3. For each peer in this list:
   1. Establish a connection with the peer.
   2. Request a peer list from that peer.
   3. Request information about the most recent chain state (total accumulated work, block height, etc.) from the peer.
4. Choose the longest chain based on pow. 
5. Download all headers from genesis block up onto [current head](currenthead), and validate the headers as you receive them.
6. Download all blocks from  [pruning horizon](pruninghorizon) up onto [current head](currenthead), if the node is doing a complete sync, the pruninghorizon will just be infinite, which means you will download all blocks ever created.
7. Validate blocks as if they where just mined and then received, in chronological order. 

After this process the node will be in sync and able to process blocks and transaction normally. 

#### Keeping in sync

The node should periodically test its peers with ping messages to ensure that they are alive. When a node sends a ping message, it should enclude the current total pow, hash of the [current head](currenthead) and genesis block hash of its own current longest chain in the ping message. The receiving node will replay with a pong message also including the total pow, [current head](currenthead) and genesis block hash of its longest chain. If the two chains dont match up, the node with the lowest pow has the responsiblity to ask the peer for syncing information. 

If the genesis block hash's dont match the node is removed from its peer list as this node is running a different block chain, we can remove it. 

This will be hanled by the node asking for each block from the [current head](currenthead) going backword untill a known block is found. If no block is found, the node will enter sync mode and resync. It cannot recover from its state as the fork is older than its [pruning horizon](pruninghorizon).

#### Chain forks

Chain forks can be a problem since in mimblewimble not all nodes keep the complete history, the design philosophy  is more in the lines of only keeping the current [utxo]. Only keepinng the current [utxo] will not be possible, because if you encounter a fork where there are two running version op the blockchain, you will not be able to swop without doing sync again. 

To counter this problem we use  [pruning horizon](pruninghorizon), this allows every [node](base node) to be a "light" [archival node](archivenode). This in effect means that the node will keep a full history for a short while. If the node encounters a fork it can easily rewind its state to apply the fork. If the fork is longer than the [pruning horizon](pruninghorizon), the node will enter a sync state where it will resync. 

### Pruning and cut-through

[Pruning and cut-through]: #Pruning-and-cut-through	"Remove already spent outputs from the [utxo]"

In MimbleWimble, the state can be completely verified using the current [UTXO](utxo) set, the set of excess signatures (contained in the transaction kernels) and the proof-of-work. The full block and transaction history is not required. This allows base layer nodes to remove old used inputs from the [blockchain] and or the [mempool]. [Cut-through](cut-through) happens in the [mempool] while pruning happens in the [blockchain] with already confirmed transactions. This will remove the inputs and outputs, but will retain the excesses  of each [transaction]. 

Pruning is only for the benefit of the local base node as it reduces the local blockchain size. Pruning only happens afterthe [pruning horizon](pruninghorizon) height. A Base node will either run in archive mode or prune mode, if the base node is running in archive mode it should not prune. 

When running in pruning mode, [base node]s have the following responsibilities:

1. MUST remove all spent outputs thats older than the [pruning horizon](pruninghorizon) in it's current stored [UTXO](utxo) when a new block is received from another [base node].



[archivenode]: Glossary.md#archivenode
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
