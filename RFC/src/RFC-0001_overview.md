# RFC-0001/Overview

## An overview of the Tari network

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

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

## Goals

The aim of this proposal is to provide a very high-level perspective for the moving parts involved in the Tari protocol.

## Related RFCs

* [RFC-0100: Base layer](RFC-0100_BaseLayer.md)
* [RFC-0300: Digital asset network](RFC-0300_DAN.md)
* [RFC-0310: Digital assets](RFC-0310_Assets.md)

## Description

### Abstract

The Tari network is composed of two layers:

1. The base layer deals with [Tari coin] [transaction]s. It governed by a proof-of-work blockchain that is merged-mined with
Monero. The base layer is highly secure, decentralised and relatively slow.
2. The digital assets network runs on a second layer. This layer manages all things to do with native digital assets. It
   is built for liveness, speed and scalability at the expense of decentralisation.

### Currency tokens and digital assets

There are two major digital entities on the Tari network: The coins that are the unit of transfer for the Tari
cryptocurrency, and the digital assets that could represent anything from tickets to in-game items.

Tari coins are the fuel that drives the entire Tari ecosystem. But they share many of the properties of money, and so
security is a non-negotiable requirement. In a cryptocurrency context, this is usually achieved by employing a
decentralised network running a censorship-resistant protocol like Nakamoto consensus over a proof of work blockchain.
As we know, proof of work blockchains are not scalable, or terribly fast.

On the other hand, the Tari network will be used to create and manage digital assets.

In Tari parlance, a digital asset is defined as a finite set of digital stateful tokens that are governed by predefined
rules. A single digital asset may define anything from one to thousands of tokens within its scope.

For example, in a ticketing context, an _event_ will be an asset. The asset definition will allocate tokens representing
the tickets for that event. The ticket tokens will have state, such as its current owner and whether it has been
redeemed or not. Users might be interacting with digital assets hundreds of times a second, and state updates need to be
propagated and agreed upon by the network very quickly. A blockchain-enabled ticketing system is practically useless if
a user has to wait for "3 block confirmations" before the bouncer will let her into a venue. Users expect near-instant
state updates because centralised solutions offer them that today.

Therefore the Tari digital assets network must offer speed and scalability.

#### Two layers

