# RFC-0130: Mining

## Full-node mining on Tari base layer

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Yuko Roodt] (https://github.com/neonknight64)

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

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in [RFC 2119](http://tools.ietf.org/html/rfc2119).

## Disclaimer

The purpose of this document and its content is for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in theprocess of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

This document will provide a brief overview of the Tari merged mining process and will introduce the primary functionality required of the Mining Server and Mining Worker.

## Related RFCs

* [RFC-0100: Base layer](RFC-0100_BaseLayer.md)
* [RFC-0110: Base nodes](RFC-0110_BaseNodes.md)

## Description

### Assumptions
- That the Tari [blockchain] will be merged mined with Monero.
- The Tari [Base Layer] has a network of [Base Node]s that verify and propagate valid [transaction]s and [block]s. 

### Abstract

The process of merged mining Tari with Monero on the Tari Base Layer is performed by Mining Servers and Mining Workers. Mining Servers are responsible for constructing new blocks by bundling transactions from the [mempool] of a connected Base Node. They then distribute Proof-of-Work(PoW) tasks to Mining Workers in an attempt to solve newly created blocks. Solved solutions and shares are sent by the Mining Workers to the Mining Server, who in turns verifies the solution and distributes the newly created blocks to the Base Node and Monero Node for inclusion in their respective blockchains.

### Merged mining on the Tari Base Layer

This document is divided into three parts. First, a brief overview of the merged mining process and the interactions between the Base Node, Mining Server and Mining Worker will be provided, then the primary functionality required of the Mining Server and Mining Worker will be proposed.

####  Overview of the Tari merged mining process using Mining Servers and Mining Workers

Mining on the Tari Base Layer consists of three primary entities: the Base Nodes, Mining Servers and Mining Workers. A description of the Base Node is provided in [RFC-0110/Base Nodes] (https://tari-project.github.io/tari/RFC-0110_BaseNodes.html).
A Mining Server is connected locally or remotely to a Tari Base Node and a Monero Node, and is responsible for constructing Tari and Monero Blocks from their respective mempools. The Mining Server should retrieve transactions from the mempool of the connected Base Node and assemble a new Tari block by bundling transactions together. Mining servers also have the option to re-verify transactions before including them in a new Tari block, but this verification process of checking that the transaction's rules such as signatures and timelocks are enforced is the responsibility of the connected Base Node. 

To enable Merged mining of Tari with Monero, both a Tari and a Monero block needs to be created and linked. First, a new Tari block is created and then the block header hash of the new Tari block should be included in the coinbase transaction of the new Monero block. Once a new merged mined Monero block has been constructed, PoW tasks can then be sent to the connected Mining Workers that will attempt to solve the block by performing the latest released version of the PoW algorithm selected by Monero.

Assuming the Tari difficulty is less than the Monero difficulty, miners get rewarded for solving the PoW at any difficulty above the Tari difficulty. If the block is solved above the Tari difficulty, a new Tari block is mined. If the difficulty is also greater than the Monero difficulty, a new Monero block is mined as well. In either event, the header for the candidate Monero block is included in the Tari block header.

If the PoW solution was sufficient to meet the difficult level of both the Tari and Monero blockchains, then the individual blocks for each blockchain can be sent from the Mining Server to the Base Node and Monero Node to be added to the different blockchains.  Before the Mining Server sends the new Tari block to the Base Node it must first update it by including the solved Monero block’s information (block header hash, Merkel tree branch, and hash of the coinbase transaction) into the PoW summary section of the Tari block header. If the PoW solution found by the Mining Workers only solved the problem at the Tari difficulty, then the new Tari block must be updated and added to the Tari blockchain and the Monero block can be discarded. 

This process will ensure that the Tari difficulty remains independent. Adjusting the difficulty will ensure that the Tari block times are preserved. Also, the Tari block time is (hard fork) flexible and can be less than, equal or greater than the Monero block times. A more detailed description of the Merged Mining process between a Primary and Auxiliary blockchain is provided in the [Merged Mining TLU report] (https://tlu.tarilabs.com/merged-mining/merged-mining.html).

#### Functionality required by the Tari Mining Server

- The Tari blockchain MUST have the ability to be merged mined with Monero. 
- The Mining Server MUST maintain a local or remote connection with a Base Node and a Monero Node.
- It MUST have a mechanism to construct a new Tari and Monero block by selecting transactions from the different Tari and Monero mempools that need to be included in the different blocks.
- It MAY have a configurable transaction selection mechanism for the block construction process. 
- It MAY have the ability to re-verify transactions before including them in a new Tari block.
- It MUST have the ability to include the block header hash of the new Tari block into the coinbase section of a newly created Monero block to enable merged mining.
- It MUST be able to include the Monero block header hash, Merkel tree branch and hash of the coinbase transaction of the Monero block into the PoW summary field of the new Tari block header. 
- It MUST have the ability to transmit and distribute PoW tasks for the newly created Monero block, that contains the Tari block information, to connected Mining Workers.
- It MUST verify PoW solutions received from Mining Workers and it MUST reject and discard invalid solutions or solutions that do not meet the minimum required difficulty.
- The Mining Server MAY keep track of mining share contributions of the connected Mining Workers. 
- It MUST submit completed Tari blocks to the Tari Base Node.
- It SHOULD submit completed Monero blocks to the Monero Network.  

#### Functionality required by the Tari Mining Worker

- It MUST maintain a local or remote connection to a Mining Server.

- It MUST have the ability to receive PoW tasks from the connected Mining Server. 

- It MUST have the ability to perform the latest released version of Monero's PoW algorithm on the received PoW tasks.

- It MUST attempt to solve the PoW algorithm at the Tari and/or Monero difficulties. 

- It MUST submit completed shares to the connected Mining Server. 


[blockchain]: Glossary.md#blockchain
[Base Layer]: Glossary.md#base-layer
[base node]: Glossary.md#base-node
[transaction]: Glossary.md#transaction
[block]: Glossary.md#block
[mempool]: Glossary.md#mempool
