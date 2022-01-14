# RFC-0312/DANSpecification

## High level Digital Asset Network Specification

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

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

This document describes the high level specification for how digital assets are created, managed, secured, and wound-
down on the Tari digital asset network (DAN).

The document covers, among other things:

* The relationship of side-chains to digital assets,
* Required characteristics of side-chains,
* Peg-in and peg-out mechanisms,
* Digital asset template minimum requirements,
* Validator node requirements,
* Checkpoint and refund mechanisms

This RFC covers a lot of ground. Therefore the intent is not to provide a detailed, code-ready specification for the
entire DAN infrastructure; those are left to other RFCs; but to establish a foundation onto which the rest of the DAN
specifications can be built.

This RFC supersedes and deprecates several older RFCs:
  - [RFC-0300: Digital Assets Network](RFCD-0300_DAN.md)
  - [RFC-0301: Namespace Registration](RFCD-0301_NamespaceRegistration.md)
  - [RFC-0302: Validator Nodes](RFCD-0302_ValidatorNodes.md)
  - [RFC-0304: Validator Node committee selection](RFCD-0304_VNCommittees.md)
  - [RFC-0345: Asset Life cycle](RFC-0345_AssetLifeCycle.md)

Several RFC documents are in the process of being revised in order to fit into this proposed framework:

* [RFC-0300: The Digital Assets Network](RFCD-0300_DAN.md)
* [RFC-0340: Validator Node Consensus](RFC-0340_VNConsensusOverview.md)

### Motivation
There are many ways to skin a cat.
The philosophy guiding the approach in the RFC is one that permits
scaling of the network to handle in the region of **1 billion messages per day** network-wide and
**1 million digital assets** with **near real-time user experience** on asset state retrieval, updating and transfer,
on a sufficiently decentralised and private basis.

The definition of _sufficient_ here is subjective, and part of the design philosophy of Tari is that we leave it up to the
user to determine what that means, keeping in mind that there is always a trade-off between decentralisation, performance,
and cost.

For some assets, decentralisation and censorship resistance will be paramount, and users will be willing to live with a
more laggy experience. Gamers in a Web 3.0-MMORPG on the other hand, want cheap, fast transactions with verifiable ownership, and
therefore will generally be happy to sacrifice decentralisation for that.

The goal of the DAN is for asset issuers to be able to configure the side-chain for their project to suit their particular
needs.

## Description

### The role of the Layer 1 base chain

The Tari Overview RFC describes [the role of the base layer](./RFC-0001_overview.md#the-role-of-the-base-layer).
In summary, the base layer maintains the integrity of the Tari cryptocurrency token, maintains a register of side-chains
and maintains a register of Validator nodes.

It does not know about or care about what happens in the side chains as long as the Tari consensus, side-chain and
validator node rules are kept.


### The (side-chain)-(contract)-(validator node) relationship

Every contract MUST be governed by one, and only one, Tari side-chain.

Every digital asset MUST be governed by a single smart contract. This contract can be very simple or highly complex.

Side-chains MUST be initiated by Asset Issuers at the time of contract creation.

The side-chain consensus MUST be maintained by one or more Validator Nodes.

### Asset accounts

Tari uses the UTXO model in its ledger accounting. On the other hand Tari side-chains SHOULD use an account-based system
to track balances and state.

The reasons for this are:
* An account-based approach leads to fewer outputs on peg-in transactions. There is roughly a 1:1 ratio of users
  to balances in an account-based system. On the other hand there are O(n) UTXOs in an output-based system where `n` are
  the number of transactions carried out on the side-chain. When a side-chain wants to shut down, they must record a new
  output on the base layer for every account or output (as the case may be) that they track in the peg-out transaction(s).
  It should be self-evident that account-based systems are far scalable in the vast majority of use-cases.
* Following on from this, Accounts scale better for micro-payment applications, where hundreds or thousands of tiny payments
  flow between the same two parties.
* Many DAN applications will want to track state (such as NFTs) as well as currency balances. Account-based ledgers make
  this type of application far simpler.

Tari side-chain accounts MUST be representable as, or convertible to, a valid base layer UTXO.

When a side-chain pegs out, either partially (when users withdraw funds) or completely (when the side-chain shuts down),
all balances MUST be returned to the base layer in a manner that satisfies the Tari base layer consensus rules.

#### Pedersen commitments and account-based ledgers

Standard Pedersen commitments are essentially useless in account-based ledgers.

The reason being that since the spending keys would be common to all transactions involving a given account, it is trivial
to use the accounting rules to cancel out the `k.G` terms from transactions and to use a pre-image attack to unblind all
the values.

An alternative approach is to rather maintain a map of public keys to a single-balance UTXOs. Thus the primary side-chain
ledger would be a map of

$$
  k_i \cdot J = J_i \Rightarrow
  \begin{cases}
        j, \; \text(account update counter) \\\\
        C_j = k_j \cdot G + v_j \cdot H, \\\\
        d_j = \text{data}_j  
  \end{cases}
$$

where $J_i$ represents the i-th account public key, and the private key is known only by the account holder,
$C_j$ is balance for account _i_ after the j-th update. There is also an associated field $d_j$ that holds arbitrary
state for the account after the j-th update.

Updates to an account from a transaction involving _n_ accounts require

$$
\sum_{i, \text{before}}^n C_i + \mathrm{fee} = \sum_{i, \text{after}}^n C_i + \text{excess}
$$





### Side-chain architecture



### Validator Node registration requirements

### Contract creation flow

#### The owner collects data

#### Metadata specification

#### Owner Collateral

#### Peg-out transaction

#### Peg-in transaction

#### Side-chain funding transaction

#### Refund transaction

#### Checkpoint transaction

#### Slashing authorised signers

#### Updating authorised signers

#### Contract-creation time-out

#### DAN contract template specification

[RFC-0001]: RFC-0001_overview.md