# RFC-0110/BaseNodes

## Base Layer Full Nodes (Base Nodes)

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77), [S W van heerden](https://github.com/SWvheerden) and  [Stanley Bondi](https://github.com/sdbondi)

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

The aim of this Request for Comment (RFC) is to describe the roles that [base node]s play in the Tari network as well as 
their general approach for doing so.

## Related Requests for Comment

* [RFC-0100: Base Layer](RFC-0100_BaseLayer.md)
* [RFC-0140: SyncAndSeeding](RFC-0140_Syncing_and_seeding.md)

$$
\newcommand{\so}{\gamma} % script offset
$$

## Description

### Broad Requirements

Tari Base Nodes form a peer-to-peer network for a proof-of-work based blockchain running the [Mimblewimble]
protocol. The proof-of-work is performed via hybrid mining, that is merge mining with Monero and stand-alone SHA 3. 
Arguments for this design are presented [in the overview](RFC-0001_overview.md#proof-of-work).

Tari Base Nodes MUST carry out the following tasks:

* validate all [Tari coin] transactions;
* propagate valid [transaction]s to peer nodes;
* validate all new [block]s received;
* propagate validated new blocks to peer nodes;
* connect to peer nodes to catch up (sync) with their blockchain state;
* provide historical block information to peers that are syncing.

Once the Digital Assets Network (DAN) goes live, Base Nodes will also need to support the tasks described in
[RFC-0300_DAN](RFCD-0300_DAN.md). These requirements are omitted for the moment.

To carry out these tasks effectively, Base Nodes SHOULD:

* save the [blockchain] into an indexed local database;
* maintain an index of all Unspent Transaction Outputs ([UTXO]s);
* maintain a list of all pending, valid transactions that have not yet been mined (the [mempool]);
* manage a list of Base Node peers present on the network.

Tari Base Nodes MAY implement chain pruning strategies that are features of Mimblewimble, including transaction
[cut-through and block compaction techniques](https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#mimblewimble-protocol-overview).

Tari Base Nodes MAY also implement the following services via an Application Programming Interface (API) to clients:

- Block queries
- Kernel data queries
- Transaction queries
- Submission of new transactions

 Such clients may include "light" clients, block explorers, wallets and Tari applications.

### Transaction Validation and Propagation

Base nodes can be notified of new transactions by:
* connected peers;
* clients via APIs.

When a new transaction has been received, it is then passed to the mempool service where it will be 
validated and either stored or rejected.

The transaction is validated as follows:

* All inputs to the transaction are valid [UTXO]s in the [UTXO] set or are outputs in the current block.
* No inputs are duplicated.
* All inputs are able to be spent (they are not time-locked).
* All inputs are signed by their owners.
* All outputs have valid [range proof]s.
* No outputs currently exist in the current [UTXO] set.
* The transaction does not have [timelocks] applied, limiting it from being mined and added to the blockchain before a
  specified block height or timestamp has been reached.
* The transaction excess has a valid signature.
* The [transaction weight] does not exceed the maximum permitted in a single block as defined by consensus.
* The transaction excess is a valid public key. This proves that:
  $$ \Sigma \left( \mathrm{inputs} - \mathrm{outputs} - \mathrm{fees} \right) = 0 $$.
* The transaction excess has a unique value across the whole chain.
* The [Tari script] of each input must execute successfully and return the public key that signs the script signature. 
* The script offset \\( \so\\) is calculated and verified as per [RFC-0201_TariScript].

Rejected transactions are dropped silently.

Timelocked transactions are rejected by the mempool. The onus is on the client to submit transactions once they are 
able to be spent.

**Note:** More detailed information is available in the [timelocks] RFC document.

Valid transactions are:

* added to the [mempool];
* forwarded to peers using the transaction [BroadcastStrategy].

#### Block/Transaction Weight 
[block-transaction weight]: #blocktransaction-weight "Block/Transaction Weight"

The weight of a transaction / block measured in "grams". Input, output and kernel weights reflect their respective relative 
storage and computation cost. Transaction fees are typically proportional to a transaction body's total weight, creating 
incentive to reduce the size of the UTXO set.

Given the target block size of `S` and the choice for 1 gram to represent `N` bytes, we end up with
a maximum block weight of `S/N` grams. 

To illustrate (these values should not be considered authoritative), with an `S` of 1MiB and `N` of 16, the block and 
transaction body weights are as follows:


|                   	| Byte size 	| Natural Weight         	| Adjust 	| Final                  	|
|-------------------	|-----------	|------------------------	|--------	|------------------------	|
| Output            	|           	|                        	|        	|                        	|
| - Per output      	| 832       	| 52                     	| 0      	| 52                     	|
| - Tari Script     	| variable  	| size_of(script) / 16   	| 0      	| size_of(script) / 16   	|
| - Output Features 	| variable  	| size_of(features) / 16 	| 0      	| size_of(features) / 16 	|
| Input             	|       169 	|                     11 	|     -2 	|                      9 	|
| Kernel size       	|       113 	|                      8 	|      2 	|                     10 	|

Pseudocode: 

```text
    output_weight = num_outputs * PER_OUTPUT_GRAMS(53)
    foreach output in outputs:
        output_weight += serialize(output.script) / BYTES_PER_GRAM
        output_weight += serialize(output.features) / BYTES_PER_GRAM
        
    input_weight = num_inputs * PER_INPUT_GRAMS(9)
    kernel_weight = num_kernels * PER_KERNEL_GRAMS(10)
    
    weight = output_weight + input_weight + kernel_weight
```

where the capitalized values are hard-coded constants.

### Block Validation and Propagation

The block validation and propagation process is analogous to that of transactions. New blocks are received from the 
peer-to-peer network, or from an API call if the Base Node is connected to a Miner.

When a new block is received, it is passed to the block validation service. The validation service checks that:

* The block has not been processed before.
* Every [transaction] in the block is valid.
* The proof-of-work is valid.
* The block header is well-formed.
* The block is being added to the chain with the highest accumulated proof-of-work.
  * It is possible for the chain to temporarily fork; Base Nodes SHOULD store orphaned forks up to some configured depth.
  * It is possible that blocks may be received out of order. Base Nodes SHOULD keep blocks that have block heights 
    greater than the current chain tip for some preconfigured period.
* The sum of all excesses is a valid public key. This proves that:
   $$ \Sigma \left( \mathrm{inputs} - \mathrm{outputs} - \mathrm{fees} \right) = 0$$. 
* That all kernel excess values are unique for that block and the entire chain.
* Check if a block contains already spent outputs, reject that block.
* The [Tari script] of every input must execute successfully and return the public key that signs the script signature.
* The script offset \\( \so\\) is calculated and verified as per [RFC-0201_TariScript]. This prevents [cut-through] from 
  being applied.


Because Mimblewimble blocks can simply be seen as large transactions with multiple inputs and outputs, the block 
validation service checks all transaction verification on the block as well.

Rejected blocks are dropped silently.

Base Nodes are not obliged to accept connections from any peer node on the network. In particular:

* Base Nodes MAY refuse connections from peers that have been added to a denylist.
* Base Nodes MAY be configured to exclusively connect to a given set of peer nodes.

Validated blocks are
* added to the [blockchain];
* forwarded to peers using the block [BroadcastStrategy].

In addition, when a block has been validated and added to the blockchain:
* The mempool MUST also remove all transactions that are present in the newly validated block.
* The UTXO set MUST be updated by removing all inputs in the block, and adding all the new outputs into it.

### Synchronizing and Pruning of the Chain

Syncing, pruning and cut-through are discussed in detail in [RFC-0140](RFC-0140_Syncing_and_seeding.md).

### Archival Nodes

[Archival nodes] are used to keep a complete history of the blockchain since genesis block. They do not employ pruning
at all. These nodes will allow full syncing of the blockchain, because normal nodes will not keep the full history to
enable this. These nodes must sync from another archival node.

### Pruned Nodes

[Pruned nodes] take advantage of the cryptography of [mimblewimble] to allow them to prune spent inputs and outputs 
beyond the pruning horizon and still validate the integrity of the blockchain i.e. no coins were destroyed or created 
beyond what is allowed by consensus rules. A sufficient number of blocks back from the tip should be configured because 
reorgs are no longer possible beyond that horizon. These nodes can sync from any other base node (archival and pruned).



[archival nodes]: Glossary.md#archive-node
[tari coin]: Glossary.md#tari-coin
[blockchain]: Glossary.md#blockchain
[transaction]: Glossary.md#transaction
[block]: Glossary.md#block
[base node]: Glossary.md#base-node
[utxo]: Glossary.md#unspent-transaction-outputs
[mimblewimble]: Glossary.md#mimblewimble
[mempool]: Glossary.md#mempool
[BroadcastStrategy]: Glossary.md#broadcaststrategy
[range proof]: Glossary.md#range-proof
[SynchronisationStrategy]: Glossary.md#synchronisationstrategy
[SynchronisationState]: Glossary.md#synchronisationstate
[mining server]: Glossary.md#mining-server
[cut-through]: Glossary.md#cut-through
[timelocks]: RFC-0230_HTLC.md#time-locked-contracts
[transaction weight]: ./Glossary.md#transaction-weight
[Tari script]: ./RFC-0201_TariScript.md
[RFC-0201_TariScript]: ./RFC-0201_TariScript.md
