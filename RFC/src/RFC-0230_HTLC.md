# RFC-0230/Time-related Transactions

## Time-related Transactions

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [S W van Heerden](https://github.com/SWvheerden) and [Philip Robinson](https://github.com/philipr-za)

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

#### Time-locked UTXOs
Time-locked Unspent Transaction Outputs (UTXOs) can be accomplished by adding a feature flag to a UTXO and a lock height, 
also referred to as the output's maturity. This allows a consensus limit on after which height the output can be 
spent.

This requires that users constructing a transaction:

- MUST include a feature flag of their UTXO; and
- MUST include a lock height in their UTXO.

This adds the following requirement for a [base node]:
- A [base node] MUST NOT allow a UTXO to be spent if the [current head] has not already exceeded the UTXO's lock height.

This also adds the following requirement for a [base node]:
- A base node MUST reject any [block] that contains a [UTXO] with a lock height not already past the [current head].

#### Time-locked Contracts
In standard [Mimblewimble], time-locked contracts can be accomplished by modifying the kernel of each transaction to 
include a lock height. This limits how early in the blockchain lifetime the specific transaction can be included in a 
block. This approach is used in a traditional [Mimblewimble] construction that does not implement any kind of scripting.
This has two disadvantages. Firstly, the spending condition is very primitive and cannot be linked to other conditions.
Secondly, it bloats the kernel, which is a component of the transaction that cannot be pruned.

However, with [TariScript] it becomes possible to express spending conditions like a time-lock as part of a UTXO's script.
The `CheckHeightVerify(height)` TariScript Op code allows a time-lock check to be incorporated into a script. The following is a simple
example of a plain time-lock script that prevents an output from being spent before the chain reaches height 4000:

```text
CheckHeightVerify(4000)
```

#### Hashed Time-locked Contract
Hashed time-locked contracts ([HTLC]) are a way of reserving funds that can only be spent if a hash pre-image can be provided or
if a specified amount of time has passed. The hash pre-image is a secret that can be revealed under the right conditions
to enable spending of the UTXO before the time-lock is reached. The secret can be directly exchanged between the parties or
revealed to the other party by spending an output that makes use of an adaptor signature.

[HTLC]s enable a number of interesting transaction constructions. For example, [Atomic Swaps](https://tlu.tarilabs.com/protocols/atomic-swaps/AtomicSwaps.html)
and Payment Channels like those in the [Lightning Network](https://tlu.tarilabs.com/protocols/lightning-network-for-dummies).

The following is an example of an [HTLC] script. In this script, Alice sends some Tari to Bob that he can spend using the 
private key of `P_b` if he can provide the pre-image to the SHA256 hash output (`HASH256{pre_image}`) specified in the script
by Alice. If Bob has not spent this UTXO before the chain reaches height 5000 then Alice will be able to spend the output
using the private key of `P_a`.

```text
HashSha256
PushHash(HASH256{pre_image})
Equal
IFTHEN
   PushPubkey(P_b)
ELSE
   CheckHeightVerify(5000)
   PushPubkey(P_a)
ENDIF
```

A more detailed analysis of the execution of this kind of script can be found at [Time-locked Contact](RFC-0202_TariScriptOpcodes.md#time-locked-contract)



[HTLC]: Glossary.md#hashed-time-locked-contract
[Mempool]: Glossary.md#mempool
[Mimblewimble]: Glossary.md#mimblewimble
[Base Node]: Glossary.md#base-node
[Block]: Glossary.md#block
[current head]: Glossary.md#current-head
[UTXO]: Glossary.md#unspent-transaction-outputs
[Multisig]: Glossary.md#multisig
[Transaction]: Glossary.md#transaction
[TariScript]: RFC-0201_TariScript.md


