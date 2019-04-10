# RFC-0230/HTLC

## Hashed Time Locked Contract (HTLC)

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

This document describes how to implement hashed time locked contracts as an extension to [Mimblewimble].

## Related RFCs
## Description
### Abstract
Hashed Time Locked Contracts ([HTLC]s) are time locked contracts that only pays out after a certain criteria has been met or refunds the originator if a certain period has expired. 

### Time Locked Contracts

In [Mimblewimble] time locked contracts can be accomplished by preventing a transaction to be mined or by preventing a UTXO that is available in the blockchain to be spent until certain conditions are met.

#### Time Locked Transactions

Time locked transactions can be accomplished by modifying the transaction kernel to include a lock height. This allows a limit to how early in the blockchain lifetime the specific transaction can be included in a block. 

This requires that users constructing a transaction:
* MUST include a lock height in the kernel of their transaction,
* MUST include the lock height in the signature challenge to stop tampering with the lock height after the transaction was constructed.

This add the following requirement to a miner:
* MUST not add any transaction to the new [block] to be mined where the blockchain's block height has not already exceeded the transaction's lock height.

This also adds the following requirement to a [base node]:
* MUST reject any [block] that contains a kernel with a lock height not already past the [current head].

#### Time Locked UTXOs

Time locked UTXOs can be accomplished by modifying the feature flags of a UTXO to include a lock height. This allows a limit as to when in the blockchain lifetime the specific UTXO can be spent. 

This requires that users constructing a transaction:

- MUST include a lock height in the feature flag of their UTXO,
- MUST include the lock height in the signature challenge to stop tampering with the lock height after the transaction was constructed.

This add the following requirement to a miner:

- MUST not allow a UTXO to be spent if the blockchain's block height has not already exceeded the UTXO's lock height.

This also adds the following requirement to a [base node]:

- MUST reject any [block] that contains a [UTXO] with a lock height not already past the [current head].

### Mimblewimble N-of-N Multisig UTXO

A normal Mimblewimble UTXO does not have a notion of being a [multisig] UTXO. The UTXO is hidden inside the commitment `C(v,r) = r·G + v·H` by virtue of being blinded. However, the blinding factor `r` can be composed of multiple blinding factors where `r = r1 + r2 + ... + rn`. The output commitment can then be constructed as `C(v,r) = r1·G + r2·G + ... + rn·G + v·H = (r1 + r2 + ... + rn)·G + v·H` where each participant keeps their private blinding factor hidden and only provides their public blinding factor. A multi-party aggregated signature (aggsig) scheme like Musig may be employed to sign such a transaction so that all parties' interests can be protected.

The base layer is oblivious as to how the commitment and related signature were constructed. To open such commitment (in order to spend it) only the n-of-n blinding factor `r` is required, and not the original aggsig that was used to sign the transaction. The parties that wants to open the commitment needs to collaborate with special scriptless scripts to produce the n-of-n blinding factor `r`.

### Hashed Time Locked Contract

Unlike Bitcoin where an [HTLC] can be accomplished with a single transaction, in [Mimblewimble] it is a multi-step process. 

The steps where only one sender and one receiver are involved are as follows:
* The sender MUST pay all the funds into a [2-of-2 multisig UTXO](#mimblewimble-n-of-n-multisig-utxo), where the participants are both the sender and receiver.
* The parties involved MUST construct a refund [transaction] paying back all funds to the sender from the 2-of-2 multisig [UTXO]. This transaction MUST have a [transaction lock height](#time-locked-transactions) in the far future so that it cannot be mined immediately. It therefor lives in the [mempool].
* The parties involved MUST publish both above [transaction]s at the same time. 
* The parties involved MUST construct a payout [transaction] that pay the receiver the funds. This type of [transaction] typically makes use of a preimage (similar to [adapter signatures](introduction-to-scriptless-scripts.html#adaptor-signatures)) to allow spending of the [transaction] if the user reveals some knowledge, allowing the user to unlock the UTXO.

HTLC's in [Mimblewimble] makes use of double spending the n-of-n [multisig] UTXO and the first valid published [transaction] can then be mined and claim the n-of-n multisig UTXO. 

An example of an [HTLC] in practice can be viewed at Tari University:
[Mimblewimble atomic swaps](https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#atomic-swaps)

[HTLC]: Glossary.md#hashed-time-locked-contract
[mempool]: Glossary.md#mempool
[Mimblewimble]: Glossary.md#mimblewimble
[base node]: Glossary.md#base-node
[block]: Glossary.md#block
[current head]: Glossary.md#current-head
[utxo]: Glossary.md#unspent-transaction-outputs
[multisig]: Glossary.md#multisig
[transaction]: Glossary.md#transaction
