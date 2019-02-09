# RFC-0110/BaseNodes

## Base Layer Full Nodes (Base Nodes)

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77) [SW van heerden](https://github.com/SWvheerden)

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

This document describes the roles that [base node]s play in the Tari network and their general approach for doing so.

## Related RFCs

* [RFC-0100: Base layer](RFC-0100_BaseLayer.md)

## Description

### Broad requirements

Tari Base Nodes form a peer-to-peer network for a proof-of-work based blockchain running the [Mimblewimble]
protocol. The proof of work is performed via merge mining with Monero. Arguments for this design are
presented [in the overview](RFC-0001_overview.md#proof-of-work).

Tari Base Nodes MUST carry out the following tasks:

* Validate all [Tari coin] [transaction]s.
* Propagate valid transactions to peer nodes.
* Validate all new [block]s received.
* Propagate validated new blocks to peer nodes.
* Connect to peer nodes to catch up (sync) its blockchain state.
* Provide historical block information to peers that are syncing.

Once the Digital Assets Network goes live, Base Nodes will also need to support the tasks described in
[RFC-0300/Assets](RFC-0300_DAN.md). These requirements are omitted for the moment.

To carry out these tasks effectively, Base Nodes SHOULD:

* Save the [blockchain] into a indexed local database.
* Maintain an index of all unspent outputs ([UTXO]s).
* Maintain a list of all pending, valid transactions that have not yet been mined (the [mempool]).
* Manage a list of Base Node peers present on the network.

Tari Base nodes MAY implement chain pruning strategies that are features of Mimblewimble, including transaction
[cut-through and block compaction techniques](https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#mimblewimble-protocol-overview).

Tari Base Nodes MAY also implement the following services via an API to clients. Such clients may include "light"
clients, block explorers, wallets, and Tari applications:

  * Block queries.
  * Kernel data queries.
  * Transaction queries.
  * Submission of new transactions.

### Transaction validation and propagation

Base nodes can be notified of new transactions by
* connected peers.
* clients via APIs.

When a new transaction has been received, it has the `unvalidated` [ValidationState]. The transaction is then passed
to the transaction validation service, where its state will become either `rejected`, `timelocked` or `validated`.

The transaction validation service checks that:

* all inputs to the transaction are valid [UTXO]s.
* no inputs are duplicated.
* all inputs are able to be spent (they're not time-locked).
* all inputs are signed by their owners.
* all outputs have valid [range proof]s.
* no outputs currently exist in the [UTXO] set.
* the transaction does not have a timelock applied, limiting it from being added to the blockchain before a specified block height or timestamp has been reached.
* the transaction excess has a valid signature.
* the transaction excess is a valid public key. This proves that:
  $$ \Sigma \left( \mathrm{inputs} - \mathrm{outputs} - \mathrm{fees} \right) = 0 $$

`Rejected` transactions are dropped silently.

`Timelocked` transactions are:
* marked with a timelocked status and gets added to the [mempool].
* will be evaluated again at a later state to determine if the timelock has passed and if it can be upgraded to 'Validated' status.

`Validated` transactions are:
* Added to the [mempool].
* forwarded to peers using the transaction [BroadcastStrategy].

### Block validation and propagation

The Block validation and propagation process is analogous to that of transactions.

New blocks are received from the peer-to-peer network, or from an API call if the Base Node is connected to a Miner.

When a new block is received, it is assigned the `unvalidated` [ValidationState]. The block is then passed to the
block validation service.  The validation service checks that

* the block hasn't been processed before.
* every [transaction] in the block is valid.
* the proof-of-work is valid.
* the block header is well-formed.
* the block is being added to the chain with the highest accumulated proof-of-work.
  * it is possible for the chain to temporarily fork; base nodes SHOULD account for forks up to some configured depth.
  * it is possible that blocks may be received out of order; particularly while syncing. Base Nodes SHOULD keep blocks.
    that have block heights greater than the current chain tip in memory for some preconfigured period.
* the sum of all excesses is a valid public key. This proves that:
   $$ \Sigma \left( \mathrm{inputs} - \mathrm{outputs} - \mathrm{fees} \right) = 0$$ 
* check if [cut-through] was applied. If a block contains already spent outputs, reject that block.

Because MimbleWimble blocks can be simple be seen as large transactions with multiple inputs and outputs, the block validation service checks all transaction verification on the block as well.

`Rejected` blocks are dropped silently.

Base Nodes are not obliged to accept connections from any peer node on the network. In particular:

* Base Nodes MAY refuse connections from peers that have been added to a blacklist.
* Base Nodes MAY be configured to exclusively connect to a given set of peer nodes.

`Validated` blocks are
* added to the [blockchain].
* forwarded to peers using the block [BroadcastStrategy].

In addition, when a block has been validated and added to the blockchain:
* The mempool MUST also remove all transactions that are present in the newly validated block.
* The UTXO set MUST be updated; removing all inputs in the block, and adding all the new outputs in it.

### Synchronising and pruning of the chain

Syncing, pruning and cut-through is discussed in detail in [RFC-0140](RFC-0140_Syncing.md)

### Archival nodes

[Archival nodes](archivenode) are used to keep a complete history of the blockchain since genesis block, they do not employ pruning at all. These nodes will allow full syncing of the blockchain because normal nodes will not keep the full history to enable this. 



[archivenode]: Glossary.md#archivenode

[tari coin]: Glossary.md#tari-coin
[blockchain]: Glossary.md#blockchain
[transaction]: Glossary.md#transaction
[block]: Glossary.md#block
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
[cut-through]: RFC-0140_Syncing.md#Pruning-and-cut-through
