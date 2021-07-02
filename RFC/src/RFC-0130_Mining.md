# RFC-0130/Mining

## Full-node Mining on Tari Base Layer

![status: deprecated](theme/images/status-deprecated.svg)

**Maintainer(s)**: [Yuko Roodt] (https://github.com/neonknight64)

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

The aim of this Request for Comment (RFC) is to provide a brief overview of the Tari merged mining process and introduce 
the primary functionality required of the Mining Server and Mining Worker.

## Related Requests for Comment

* [RFC-0100: Base Layer](RFC-0100_BaseLayer.md)
* [RFC-0110: Base Nodes](RFC-0110_BaseNodes.md)

## Description

### Assumptions
- The Tari [blockchain] will be merged mined with Monero.
- The Tari [Base Layer] has a network of [Base Node]s that verify and propagate valid [transaction]s and [block]s. 

### Abstract

The process of merged mining Tari with Monero on the Tari Base Layer is performed by [Mining Server]s and [Mining 
Worker]s. Mining Servers are responsible for constructing new blocks by bundling transactions from the [mempool] 
of a connected Base Node. They then distribute Proof-of-Work (PoW) tasks to Mining Workers in an attempt to solve 
newly created blocks. Solved solutions and shares are sent by the Mining Workers to the Mining Server, which in turn 
verifies the solution and distributes the newly created blocks to the Base Node and Monero Node for inclusion in 
their respective blockchains.

### Merged Mining on Tari Base Layer

This document is divided into three parts:

- A brief overview of the merged mining process and the interactions between the Base Node, Mining 
  Server and Mining Worker.
- The primary functionality required of the Mining Server.
- The primary functionality required of the Mining Worker.

####  Overview of Tari Merged Mining Process using Mining Servers and Mining Workers

Mining on the Tari Base Layer consists of three primary entities: the Base Nodes, Mining Servers and Mining Workers. 
A description of the Base Node is provided in [RFC-0110/Base Nodes](https://tari-project.github.io/tari/RFC-0110_BaseNodes.html).
A Mining Server is connected locally or remotely to a Tari Base Node and a Monero Node, and is responsible for 
constructing Tari and Monero Blocks from their respective mempools. The Mining Server should retrieve transactions 
from the mempool of the connected Base Node and assemble a new Tari block by bundling transactions together.

Mining servers may re-verify transactions before including them in a new Tari block, but this enforcement of 
verification and transaction rules such as signatures and timelocks is the responsibility of the connected Base Node. 
Mining Servers are responsible for [cut-through], as this is required for scalability and privacy.

To enable merged mining of Tari with Monero, both a Tari and a Monero block need to be created and linked. First, 
a new Tari block is created and then the block header hash of the new Tari block is included in the coinbase 
transaction of the new Monero block. Once a new merged mined Monero block has been constructed, PoW tasks can 
be sent to the connected Mining Workers, which will attempt to solve the block by performing the latest released 
version of the PoW algorithm selected by Monero.

Assuming the Tari difficulty is less than the Monero difficulty, miners get rewarded for solving the PoW at any 
difficulty above the Tari difficulty. If the block is solved above the Tari difficulty, a new Tari block is mined. 
If the difficulty is also greater than the Monero difficulty, a new Monero block is mined as well. In either event, 
the header for the candidate Monero block is included in the Tari block header.

If the PoW solution was sufficient to meet the difficult level of both the Tari and Monero blockchains, then the 
individual blocks for each blockchain can be sent from the Mining Server to the Base Node and Monero Node to be 
added to the respective blockchains.  

Every Tari block must include the solved Monero block's information (block header hash, Merkle tree branch and 
hash of the coinbase transaction) in the PoW summary section of the Tari block header. 
If the PoW solution found by the Mining Workers only solved the problem at the Tari difficulty, the Monero block can be discarded. 

This process will ensure that the Tari difficulty remains independent. Adjusting the difficulty will ensure that 
the Tari block times are preserved. Also, the Tari block time can be less than, equal to or greater than the Monero block 
times. A more detailed description of the merged mining process between a Primary and Auxiliary blockchain is provided 
in the [Merged Mining TLU report](https://tlu.tarilabs.com/merged-mining/merged-mining.html).

#### Functionality Required by Tari Mining Server

- The Tari blockchain MUST have the ability to be merged mined with Monero. 
- The Tari Mining Server:
  - MUST maintain a local or remote connection with a Base Node and a Monero Node.
  - MUST have a mechanism to construct a new Tari and Monero block by selecting transactions from the different 
    Tari and Monero mempools that need to be included in the different blocks.
  - MUST apply [cut-through] when mining Tari transactions from the [mempool] and only add the excess to the list of new Tari block transactions. 
  - MAY have a configurable transaction selection mechanism for the block construction process. 
  - MAY have the ability to re-verify transactions before including them in a new Tari block.
  - MUST have the ability to include the block header hash of the new Tari block in the coinbase section of a 
    newly created Monero block to enable merged mining.
  - MUST be able to include the Monero block header hash, Merkle tree branch and hash of the coinbase transaction 
    of the Monero block into the PoW summary field of the new Tari block header. 
  - MUST have the ability to transmit and distribute PoW tasks for the newly created Monero block, which contains 
    the Tari block information, to connected Mining Workers.
  - MUST verify PoW solutions received from Mining Workers and MUST reject and discard invalid solutions or 
    solutions that do not meet the minimum required difficulty.
  - MAY keep track of mining share contributions of the connected Mining Workers. 
  - MUST submit completed Tari blocks to the Tari Base Node.
  - MUST submit completed Monero blocks to the Monero Network.  

#### Functionality Required by Tari Mining Worker

The Tari Mining Worker:

- MUST maintain a local or remote connection to a Mining Server.
- MUST have the ability to receive PoW tasks from the connected Mining Server. 
- MUST have the ability to perform the latest released version of Monero's PoW algorithm on the received PoW tasks.
- MUST attempt to solve the PoW task at the difficulty specified by the Mining Server. 
- MUST submit completed shares to the connected Mining Server. 


[blockchain]: Glossary.md#blockchain
[Base Layer]: Glossary.md#base-layer
[base node]: Glossary.md#base-node
[transaction]: Glossary.md#transaction
[mining server]: Glossary.md#mining-server
[mining worker]: Glossary.md#mining-worker
[block]: Glossary.md#block
[mempool]: Glossary.md#mempool
[cut-through]: Glossary.md#cut-through
