# RFC-0341: Asset registration
## Asset registration process

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: Philip Robinson <philipr-za>

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

This document will describe the process that an [Asset Issuer] will need to engage in to register a [digital asset] and commence its operation on the [digital asset network].

## Related RFCs
* [RFC-0311: Digital Asset templates](RFC-0311_AssetTemplates.md)
* [RFC-0302: Validator Nodes](RFC-0302_ValidatorNodes.md)
* [RFC-0304: Validator Node committee selection](RFC-0304_VNCommittees.md)
* [RFC-0220: Asset Checkpoints](RFC-0220_AssetCheckpoints.md)

## Description

### Abstract
This document will describe the process an [Asset Issuer] (AI) will go through in order to:
- Register a [digital asset] (DA) on the [base layer],
- assemble a [committee] of [Validator Node]s (VNs) and
- commence operation of the (DA) on the [Digital Asset Network] (DAN).

### Asset creation instruction
The first step in registering and commencing the operation of an asset is for the AI to issue an asset creation transaction to the [base layer].
This transaction will be time-locked for the length of the desired nomination period. This ensures that this transaction cannot be spent until the nomination period has elapsed so that it is present during the entire nomination process. The value of the transaction will be the `asset_creation_fee` described in [RFC-0311](RFC-0311_AssetTemplates.md). The AI will spend the transaction back to themselves but locking this fee up at this stage achieves 2 goals. Firstly, it makes it expensive to spam the network with asset creation transactions that a malicious AI does not intend to complete. Secondly, it proves to the VNs that participate in the nomination process that the AI does indeed have the funds required to commence operation of the asset once the committee has been selected. If the asset registration process fails, for example if there are not enough available VNs for the committee, then the AI can refund the fee to themselves after the time-lock expires.

The transaction will contain the following extra meta-data to facilitate the registration process:

1. The value of the transaction in clear text so that it can be verified by third parties.
2. The public key of the fee commitment which is required to verify the stated value is correct.
3. A commitment (hash) to the asset parameters as defined by a [DigitalAssetTemplate] described in [RFC-0311](RFC-0311_AssetTemplates.md). This template will define all the parameters of the asset the AI intends to register including information the VNs need to know like what the required [AssetCollateral] is to be part of the committee.

Once this transaction appears on the blockchain the nomination phase can begin.

### Nomination phase
The next step in registering an asset is for the AI to select a committee of VNs to manage the asset. The process to do this is described in [RFC-0304](RFC-0304_VNCommittees.md). This process lasts as long as the time-lock on the asset creation transaction described above. The VNs have until that time-lock elapses to nominate themselves (in the case of an asset being registered using the `committee_mode::PUBLIC_NOMINATION` parameter in the [DigitalAssetTemplate]).

### Asset commencement
Once the Nomination phase is complete and the AI has selected a committee as described in [RFC-0304](RFC-0304_VNCommittees.md) the chosen committee and AI are ready to commit their `asset_creation_fee` and [AssetCollateral]s to commence the operation of the asset. This is done by the AI and the committee members collaborating to build the initial [Checkpoint] of the asset. When this [Checkpoint] transaction is published to the [base layer] the [digital asset] will be live on the DAN. The [Checkpoint] transaction is described in [RFC_0220](RFC-0220_AssetCheckpoints.md).

[assetcollateral]: Glossary.md#assetcollateral
[asset issuer]: Glossary.md#asset-issuer
[base layer]: Glossary.md#base-layer
[checkpoint]: Glossary.md#checkpoint
[digital asset]: Glossary.md#digital-asset
[DigitalAssetTemplate]: Glossary.md#digitalassettemplate
[committee]: Glossary.md#committee
[CommitteeSelectionStrategy]: Glossary.md#committeeselectionstrategy
[validator node]: Glossary.md#validator-node
[digital asset network]: Glossary.md#digital-asset-network
[trusted node]: Glossary.md#trusted-node
