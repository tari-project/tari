# Mempool

## The Mempool for Unconfirmed Transactions on the Tari Base Layer

![status: outdated](theme/images/status-outofdate.svg)

**Maintainer(s)**: [Yuko Roodt](https://github.com/neonknight64)

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

This document will introduce the Tari [base layer] [Mempool] that consists of a [Transaction Pool], [Pending Pool], [Orphan Pool] and [Reorg Pool].
The Mempool is used for storing and managing unconfirmed and time-lock restricted [transaction]s.

## Related RFCs

* [RFC-0100: The Tari Base Layer](RFC-0100_BaseLayer.md)

## Description

### Assumptions

- Each [base node] is connected to a number of peers that maintain their own copies of the Mempool.

### Abstract

The Mempool is responsible for managing, verifying and maintaining all unconfirmed transactions that have not yet 
been included in a [block] and added to the Tari [blockchain]. It consists of a Transaction Pool, Pending Pool, 
Orphan Pool and Reorg Pool to achieve these tasks. It is also responsible for propagating valid transactions and 
sharing the Mempool state with connected peers. An overview of the required functionality for the Mempool and each 
of its component pools will be provided.

### Overview

Every base node maintains a Mempool that consists of four separate pools: the Transaction Pool, Pending Pool, 
Orphan Pool and Reorg Pool. These four pools have different tasks and work together to form the Mempool used 
for maintaining unconfirmed transactions. 

This is the role descriptions for each component pool:
- Transaction Pool: contains all unconfirmed transactions that have been verified, have passed all checks, that 
only spend valid [UTXO]s and don't have any time-lock restrictions.
- Pending Pool: contains unconfirmed transactions that have time-lock restrictions. The transactions in this pool 
either attempt to spend UTXOs with time-locks or the transactions themselves have time-locks limiting them from 
being included in new blocks until a specified future time or block height has been reached. 
- Orphan Pool: out of order unconfirmed transactions are managed by this pool, these transactions attempt to spend 
non-existent UTXOs.
- Reorg Pool: stores a backup of all transactions that have recently been included into blocks, in case a blockchain 
reorganization occurs and these transactions have to be restored to the Transaction Pool so that they can be included 
in future blocks.

### Prioritizing Unconfirmed Transactions

The maximum storage capacity used for storing unconfirmed transactions by the Mempool and each of its component pools 
can be configured. If a new transaction is received and the storage capacity limits have been reached, then 
transactions are prioritized according to the transaction priority metric. The transaction priority metric consist 
of the fees and the maturity of the UTXOs being spent by the transaction and should be applied to determine the priority 
of each transaction in the Mempool. As the allocated storage space of the Mempool becomes limited, the transactions 
with the highest priority are kept in the pool. The lowest priority transactions are discarded to make room for 
higher priority incoming transactions.

The transaction priority metric has the following behavior:
 - Transactions spending UTXOs with higher block height maturity SHOULD be prioritized over transactions spending UTXOs 
 with lower block height maturity.
 - Transactions with higher fees per transaction message size SHOULD be prioritized over lower fee transactions.

### Syncing and Updating of the Memory Pool State

On the initial startup of the Mempool, the complete state of the Mempool that consists of the Transaction Pool, Pending 
Pool, Orphan Pool and Reorg Pool can be requested and downloaded from the connected peers. A memory pool typically 
doesn't make use of persistent storage but could be configured to keep a backup of the last known state. If an existing 
Mempool state is locally available then a more efficient update process can be performed by requesting only the 
unconfirmed transactions missing from the current Mempool state. When no state is available then the entire Mempool 
state must be downloaded from the connected peers. During downloading or updating of the Mempool state, the validity 
of all transaction in the pool must be verified and the priority of each transaction must be calculated. 

Functional behavior required for sharing and updating of the Mempool state, and propagation of transactions between peers:
- All verified transaction MUST be propagated to neighboring peers.
- Duplicate transactions MUST NOT be propagated to peers.
- Unvalidated or invalid transactions MUST NOT be propagated to peers.
- Verified transactions that were discarded due to low priority levels MUST be propagated to peers.
- The Mempool MUST have an interface that can be used by neighboring peers to query the content and state of the memory 
pool.
- It MUST have a mechanism that enables peers to download the memory pool state in full or in part.
- It MUST accept all transactions received from peers but MAY decide to discard low priority transactions.
- It MUST allow wallets to track payments by monitoring that a particular transaction has been added to the Mempool.
- A Mempool MAY choose:
    - to discard the Mempool state on restarts and then download the full state from its peers or
    - to store the state of the Mempool using persistent storage to reduce communication bandwidth required when 
    reinitializing the Mempool after a restart.

### Transaction Pool

The Transaction Pool consists of all unconfirmed transactions that have been received, verified and have passed 
all checks. These unconfirmed transactions in the Transaction Pool are ready to be included and can be used to 
construct new blocks for the Tari blockchain.

Functional behavior required of the Transaction Pool:
- It MUST verify that incoming transactions only spend existing UTXOs.
- It MUST ensure that incoming transactions don't have a processing time-lock or has a time-lock that has 
expired.
- It MUST ensure that all time-locks of the UTXOs that will be spent by the transaction have expired.
- Transactions that have been used to construct new blocks MUST be removed from the Transaction Pool and added to the Reorg Pool.

### Pending Pool

The Pending Pool contains all transactions that are restricted by time-locks. A transaction could have a time-lock 
limiting it from being processed or it can attempt to spend UTXOs with time-locks. These transactions require 
their time-locks or the time-locks of the input UTXOs to expire before they can be processed and included into new 
blocks. All transactions in the Pending Pool have been verified and passed all checks except their own time-lock has 
not yet expired or some of the UTXOs that will be spent have time-lock restrictions that are not yet valid. Once the 
transactions time-lock or the time-locks on the UTXOs have expired then the Pending Pool transactions can be moved 
to the Transaction Pool for inclusion in future blocks.  

Functional behavior required of the Pending Pool:
- Once the transaction time-lock or UTXO time-lock restricting the processing of a transaction has expired then the 
pending transaction MUST be moved to the Transaction Pool.

### Orphan Pool

The Orphan Pool contains all the received transactions that have passed all the verification steps and checks, except 
they attempt to spend UTXOs that don't exist. A possible reason these UTXOs do not yet exist is that they may not yet 
have been created and might exist in the future. Typically, these orphaned transactions are from a series or bundle of 
transactions that need to be processed in a specific order but
- have been either received out of order,
- or the order of processing the bundled transactions might not have been known or specified.

As transactions are processed and the missing UTXOs have been created, then the orphaned transactions can be moved 
to the Transaction Pool for possible inclusion in future blocks. Another possibility why the input UTXOs might not 
be available is that the UTXOs were double spent by other transactions. In time, the double spent transactions will 
be discarded from the Orphan Pool once they have reached the appropriate maturity threshold.

Functional behavior required of the Orphan Pool:
- Each newly received transaction MUST be verified and pass all checks except the UTXO validity check before it is 
placed in the Orphan Pool.
- Orphaned transactions must be upgraded and moved to the Transaction Pool once the previously unavailable UTXOs become 
available.
- Orphaned transactions that have surpassed the expiration time threshold MUST be removed from the Orphan Pool.

### Reorg Pool

The Reorg Pool consists of all unconfirmed transaction that have recently been added to blocks, resulting in 
their removal from the Transaction Pool. When a potential blockchain reorganization occurs that invalidates previously 
assembled blocks, the transactions used to construct these discarded blocks can be recovered from the Reorg Pool and 
can be added back into the Transaction Pool. This will ensure that high priority transactions are not lost during 
blockchain reorganization but can be added into future blocks without retransmission of these transactions.

Functional behavior required of the Reorg Pool:
- Copies of the verified transactions removed from the Transaction pool that were placed in blocks MUST be stored in 
the Reorg Pool.
- Transactions in the Reorg pool MAY be removed after the threshold expiration time has been reached. 
- When a blockchain reorganization is detected, all affected transactions from the Reorg Pool MUST be moved to the 
Transaction Pool.

### Mempool

The Mempool manages the four component pools and interacts with peers to share and retrieve transactions and 
the Mempool state. During the operation of the Mempool it will distribute incoming transactions to the appropriate 
component pools. When a new incoming transaction is received a number of checks and verification steps need to be performed 
to determine if the transaction can be added to the Mempool and determine which of the component pools should be responsible 
for that particular transaction. Only when the new transaction has passed these checks can it be added to the Mempool and 
should it be propagated to the connected peers.

Functional behavior required of the Mempool:
- If a duplicate transaction is received, that already exist in the Mempool, then the duplicate copy MUST be discarded.
- When considering transactions that attempt to double spend UTXOs, the highest priority transaction MUST be kept and any 
other transactions that spend the same UTXO MUST be discarded.
- When the storage limit of the Mempool has been reached, new incoming transactions SHOULD be prioritized according to the 
Priority metric.
- Lower priority Mempool transactions MUST be discarded to make room for higher priority incoming transactions.
- Incoming transactions with lower priorities than the minimum transaction priority in the Mempool MUST be discarded.
- The Mempool MUST verify that incoming transactions do not have duplicate outputs.
- It MUST check that all coinbase outputs that will be spent have matured sufficiently.
- The distribution of storage space allocated to each component pool in the memory pool MAY be configured and adjusted.
- The memory pool SHOULD have a mechanism to estimate fee categories from the current Mempool state. As an example, 
a priority fee can be estimated that will ensure that a new transactions will have the appropriate priority to be added 
into a new block in a timely manner.

Functional behavior required for distributing incoming transactions to the component pools:
- Verified transactions that have passed all checks such as spending of valid UTXOs and expired time-locks MUST be 
placed in the Transaction Pool
- All transactions that attempt to spend UTXOs with valid time-locks MUST be added to the Pending Pool.
- Incoming transactions with time-locks prohibiting them from being included into new blocks should be added to the 
Pending Pool.
- Newly received verified transaction attempting to spend a UTXO that does not yet exist MUST be added to the Orphan 
pool.
- Transactions that have been added to blocks and were removed from the Transaction Pool should be added to the Reorg Pool.

[base layer]: Glossary.md#base-layer
[mempool]: Glossary.md#mempool
[transaction pool]: Glossary.md#transaction-pool
[pending pool]: Glossary.md#pending-pool
[orphan pool]: Glossary.md#orphan-pool
[reorg pool]: Glossary.md#reorg-pool
[transaction]: Glossary.md#transaction
[base node]: Glossary.md#base-node
[block]: Glossary.md#block
[blockchain]: Glossary.md#blockchain
[utxo]: Glossary.md#unspent-transaction-outputs
