# RFC-0341: Asset Registration
## Asset Registration Process

![status: deprecated](theme/images/status-deprecated.svg)

**Maintainer(s)**: [Philip Robinson](https://github.com/philipr-za)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

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

The aim of this Request for Comment (RFC) is to  describe the process in which an [Asset Issuer] (AI) will need to engage to 
register a [Digital Asset] (DA) and commence its operation on the [Digital Asset Network] (DAN).

## Related Requests for Comment
* [RFC-0311: Digital Asset Templates](RFC-0311_AssetTemplates.md)
* [RFC-0302: Validator Nodes](RFCD-0302_ValidatorNodes.md)
* [RFC-0304: Validator Node Committee Selection](RFC-0304_VNCommittees.md)
* [RFC-0220: Asset Checkpoints](RFC-0220_AssetCheckpoints.md)

## Description

### Abstract
This document will describe the process through which an AI will go in order to:
- register a DA on the [base layer];
- assemble a [committee] of [Validator Node]s (VNs); and
- commence operation of the DA on the DAN.

### Asset Creation Instruction
The first step in registering and commencing the operation of an asset is that the AI MUST issue an asset creation 
transaction to the [base layer].

This transaction will be time-locked for the length of the desired nomination period. This ensures that this transaction 
cannot be spent until the nomination period has elapsed so that it is present during the entire nomination process. The 
value of the transaction will be the `asset_creation_fee` described in [RFC-0311](RFC-0311_AssetTemplates.md). The AI 
will spend the transaction back to themselves, but locking this fee up at this stage achieves two goals:

- Firstly, it makes 
  it expensive to spam the network with asset creation transactions that a malicious AI does not intend to complete.

- Secondly, it proves to the VNs that participate in the nomination process that the AI does indeed have the funds 
  required to commence operation of the asset once the committee has been selected.

If the asset registration process 
fails, e.g. if there are not enough available VNs for the committee, then the AI can refund the fee to themselves 
after the time lock expires.

The transaction will contain the following extra metadata to facilitate the registration process:

1. The value of the transaction in clear text and the public spending key of the commitment so that it can be verified 
   by third parties. A third party can verify the value of the commitment by using the information in (1) and (2) below, to calculate (3):
   1. The output commitment is $ C = k \cdot G + v \cdot H $.
   2. $ v​ $ and $ k \cdot G ​$ are provided in the metadata.
   3. A verifier can calculate $ C - k \cdot G = v \cdot H $ and verify this value by multiplying the clear text $ v $ by $ H $ themselves.

2. A commitment (hash) to the asset parameters as defined by a [DigitalAssetTemplate] described in 
  [RFC-0311](RFC-0311_AssetTemplates.md). This template will define all the parameters of the asset that the AI intends to 
  register, including information the VNs need to know, such as what  [AssetCollateral] is required to be part of the committee.

Once this transaction has been confirmed to the required depth on the blockchain, the nomination phase can begin.

### Nomination Phase
The next step in registering an asset is for the AI to select a committee of VNs to manage the asset. The process to do 
this is described in [RFC-0304](RFC-0304_VNCommittees.md). This process lasts as long as the time lock on the asset 
creation transaction described above. The VNs have until that time lock elapses to nominate themselves (in the case of 
an asset being registered using the `committee_mode::PUBLIC_NOMINATION` parameter in the [DigitalAssetTemplate]).

### Asset Commencement
Once the nomination phase is complete and the AI has selected a committee as described in [RFC-0304](RFC-0304_VNCommittees.md), 
the chosen committee and AI are ready to commit their `asset_creation_fee` and [AssetCollateral]s to commence the 
operation of the asset. This is done by the AI and the committee members collaborating to build the initial [Checkpoint] 
of the asset. When this [Checkpoint] transaction is published to the [base layer], the [digital asset] will be live on 
the DAN. The [Checkpoint] transaction is described in [RFC_0220](RFC-0220_AssetCheckpoints.md).

[assetcollateral]: Glossary.md#assetcollateral
[asset issuer]: Glossary.md#asset-issuer
[base layer]: Glossary.md#base-layer
[checkpoint]: Glossary.md#checkpoint
[committee]: Glossary.md#committee
[CommitteeSelectionStrategy]: Glossary.md#committeeselectionstrategy
[digital asset]: Glossary.md#digital-asset
[DigitalAssetTemplate]: Glossary.md#digitalassettemplate
[digital asset network]: Glossary.md#digital-asset-network
[trusted node]: Glossary.md#trusted-node
[validator node]: Glossary.md#validator-node
