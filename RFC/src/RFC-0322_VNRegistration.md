# RFC-0322/VNRegistration

## Validator Node Registration

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019. The Tari Development Community

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

This document describes Validator Node registration. Registration accomplishes two goals:

1. Provide a register of Validator Nodes with an authority (the Tari base layer).
2. Offer Sybil resistance against gaining probabilistic majority of any given publicly nominated VN committee.

## Related RFCs

* [RFC-0302: Validator Nodes](RFC-0302_ValidatorNodes.md)
* [RFC-0304: Validator Node committee selection](RFC-0304_VNCommittees.md)
* [RFC-0170: The Tari Communication Network and Network Communication Protocol](RFC-0170_NetworkCommunicationProtocol.md)
* [RFC-0341: Asset Registration](RFC-0341_AssetRegistration.md)
* [RFC-0200: Base Layer Extensions](RFC-0200_BaseLayerExtensions.md)

## Description

Validator Nodes register themselves on the [Base layer] using a special [transaction] type. The registration
[transaction] type requires the spending of a certain minimum amount of [Tari coin], the ([Registration Deposit]),
that has a time-lock on the output for a minimum amount of time ([Registration Term]) as well as some metadata, such as
the VNs public key.

The Node ID is calculated after registration to prevent mining of VN public keys that can be used to manipulate routing
on the DHT.

Once a VNs [Registration Term] has expired so will this specific VN registration. The UTXO timelock will have elapsed so
the [Registration Deposit] can be reclaimed and a new VN registration need to be performed. This automatic
registration expiry will ensure that the VN registry stays up to date with active VN registrations and inactive
registrations will naturally be removed.

Requiring nodes to register themselves serves two purposes:
* Makes VN Sybil attacks expensive,
* Provides an authoritative "central-but-not-centralised" registry of validator nodes from the base layer.

### Node ID

The Validator node ID can be calculated deterministically after the VN registration transaction is mined. This ensures
that VNs are randomly distributed over the DHT network.

VN Node IDs MUST be calculated as follows:

    NodeId = Hash( pk || h || kh )

Where

| Field | Description                                                                   |
|:------|:------------------------------------------------------------------------------|
| pk    | The Validator Node's DHT public key                                           |
| h     | The block height of the block in which the registration transaction was mined |
| kh    | The hash of the registration transaction's kernel                             |

Base Nodes SHOULD maintain a cached list of Validator Nodes and MUST return the Node ID in response to a
`get_validator_node_id` request.

### Validator node registration

A validator node MUST register on the base layer before it can join any DAN [committee]s. Registration happens by virtue
of a Validator Node Registration [transaction].

VN registrations are valid for the [Registration Term].

The registration term is set at SIX months.

A VN registration transaction is a special transaction.

* The transaction MUST have EXACTLY ONE UTXO with the `VN_Deposit` flag set.
* This UTXO MUST also:
  * Set a time lock for AT LEAST the [Registration Term] (or equivalent block periods)
  * Provide the _value_ of the UTXO in the signature metadata
  * Provide the _public key_ for the spending key for the output in the signature metadata
* The value of this output MUST be equal or greater than the [Registration Deposit].
* The UTXO MUST store:
  * The value of the VN deposit UTXO in as a u64.
  * The value of the _public key_ for the spending key for the output as 32 bytes in little endian order.
* The `KernelFeatures` bit flag MUST have the `VN_Registration` flag set.
* The kernel MUST also store the VN's DHT public key as 32 bytes in little endian order.

### Validator node registration renewal

If a VN owner does not _renew_ the registration before the [Registration Term] has expired, the registration will lapse
and the VN will no longer be allowed to participate in any [committee]s.

The number of consecutive renewals MAY increase the VN's reputation score.

A VN may only renew a registration in the TWO WEEK period prior to the current term expiring.

A VN renewal transaction is a special transaction:

* The transaction MUST have EXACTLY ONE UTXO with the `VN_Deposit` flag set.
* The transaction MUST spend the previous VN Deposit UTXO for this VN.
* This UTXO MUST also:
  * Set a time lock for AT LEAST 6 months (or equivalent block periods)
  * Provide the _value_ of the transaction in the signature metadata
  * Provide the _public key_ for the spending key for the output in the signature metadata
* This UTXO MUST also store
  * The value of the VN collateral UTXO in as a u64.
  * The value of the _public key_ for the spending key for the output as 32 bytes in little endian order.
  * The VN's Node ID. This can be validated by following the Renewal transaction kernel chain.
  * The kernel hash of this transaction's kernel.
  * A counter indicating that this is the n-th consecutive renewal. This counter will be confirmed by nodes and miners.
    The first renewal will have counter value of one.
* The previous VN deposit UTXO MUST NOT be spendable in a standard transaction (i.e. its time lock has not expired).
* The previous VN deposit UTXO MUST expire within the next TWO WEEKS.
* The transaction MAY provide additional inputs to cover transaction fees and increases in the [Registration Deposit].
* The transaction kernel MUST have the `VN_Renewal` bit flag set.
* The transaction kernel MUST also store
  * The hash of the previous renewal transaction kernel, or the registration kernel if this is the first renewal.

One will notice that a validator node's Node ID does not change as a result of a renewal transaction. Rather, every
renewal adds to a chain linking back to the original registration transaction. It may be desirable to establish a long
chain of renewals, in order to offer evidence of longevity and improve a VN's reputation.

[Tari Coin]: Glossary.md#tari-coin
[Transaction]: Glossary.md#transaction
[Node ID]: Glossary.md#node-id
[Base layer]: Glossary.md#base-layer
[Committee]: Glossary.md#committee
[Registration Term]: Glossary.md#registration-term
[Registration Deposit]: Glossary.md#registration-deposit