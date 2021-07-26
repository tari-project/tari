# RFC-0140/SyncAndSeeding

## Syncing Strategies and Objectives

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [S W van Heerden](https://github.com/SWvheerden), [Philip Robinson](https://github.com/philipr-za)

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

The aim of this Request for Comment (RFC) is to describe the syncing, seeding and pruning process.

## Related Requests for Comment

* [RFC-0110: Base Nodes](RFC-0110_BaseNodes.md)

## Descriptions

### Syncing

When a new node comes online, loses connection or encounters a chain reorganization that is longer than it can tolerate, 
it must enter syncing mode. This will allow it to recover its state to the newest up-to-date state. Syncing can be 
divided into two [SynchronizationStrategy]s: complete sync and horizon sync. Complete sync means that the node 
communicates with an archive node to get the complete history of every single block from genesis block. Horizon Sync 
involves the node getting every block from its [pruning horizon] to [current head], as well as every block header 
up to the genesis block. 

To determine if the node needs to synchronise the node will monitor the broadcast chain_metadata provided by its neighbours.

#### Complete Sync

Complete sync is only available from archival nodes, as these will be the only nodes that will be able to supply the 
complete history required to sync every block with every transaction from genesis block up onto [current head]. 

#### Complete Sync Process

Once the base node has determined that it is lagging behind the network tip it will start to synchronise with the peer
it determines to have all the data required to synchronise.

The syncing process MUST be done in the following steps:

1. Set [SynchronizationState] to `header_sync`.
2. Sync all missing headers from the genesis block to the current chain tip. The initial header sync allows the node to
   confirm that the syncing peer does indeed have a fully intact chain from which to sync that adheres to this node's
   consensus rules and has a valid proof-of-work that is higher than any competing chains.
3. Set [SynchronizationState] to `block_sync`.
4. Start downloading blocks from sync peer starting with the oldest block in our database. A fresh node will start from 
   the genesis block.
5. Download all block up to [current head], validating and adding the blocks to the local chain storage as we go.
6. Once all blocks have been downloaded up and including the current network tip set the [SynchronizationState] to 
   `listening`.

After this process, the node will be in sync, and will be able to process blocks and transactions normally as they 
arrive.  

#### Horizon Sync Process

The horizon sync process MUST be done in the following steps:

1. Set [SynchronizationState] to `header_sync`.
2. Sync all missing headers from the genesis block to the current chain tip. The initial header sync allows the node to
   confirm that the syncing peer does indeed have a fully intact chain from which to sync that adheres to this nodes
   consensus rules and has a valid proof-of-work that is higher than any competing chains.
3. Set [SynchronizationState] to `horizon_sync`.
4. Download all kernels from the current network tip back to this node's [pruning horizon].
5. Validate kernel MMR root against headers.
6. Download all [utxo]'s from the current network tip back to this node's [pruning horizon].
7. Validate outputs and [utxo] MMR.
8. Validate the chain balances with the expect total emission that the final sync height.  
9. Once all kernels and [utxo]s have been downloaded from the network tip back to this node's [pruning horizon] set 
   the [SynchronizationState] to `block_sync`. This hands over further syncing to the standard sync protocol which 
   should return to the `listening` state if no further data has been received from peers.

After this process, the node will be in sync, and will be able to process blocks and transactions normally as they
arrive.

#### Keeping in Sync

The node that is in the `listening` state SHOULD periodically test a subset of its peers with ping messages to ensure 
that they are alive. When a node sends a ping message, it MUST include the height of the current longest chain, current 
accumulated PoW difficulty, hash of the [current head], it's pruning horizon and it's current pruned height. The 
receiving node MUST reply with a pong message, which should include it's version of the information contained within the
ping message.

When a node receives pong replies from the current ping round, or the timeout expires, the collected chain_metadata
replies will be examined to determine what the current best chain is, i.e. the chain with the most accumulated work.
If the best chain is longer than out chain data the node will set [SynchronizationState] to `header_sync` and catch up
with the network.

#### Chain Forks

Chain forks occur in all decentralized proof-of-work blockchains. When the local node is in the `listening` state it 
will detect that it has fallen behind other nodes in the network. It will then perform a header sync and during the 
header sync process will be able to detect that a chain fork has occurred. The header sync process will then determine
which chain is the correct chain with the highest accumulated work. If required this node will switch the best chain
and proceed to sync the new blocks required to catch up to the correct chain. This process is called a chain 
reorganization or [reorg]. 

### Pruning 

In Mimblewimble, the state can be completely verified using the current [UTXO] set (which contains the output 
commitments and range proofs), the set of excess signatures (contained in the transaction kernels) and the PoW. The full
block and transaction history is not required. This allows base layer nodes to remove old spent inputs from the 
[blockchain] storage. 

Pruning is only for the benefit of the local Base Node, as it reduces the local blockchain size. Pruning only happens 
after the block is older than the [pruning horizon] height. A Base Node will either run in archival mode or pruned mode.
If the Base Node is running in archive mode, it MUST NOT prune. 

When running in pruning mode, [Base Node]s MUST remove all spent outputs that are older than the 
[pruning horizon] in their current stored [UTXO] set when a new block is received from another [Base Node].


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
[BroadcastStrategy]: Glossary.md#broadcaststrategy
[range proof]: Glossary.md#range-proof
[reorg]: Glossary.md#chain-reorg
[SynchronizationStrategy]: Glossary.md#synchronisationstrategy
[SynchronizationState]: Glossary.md#synchronisationstate
[mining server]: Glossary.md#mining-server
