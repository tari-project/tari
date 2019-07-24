# RFC-0230/Time-related Transactions

## Time-related Transactions

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [S W van Heerden](https://github.com/SWvheerden)

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

The aim of this Request for Comment (RFC) is to describe a few extensions to [Mimblewimble] to allow time-related transactions.

## Related Requests for Comment

* [RFC-0200: Base Layer Extensions](BaseLayerExtensions.md)

## Description

#### Time-locked Contracts
In [Mimblewimble], time-locked contracts can be accomplished by modifying the kernel of each transaction to include a
block height. This limits how early in the blockchain lifetime the specific transaction can be included in a block.

This means that users constructing a transaction:
* MUST include a lock height in the kernel of their transaction; and
* MUST include the lock height in the transaction signature to prevent lock height malleability.

Tari Miners MUST NOT add any transaction to the mined [block] that has not already exceeded its lock height.

This also adds the following requirement to a [Base Node]:
* It MUST reject any [block] that contains a kernel with a lock height greater than the [current head].

#### Time-locked UTXOs
Time-locked Unspent Transaction Outputs (UTXOs) can be accomplished by adding a feature flag to a UTXO and a lock height. 
This allows a limit on when in the blockchain lifetime the specific UTXO can be spent.

This requires that users constructing a transaction:

- MUST include a feature flag of their UTXO; and
- MUST include a lock height in their UTXO.

This adds the following requirement for a miner:
- A miner MUST not allow a UTXO to be spent if the [current head] has not already exceeded the UTXO's lock height.

This also adds the following requirement for a [base node]:
- A base node MUST reject any [block] that contains a [UTXO] with a lock height not already past the [current head].

#### Hashed Time-locked Contract
Hashed time-locked contracts are a way of reserving funds for a certain payment, but they only pay out to the receiver if
certain conditions are met. If these conditions are not met within a time limit, the funds are paid back to the sender.

Unlike Bitcoin, where this can be accomplished with a single transaction, in [Mimblewimble], HTLCs involve a multi-step
process to construct a time-locked contract.

The steps are as follows:
* The sender MUST pay all the funds into an n-of-n [multisig] [UTXO].
* All parties involved MUST construct a refund [transaction] to pay back all funds to the sender who has spent this n-of-n
  [multisig] [UTXO]. However, this [transaction] has a [transaction lock height](#hashed-time-locked-contract) set in
  the future and cannot be mined immediately. It therefore lives in the [mempool]. This means that if anything goes
  wrong from here on, the sender will get their money back after the time lock expires.
* The sender MUST publish both above [transaction]s at the same time to ensure the receiver cannot hold the sender hostage.
* The parties MUST construct a third [transaction] that pays the receiver the funds. This [transaction] typically makes
  use of a preimage to allow spending of the [transaction] if the user reveals some knowledge, allowing the user to
  unlock the [UTXO].

HTLCs in [Mimblewimble] make use of double-spending of the n-of-n [multisig] [UTXO]. The
first valid published [transaction] can then be mined and claim the n-of-n [multisig]
[UTXO].

An example of an [HTLC] in practice can be viewed at Tari University:

- [Bitcoin atomic swaps](https://tlu.tarilabs.com/protocols/atomic-swaps/AtomicSwaps.html)
- [Mimblewimble atomic swaps](https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#atomic-swaps)

[HTLC]: Glossary.md#hashed-time-locked-contract
[Mempool]: Glossary.md#mempool
[Mimblewimble]: Glossary.md#mimblewimble
[Base Node]: Glossary.md#base-node
[Block]: Glossary.md#block
[current head]: Glossary.md#current-head
[UTXO]: Glossary.md#unspent-transaction-outputs
[Multisig]: Glossary.md#multisig
[Transaction]: Glossary.md#transaction


