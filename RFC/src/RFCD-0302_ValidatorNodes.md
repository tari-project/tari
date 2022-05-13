# RFC-0302/ValidatorNode

## Validator Nodes

![status: deprecated](theme/images/status-deprecated.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77) and [Philip Robinson](https://github.com/philipr-za)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2018 The Tari Development Community

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

The aim of this Request for Comment (RFC) is to describe the responsibilities of Validator Nodes (VNs) on the Digital 
Asset Network (DAN).

## Related Requests for Comment
* [RFC-0322: Validator Node Registration](RFC-0322_VNRegistration.md)
* [RFC-0304: Validator Node Committee Selection](RFCD-0304_VNCommittees.md)
* [RFC-0340: VN Consensus Overview](RFC-0340_VNConsensusOverview.md)

## Description
### Abstract
[Validator Node]s form the basis of the second-layer DAN. All actions on this network take place by interacting with VNs. 
Some examples of actions
that VNs will facilitate are:

* issuing a [Digital Asset] (DA);
* querying the state of a DA and its constituent [tokens]; and
* issuing an instruction to change the state of a DA or tokens.

VNs will also perform archival functions for the assets they manage. The lifetime of these archives and the fee structure 
for this function are
still being discussed.

#### Registration
VNs register themselves on the [Base Layer] using a special [transaction] type.

Validator node registration is described in [RFC-0322](RFC-0322_VNRegistration.md).

#### Execution of Instructions
VNs are expected to manage the state of DAs on behalf of DA issuers. They receive fees as reward
for doing this.

* DAs consist of an initial state plus a set of state transition rules. These rules are set by the Tari
  protocol, but will usually provide parameters that must be specified by the [Asset Issuer].
* The set of VNs that participate in managing state of a specific DA is called a [Committee]. A committee is selected 
during the asset
issuance process and membership of the committee can be updated at [Checkpoint]s.
* The VN is responsible for ensuring that every state change in a DA conforms to the contract's rules.
* VNs accept DA [Instructions] from clients and peers. Instructions allow for creating, updating, expiring and 
archiving DAs on the DAN.
* VNs provide additional collateral, called [AssetCollateral], when accepting an offer to manage an asset, which is 
stored in a multi-signature (multi-sig)
  Unspent Transaction Output (UTXO) on the base layer. This collateral can be taken from the VN if it is proven that the 
  VN engaged in
  malicious behaviour.
* VNs participate in fraud-proof validations in the event of consensus disputes (which could result in the malicious VN's
  collateral being slashed).
* DA metadata (e.g. large images) is managed by VNs. The large data itself will not be stored on the VNs, but 
in an external location, and a hash of the data can be stored. Whether the data is considered part of the state
(and thus checkpointed) or out of state depends on the type of DA contract employed.

#### Fees
Fees will be paid to VNs based on the amount of work they did during a checkpoint period. The fees will be paid from a 
fee pool, which will be collected
and reside in a UTXO that is accessible by the committee. The exact mechanism for the payment of the fees by the 
committee and who pays the various
types of fees is still under discussion.

#### Checkpoints
VNs periodically post checkpoint summaries to the [base layer] for each asset that they are managing. The checkpoints 
will form an immutable
record of the DA state on the base layer. There will be two types of checkpoints:
* An Opening Checkpoint (OC) will:
  * specify the members of the VN committee;
  * lock up the collateral for the committee members for this checkpoint period; and
  * collect the fee pool for this checkpoint period from the Asset Issuer into a multi-sig UTXO under the control of the 
  committee.
  This can be a top-up of the fees or a whole new fee pool.

* A Closing Checkpoint (CC) will:
  * summarize the DA state on the base layer;
  * release the fee payouts;
  * release the collateral for the committee members for this checkpoint period; and
  * allow for committee members to resign from the committee.

After a DA is issued, there will immediately be an OC. After a checkpoint period there will then be a 
CC, followed
immediately by an OC for the next period. We will call this set of checkpoints an Intermediate checkpoint, which could be a compressed combination of an OC and CC. This will continue
until the end of the asset's lifetime, when there will be a final CC that will be followed by the retirement of the asset.

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
Committees of VNs will use a [ConsensusStrategy] to manage the process of:
* propagating signed instructions between members of the committee; and
* determining when the threshold has been reached for an instruction to be considered valid.

Part of the Consensus Strategy will be mechanisms for detecting actions by [Bad Actor]s. The nature of the enforcement 
actions that can be taken
against bad actors is still to be decided.

### Network Communication
The VNs will communicate using a Peer-to-Peer (P2P) network. To facilitate this, the VNs must perform the following functions:
* VNs MUST maintain a list of peers, including which assets each peer is managing.
* VNs MUST relay [instructions] to members of the committee that are managing the relevant asset.
* VNs MUST respond to requests for information about DAs that they manage on the DAN.
* VNs and clients can advertise public keys to facilitate P2P communication encryption.

[assetcollateral]: Glossary.md#assetcollateral
[asset issuer]: Glossary.md#asset-issuer
[base layer]: Glossary.md#base-layer
[bad actor]: Glossary.md#bad-actor
[digital asset]: Glossary.md#digital-asset
[checkpoint]: Glossary.md#checkpoint
[committee]: Glossary.md#committee
[ConsensusStrategy]: Glossary.md#consensusstrategy
[validator node]: Glossary.md#validator-node
[transaction]: Glossary.md#transaction
[tokens]: Glossary.md#digital-asset-tokens
[instructions]: Glossary.md#instructions
