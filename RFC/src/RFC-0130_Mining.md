# RFC-0130: Mining

## Full-node mining on Tari base layer

![status: raw](theme/images/status-raw.svg)

**Maintainer(s)**: <Maintainer Yuko Roodt> <github neonknight64>

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright <YEAR> <COPYRIGHT HOLDER | The Tari Development Community>

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

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

This document will provide an overview of the Tari mining process and will propose the primary functionality required for the Tari full-node miner.

## Related RFCs

## Assumptions
- That the Tari blockchain will be merged mined with Monero 


## Abstract

The process of mining on the Tari base layer is responsible for confirming and adding valid transactions to the Tari blockchain and distributing transactions on the Tari base layer network. This task is achieved by validating transactions and by performing Nakamoto consensus through Proof-of-Work. New blocks on the Tari blockchain can be merged mined with Monero by linking Tari blocks to solved Monero blocks.  

## Description


###  Overview of Tari merged mining process

Valid transactions that need to be included on the Tari blockchain should be propagated by the mining full-nodes on the Tari base layer network. A Tari mining full-node should retrieve transactions from its mempool and assemble a new Tari block by bundling transactions together. It should ensure that the transactions that are included in the new Tari block are valid and that rules such as signatures, multi-signatures and timelocks are enforced before they are including in a new block.

As Tari could be merged mined with Monero, both a Tari and Monero block needs to be created and linked by including Tari block information in the Monero block and Monero block information in the Tari block. First, a new Tari block is created and then the block header hash of the new Tari block should be included in the coinbase transaction of the new Monero block. Once the block construction is complete, the mining full-node should perform the CryptoNight Proof-of-Work algorithm on the Monero block that includes the information of the new Tari block. 

The solution to the PoW problem could be solved at the difficulty of either the Tari and/or Monero blockchain. If a solution has been found that meets the minimum difficulty requirements of the Tari and/or Monero blockchain, then the new Tari block should be updated by including the solved Monero block’s information (block header hash, merkel tree branch, and hash of the coinbase transaction). If a solution was found that meet the Tari or Monero blockchain difficulty then the new Tari block can be added to the Tari blockchain. If the solution met the difficulty requirements of the Monero blockchain then the new Monero block can also be added to the Monero blockchain. If the PoW solution was sufficient to meet the difficult level of both the Tari and Monero blockchains then the individual blocks for each cryptocurrency can be added to their respective blockchain.

Solved and completed blocks should be propagated to the rest of the base layer network so that Nakamoto consensus can be performed on the new block and it can be added to the local blockchain copies of the full mining nodes.

###  Primary functionality required by a Tari full-node miner
- The Tari mining full-node can be merged mined with Monero using the CryptoNight Proof-of-Work algorithm. 
- The mining full-node must maintain complete or pruned copies of the Tari and Monero blockchains.
- It must be able to transmit and propagate information on the Tari base layer network using peer-to-peer communication using a gossip protocol. It could also propagate information and blocks on the Monero network.
- It should have a mechanism to construct a new Tari and Monero block by selecting transactions from the different Tari and Monero mempools that need to be included in the different blocks.
- It must have the ability to include the block header hash of the new Tari block into the coinbase section of a newly created Monero block.
- It must be able to include the Monero block header hash, merkel tree branch, and hash of the coinbase transaction of the Monero block, into the extra data field of the new Tari block. 
- It must have the ability to perform a Proof-of-Work (PoW) algorithm on the newly created Tari and Monero blocks, attempting to solve the PoW at the desired Tari or Monero difficulty.  
- It must have the functionality to verify the validity of a newly received or created block for either Tari and/or Monero.
- Created or received valid Tari blocks should be included in the local Tari blockchain.
- Created or received valid Monero blocks should be included in the local Monero blockchain.
- Valid Tari and/or Monero blocks should be propagated to other mining nodes on the Tari base layer network and/or the Monero network. 
- Invalid  Tari and/or Monero blocks must be reject and discarded.  















