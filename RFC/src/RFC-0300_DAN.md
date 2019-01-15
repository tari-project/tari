# The Digital Assets Network
## An overview of the Tari network

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77), [Philip Robinson](https://github.com/philipr-za)

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
[BCP 14](https://tools.ietf.org/html/bcp14) [RFC2119] [RFC8174] when, and only when, they appear in all capitals, as 
shown here.
      
## Disclaimer

The purpose of this document and its content is for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

The goal of this RFC is to describe the key features of the Tari second layer, a.k.a the Digital Assets Network (DAN)

## Related RFCs

* [RFC-0100: Base layer](RFC-0100_BaseLayer.md)
* [RFC-0310: Digital assets](RFC-0310_Assets.md)
* [RFC-0311: Second layer consensus strategies](RFC-3011_SecondLayerConsensus.md)
* [RFC-0312: Validator nodes](RFC-3012_ValidatorNodes.md)

## Description

### Abstract

[Digital asset]s (DAs) are managed by committees of special nodes called [Validator node]s (VNs). VNs manage digital asset state change and ensure
that the rules of the asset contracts are enforced.
* VNs form a peer-to-peer communication network that together defines the Tari [Digital Asset Network] (DAN)
* VNs register themselves on the base layer and commit collateral to prevent Sybil attacks.
* Scalability is achieved by sacrificing decentralisation. Not *all* VNs manage *every* asset. Assets are managed by
  subsets of the DAN, called VN committees. These committees reach consensus on DA state amongst themselves.
* VNs earn fees for their efforts.
* Digital asset contracts are not Turing complete, but are instantiated by [Asset Issuer]s (AIs) using Digital Asset templates that are defined
  in the DAN protocol code.

### Digital Assets

* Digital asset contracts are *not* Turing complete, but are selected from a set of [DigitalAssetTemplate]s that govern
  the behaviour of each contract type. e.g. there could be a Single-Use Token template for simple ticketing systems; a
  Coupon template for loyalty programmes and so on.
* The template system is intended to be highly flexible and additional templates can be added to the protocol periodically.
* Asset issuers can register Top-Level Digital Issuer (TLDI) names on the base chain to help disambiguate similar
  contracts and improve the signal-to-noise ratio from scam- or copy-cat contracts.

An [Asset Issuer] (AI) will issue a Digital Assets by constructing a contract from one of the supported set of [DigitalAssetTemplate]s. The AI will choose
 how large the committee of Validator Nodes will be for this DA and have the option to nominate [Trusted Node]s to be part of the VN committee for the DA.
Any remaining spots on the committee will be filled by permissionless VNs that are selected according to a [CommitteeSelectionStrategy]. This is a strategy
for the DAN to algorithmically select candidates for the committee from the available registered Validator Nodes. The VNs will need to accept the nomination
to become part of the committee by putting up the specified collateral.

### Validator Nodes

[Validator Node]s form the basis of the second layer DAN. All actions on this network take place by interacting with VN's. Some examples of actions
that VNs will facilitate are:
* Issuing a [Digital Asset],
* querying the state of [Digital Asset] and its constituent [tokens],
* issuing an instruction to change the state of a [Digital Asset] or [tokens].

VNs will also perform archival functions for the assets they manage. The lifetime of these archives and the fee structure for this function is
still being discussed.

#### Registration
VNs register themselves on the [Base Layer] using a special [transaction] type. The registration [transaction] type
requires the spending of a certain minimum amount of [Tari coin], the ([RegistrationCollateral]), that has a time-lock on the
output for a minimum amount of time ([RegistrationTerm]) as well as some metadata, such as the VNs public key and a generated Node ID. The Node ID is generated
during registration to prevent mining of VN public keys that can be used to manipulate routing on the DAN. The blinding factor for the Registration transaction is the private key
that the VN node will use to sign every instruction that it executes for the duration of its [RegistrationTerm].

Once a VNs [RegistrationTerm] has expired so will this specific VN registration. The UTXO timelock will have elapsed so the [RegistrationCollateral] can be reclaimed and a new VN registration
need to be performed. This automatic registration expiry will ensure that the VN registry stays up to date with active VN registrations and inactive registrations will naturally be removed.

Requiring nodes to register themselves serves two purposes:
* Makes VN Sybil attacks expensive,
* Provides an authoritative "central-but-not-centralised" registry of validator nodes from the base layer.

#### Execution of instructions
VNs are expected to manage the state of digital assets on behalf of digital asset issuers. They receive fees as reward
for doing this.
* Digital assets consist of an initial state plus a set of state transition rules. These rules are set by the Tari
  protocol, but will usually provide parameters that must be specified by the [Asset Issuer].
* The set of VNs that participate in managing state of a specific DA is called a [Committee]. A committee is selected during the asset
issuance process and membership of the committee can be updated at [Checkpoint]s.
* It is the VNs responsibility to ensure that every state change in a digital asset conforms to the contract's rules.
* VNs accept digital asset [Instructions] from clients and peers. [Instructions] allow for creating, updating, expiring and archiving digital assets on the DAN.
* VNs provide additional collateral when accepting an offer to manage an asset, which is stored in a multi-signature
  UTXO on the base layer. This collateral can be taken from the VN if it is proven that the VN engaged in
  malicious behaviour.
* VNs participate in fraud proof validations in the event of consensus disputes (which could result in the malicious VN's
  collateral being slashed).
* Digital asset metadata (e.g. large images) are managed by VNs. The large data itself will not be stored on the VNs but an external location and a hash of the data can be stored. Whether the data is considered part of the state
  (and thus checkpointed) or out of state depends on the type of digital asset contract employed.

#### Fees
Fees will be paid to VNs based on the amount of work they did during a checkpoint period. The fees will be paid from a fee pool which will be collected
and reside in a UTXO that is accessible by the committee. The exact mechanism for the the payment of the fees by the committee and who pays the various
types of fees is still under discussion.

#### Checkpoints
VNs periodically post checkpoint summaries to the [base layer] for each asset that they are managing. The checkpoints will form an immutable
record of the [Digital Asset] state on the base-layer. There will be two types of checkpoints:
* An Opening checkpoint (OC) will:
  * Specify the members of the VN committee.
  * Lock up the collateral for the committee members for this checkpoint period.
  * Collect the fee pool for this checkpoint period from the Asset Issuer into a Multi-Sig UTXO under the control of the committee.
  This can be a top-up of the fees or a whole new fee pool.

* A Closing checkpoint (CC) will:
  * Summarize the Digital Asset state on the base layer.
  * Release the fee pay outs.
  * Release the collateral for the committee members for this checkpoint period.
  * Allow for committee members to resign from the committee

After an DA is issued there will immediately be an Opening checkpoint. After a checkpoint period there will then be a Closing checkpoint followed
immediately by an Opening checkpoint for the next period, we will call this set of checkpoints an Intermediate checkpoint, which could be a compressed combination of an opening and closing checkpoint. This will continue
until the end of the asset's lifetime where there will be a final Closing checkpoint that will be followed by the retirement of the asset.

<div class="mermaid">
graph LR;
    subgraph Asset Issuance
    IssueAsset-->OC1;
    end
    OC1-->CC1;
    subgraph Intermediate Checkpoint
    CC1-->OC2;
    end
    OC2-->CC2;
    subgraph Intermediate Checkpoint
    CC2-->OC3;
    end
    OC3-->CC3;
    subgraph Asset Retirement
    CC3-->RetireAsset;
    end

</div>

#### Consensus
Committees of VNs will use a [ConsensusStrategy] to manage the process of
* Propogating signed instructions between members of the committee.
* Determining when the threshold has been reached for an instruction to be considered valid.

Part of the [ConsensusStrategy] will be mechanisms for detecting actions by [Bad Actor]s. The nature of the enforcement actions that can be taken
against bad actors are still to be decided.

### Network communication
The VNs will communicate using a peer-to-peer (P2P) network. To facilitate this this VNs must perform the following functions:
* VNs MUST maintain a list of peers, and which assets each peer is managing.
* VNs MUST relay [instructions] to members of the committee that are managing the relevant asset.
* VNs MUST respond to requests for information about digital assets that they manage on the DAN.
* VNs and clients can advertise public keys to facilitate P2P communication encryption

[asset issuer]: Glossary.md#asset-issuer
[base layer]: Glossary.md#base-layer
[bad actor]: Glossary.md#bad-actor
[digital asset]: Glossary.md#digital-asset
[checkpoint]: Glossary.md#checkpoint
[committee]: Glossary.md#committee
[CommitteeSelectionStrategy]: Glossary.md#committeeselectionstrategy
[ConsensusStrategy]: Glossary.md#consensusstrategy
[validator node]: Glossary.md#validator-node
[digital asset network]: Glossary.md#digital-asset-network
[transaction]: Glossary.md#transaction
[tari coin]: Glossary.md#tari-coin
[tokens]: Glossary.md#digital-asset-tokens
[trusted node]: Glossary.md#trusted-node
[instructions]: Glossary.md#instructions
[RegistrationCollateral]: Glossary.md#registrationcollateral
[RegistrationTerm]: Glossary.md#registrationterm
[DigitalAssetTemplate]: Glossary.md#digitalassettemplate