The [distributed system trilemma](https://en.wikipedia.org/wiki/CAP_theorem) tells us that these requirements are
 mutually exclusive.

We can't have fast, cheap digital assets and also highly secure and decentralised currency tokens on a single system.

Tari overcomes this constraint by building two layers:

* A base layer that provides a public ledger of Tari coin transactions, secured by proof of work to maximise security, and
* A second layer that manages digital asset state that is very fast and cheap, at the expense of decentralisation.

If required, the digital asset layer can refer back to the base layer to temporarily give up speed in exchange for
increased security. This fallback is used to resolve consensus issues on the second layer that may crop up from time to
time as a result of the lower degree of decentralisation.

### The Base Layer

_Refer to [RFC-0100/BaseLayer](RFC-0100_BaseLayer.md) for more detail_.

The Tari base layer has the following primary features:

* Proof of work-based blockchain using Nakamoto consensus
* Transactions and blocks based on the [Mimblewimble] protocol

[Mimblewimble] is an exciting new blockchain protocol that offers some key advantages over other [UTXO]-based
cryptocurrencies like Bitcoin:

* Transactions are private. This means that casual observers cannot ascertain the amounts being transferred or the
  identities of the parties involved.
* Mimblewimble employs a novel blockchain "compression" method called cut-through that dramatically reduces the
  storage requirements for blockchain nodes.
* Multi-signature transactions can be easily aggregated, making such transactions very compact, and completely hiding
  the parties involved, or the fact that there were multiple parties involved at all.

> "Mimblewimble is the most sound, scalable 'base layer' protocol we know" -- @fluffypony

#### Proof of work

There are a few options for the proof of work mechanism for Tari:

* Implement an existing PoW mechanism. This is a bad idea, because a nascent cryptocurrency that uses a non-unique
  mining algorithm is incredibly vulnerable to a 51% attack from miners from other currencies using the same algorithm.
  Bitcoin Gold and Verge have already experienced this, and it's a [matter of time](https://www.crypto51.app/) before it
  happens to others.
* Implement a unique PoW algorithm. This is a risky approach and comes close to breaking the #1 rule of
  cryptocurrency design: Never roll your own crypto.
* [Merge mining](https://tari-labs.github.io/tari-university/merged-mining/merged-mining-scene/MergedMiningIntroduction.html).
  This approach is not without its own risks but offers the best trade-offs in terms of bootstrapping the network. It
  typically provides high levels of hash rate from day one along with 51% attack resistance assuming mining pools are
  well-distributed.
* A hybrid approach, utilising two or more of the above mechanisms.

Given Tari's relationship with Monero, a merge-mined strategy with Monero makes the most sense, but the PoW mechanism
SHOULD be written in a way that makes it relatively easy to code, implement and switch to a different strategy in the
future.

### The Digital Assets Network

A more detailed proposal for the digital assets network is presented in [RFC-0300/DAN](RFC-0300_DAN.md). Digital assets
_are discussed in more detail in [RFC-0310/Assets](RFC-0310_Assets.md)._

The Tari digital assets network (DAN) consists of a peer-to-peer network of [Validator nodes]. These nodes ensure the
safe and efficient operation of all native digital assets on the Tari network.

Validator nodes are responsible for

* _registering_ themselves on the base layer.
* validating and executing the contracts that _create_ and issue _new digital assets_ on the network.
* validating and executing _instructions_ for _changes in state_ of digital assets, for example allowing the transfer of
  ownership of a token from one person to another.
* _Maintaining consensus_ with other validator nodes managing the same asset.
* submitting periodic _checkpoints_ to the base layer for the state of assets under their management.

The DAN is focused on achieving high speed and scalability, without compromising on security. To achieve
this we make the explicit trade-off of sacrificing decentralisation.

In many ways this is desirable, since the vast majority of assets (and their issuers) don't need or want _the entire
network_ to validate every state change in their asset contracts.

Digital assets necessarily have _state_. Therefore the digital assets layer must have a means of synchronising and
agreeing on state that is managed simultaneously by multiple servers (a.k.a. reaching consensus).

Please refer to Tari Labs University for detailed discussions on
[layer 2 scaling solutions](https://tlu.tarilabs.com/layer2scaling/layer2scaling-landscape/layer2scaling-survey.html)
and
[consensus mechanisms](https://tlu.tarilabs.com/consensus-mechanisms/BFT-consensus-mechanisms-applications/Introduction.html).

### Interaction between the base layer and the DAN

The base layer provides supporting services to the digital asset network. In general, the base layer only knows about
Tari coin transactions. It knows nothing about the details of any digital assets and their state.

This is by design: The network cannot scale if details of digital asset contracts have to be tracked on the base layer.
We envisage that there could be tens of thousands of contracts deployed on Tari. Some of those contracts may be enormous;
imagine controlling every piece of inventory and their live statistics for a MMORPG. The base layer is also too slow. If
_any_ state relies on base layer transactions being confirmed, there is an immediate lag before that state change can be
considered final, which kills the liveness properties we seek for the DAN.

It's better to keep the two networks almost totally decoupled from the outset, and allow each network to play to its
strength.

That said, there are key interactions between the two layers. The base layer is a ledger and so it can be used as a
source of truth for the DAN to use as a type of registrar as well as final court of appeal in the case of consensus
disputes. This is what gives the DAN a secure fallback in case bad actors try to manipulate asset state by taking
advantage of its non-decentralisation.

These interactions require making provision for additional transaction types, in addition to payment and coinbase
transactions, that mark validator node registrations, contract collateral and so on.

The interplay between base layer and DAN is what incentivises every actor in the system to maintain an efficient and
well-functioning network even while acting in their own self-interests.


### Summary

Table 1 summarises the defining characteristics of the Tari network layers:

|                                      | Base layer       | DAN                    |
|:-------------------------------------|:-----------------|:-----------------------|
| Speed                                | Slow             | Fast                   |
| Scalability                          | Moderate         | Very high              |
| Security                             | High             | Mod (High w/ fallback) |
| Decentralisation                     | High             | Low - Med              |
| Processes Tari coin transactions     | Yes              | No                     |
| Processes digital asset instructions | Only checkpoints | Yes                    |


[Tari coin]: ../../Glossary.md#tari-coin
[transaction]: ../../Glossary.md#transaction
[instruction]: ../../Glossary.md#instructions
[Validator Nodes]: ../../Glossary.md#validator-node
[Mimblewimble]: ../../Glossary.md#mimblewimble
[UTXO]: ../../Glossary.md#unspent-transaction-outputs
