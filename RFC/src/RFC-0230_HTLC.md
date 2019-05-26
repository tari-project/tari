# RFC-0230/Time related transactions

## Time related transactions

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [SW van heerden](https://github.com/SWvheerden)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019 The Tari Development Community

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

This document describes a few extension to [MimbleWimble] to allow time related transactions.

## Related RFCs

* [RFC-0200: Base Layer Extensions](BaseLayerExtensions.md)

## Description

#### Time Locked contracts
In [Mimblewimble] time-locked contracts can be accomplished by modifying the kernel of each transaction to include a
block height. This limits how early in the blockchain lifetime the specific transaction can be included in a block.

This requires users constructing a transaction to:
* MUST include a lock height in the kernel of their transaction,
*  MUST include the lock height in the transaction signature to prevent lock height malleability.

Tari Miners:
* MUST not add any transaction to the mined [block] that has not already exceeded its lock height.

This also adds the following requirement to a [Base Node]:
* MUST reject any [block] that contains a kernel with a lock height greater than the [current head].

#### Time Locked UTXOs
Time locked UTXOs can be accomplished by adding feature flag to a UTXO and a lock height. This allows a limit as to when
 in the blockchain lifetime the specific UTXO can be spent.

This requires that users constructing a transaction:

- MUST include a the feature flag of their UTXO,
- MUST include a lockheight in their UTXO.

This adds the following requirement to a miner:
- MUST not allow a UTXO to be spent if the [current head] has not already exceeded the UTXO's lock height.

This also adds the following requirement to a [base node]:
- MUST reject any [block] that contains a [UTXO] with a lock height not already past the [current head].

#### Hashed Time Locked Contract
Hashed time locked contracts are a way of reserving funds for a certain payment, but it only pays out to the receiver if
certain conditions are met. If these are not met withing a time limit, the funds are payed back to the sender.

Unlike Bitcoin where this can be accomplished with a single transaction, in [MimbleWimble] HTLCs involve a multi-step
process to construct a timelocked contract.

The steps are as follows:
* The sender MUST pay all the funds into a n-of-n [multisig] [UTXO].
* All parties involved MUST construct a refund [transaction] paying back all funds to the sender, spending this n-of-n
  [multisig] [UTXO]. However, this [transaction] has a [transaction lock height](#hashed-time-locked-contract) set in
  the future and cannot be immediately mined. It therefore lives in the [mempool]. This means that if anything goes
  wrong from here on out, the sender will get his money back after the time lock expires.
* The sender MUST publish both above [transaction]s at the same time to ensure the receiver cannot hold him hostage.
* The parties MUST construct a third [transaction] that pays the receiver the funds. This [transaction] typically makes
  use of a preimage to allow spending of the [transaction] if the user reveals some knowledge, allowing the user to
  unlock the [UTXO].

HTLC's in [Mimblewimble] makes use of double spending the n-of-n [multisig] [UTXO] and the
first valid published [transaction] can then be mined and claim the n-of-n [multisig]
[UTXO].

An example of a [HTLC] in practice can be viewed at Tari University:
[Bitcoin atomic swaps](https://tlu.tarilabs.com/protocols/atomic-swaps/AtomicSwaps.html)
[MimbleWimble atomic swaps](https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#atomic-swaps)

[HTLC]: Glossary.md#hashed-time-locked-contract
[Mempool]: Glossary.md#mempool
[Mimblewimble]: Glossary.md#mimblewimble
[Base Node]: Glossary.md#base-node
[Block]: Glossary.md#block
[current head]: Glossary.md#current-head
[UTXO]: Glossary.md#unspent-transaction-outputs
[Multisig]: Glossary.md#multisig
[Transaction]: Glossary.md#transaction


