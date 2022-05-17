# RFC-0304/VNCommittees

## Validator Node Committee Selection

![status: deprecated](theme/images/status-deprecated.svg)

**Maintainer(s)**: [Philip Robinson](https://github.com/philipr-za)

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

The aim of this Request for Comment (RFC) is to describe the process an [Asset Issuer] (AI) will go through in order to 
select the committee of [Validator Node]s
(VNs) that will serve a given [Digital Asset] (DA).

## Related Requests for Comment
* [RFC-0311: Digital Asset Templates](RFC-0311_AssetTemplates.md)
* [RFC-0302: Validator Nodes](RFCD-0302_ValidatorNodes.md)
* [RFC-0341: Asset Registration](RFC-0341D_AssetRegistration.md)

## Description

### Abstract
[Digital Asset]s (DAs) are managed by [committee]s of nodes called [Validator Node]s (VNs), as described in 
[RFC-0300](RFCD-0300_DAN.md) and [RFC-0302](RFCD-0302_ValidatorNodes.md). During the asset creation process, described in
[RFC-0341](RFC-0341D_AssetRegistration.md), the [Asset Issuer] (AI) MUST select a committee of VNs to manage their asset.
This process consists of the following steps:

1. Candidate VNs MUST be nominated to be considered for selection by the AI.
2. The AI MUST employ a [CommitteeSelectionStrategy] to select VNs from the set of nominated candidates.
3. The AI MUST make an offer of committee membership to the selected VNs.
4. Selected VNs MAY accept the offer to become part of the committee by posting the required [AssetCollateral].

### Nomination
The first step in assembling a committee is to nominate candidate VNs. As described in 
[RFC-0311](RFC-0311_AssetTemplates.md), an asset can be created with two possible `committee_modes` - `CREATOR_NOMINATION` 
or `PUBLIC_NOMINATION`:

- In `CREATOR_NOMINATION` mode, the AI nominates candidate committee members directly. The AI will have a list of permissioned 
  [Trusted Node]s that they want to act as the committee. The AI will contact the candidate VNs directly to inform them of 
  their nomination.
- In `PUBLIC_NOMINATION` mode, the AI does not have a list of [Trusted Node]s and wants to source unknown VNs from the 
  network. In this case, the AI broadcasts a public call for nomination to the Tari network using the peer-to-peer messaging 
  protocol described in [RFC-0172](RFC-0172_PeerToPeerMessagingProtocol.md). This call for nomination contains all the 
  details of the asset. VNs that want to participate will then nominate themselves by contacting the AI.

### Selection
Once the AI has received a list of nominated VNs, it must make a selection, assuming enough VNs were nominated to populate 
the committee. The AI will employ some [CommitteeSelectionStrategy] in order to select the committee from the candidate 
VNs that have been nominated. This strategy might aim for a perfectly random selection, or perhaps it will consider some 
metrics about the candidate VNs, such as the length of their VN registrations. These metrics might indicate that they are reliable 
and have not been denylisted for poor or malicious performance.

A consideration when selecting a committee in `PUBLIC_NOMINATION` mode will be the size of the pool of nominated VNs. 
The size of this pool relative to the size of the committee to be selected will be linked to a risk profile. If the pool 
contains very few candidates, then it will be much easier for an attacker to have nominated their own nodes in order to 
obtain a majority membership of the committee. For example, if the AI is selecting a committee of 10 members using a uniformly 
random selection strategy and only 12 public nominations are received, an attacker only requires control of six VNs to 
achieve a majority position in the committee. In contrast, if 100 nominations are received and the AI performs a 
uniformly random selection, an attacker would need to control more than 50 of the nominated nodes in order to achieve a 
majority position in the committee.

### Offer Acceptance
Once the selection has been made by the AI, the selected VNs will be informed and they will be made an offer of membership. 
If the VNs are still inclined to join the committee, they will accept the offer by posting the [AssetCollateral] 
required by the asset to the [base layer] during the initial [Checkpoint] transaction built to commence the operation of 
the asset.

[assetcollateral]: Glossary.md#assetcollateral
[asset issuer]: Glossary.md#asset-issuer
[base layer]: Glossary.md#base-layer
[checkpoint]: Glossary.md#checkpoint
[digital asset]: Glossary.md#digital-asset
[committee]: Glossary.md#committee
[CommitteeSelectionStrategy]: Glossary.md#committeeselectionstrategy
[validator node]: Glossary.md#validator-node
[digital asset network]: Glossary.md#digital-asset-network
[trusted node]: Glossary.md#trusted-node
