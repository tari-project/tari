# RFC-0300/DAN

## The Digital Assets Network

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
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as 
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
* [RFC-0311: Digital assets](RFC-0311_AssetTemplates.md)
* [RFC-0340: VN Consensus Overview](RFC-0340_VNConsensusOverview.md)
* [RFC-0302: Validator nodes](RFC-0302_ValidatorNodes.md)

## Description

### Abstract

[Digital asset]s (DAs) are managed by committees of special nodes called [Validator node]s (VNs). VNs manage digital asset state change and ensure
that the rules of the asset contracts are enforced.
* VNs form a peer-to-peer communication network that together defines the Tari [Digital Asset Network] (DAN)
* VNs register themselves on the [base layer] and commit collateral to prevent Sybil attacks.
* Scalability is achieved by sacrificing decentralisation. Not *all* VNs manage *every* asset. Assets are managed by
  subsets of the DAN, called VN [committees]. These committees reach consensus on DA state amongst themselves.
* VNs earn fees for their efforts.
* Digital asset contracts are not Turing complete, but are instantiated by [Asset Issuer]s (AIs) using Digital Asset templates that are defined
  in the DAN protocol code.

### Digital Assets

* Digital asset contracts are *not* Turing complete, but are selected from a set of [DigitalAssetTemplate]s that govern
  the behaviour of each contract type. e.g. there could be a Single-Use Token template for simple ticketing systems; a
  Coupon template for loyalty programmes and so on.
* The template system is intended to be highly flexible and additional templates can be added to the protocol periodically.
* Asset issuers can link a Registered Asset Issuer Domain (RAID) ID in an OpenAlias TXT public Domain Name System (DNS) 
  record to a Fully Qualified Domain Name (FQDN) that they own. This is to help disambiguate similar
  contracts and improve the signal-to-noise ratio from scam- or copy-cat contracts.

An [Asset Issuer] (AI) will issue a Digital Assets by constructing a contract from one of the supported set of [DigitalAssetTemplate]s. The AI will choose
how large the committee of Validator Nodes will be for this DA and have the option to nominate [Trusted Node]s to be part of the VN committee for the DA.
Any remaining spots on the committee will be filled by permissionless VNs that are selected according to a [CommitteeSelectionStrategy]. This is a strategy
for the DAN to algorithmically select candidates for the committee from the available registered Validator Nodes. The VNs will need to accept the nomination
to become part of the committee by putting up the specified collateral.

[asset issuer]: Glossary.md#asset-issuer
[base layer]: Glossary.md#base-layer
[digital asset]: Glossary.md#digital-asset
[committees]: Glossary.md#committee
[CommitteeSelectionStrategy]: Glossary.md#committeeselectionstrategy
[validator node]: Glossary.md#validator-node
[digital asset network]: Glossary.md#digital-asset-network
[trusted node]: Glossary.md#trusted-node
[DigitalAssetTemplate]: Glossary.md#digitalassettemplate
