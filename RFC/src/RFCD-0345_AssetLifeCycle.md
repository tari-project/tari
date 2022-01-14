# RFC-0345/AssetLifeCycle

## Asset Life Cycle

![status: deprecated](theme/images/status-deprecated.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019. The Tari Development Community.

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
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

## Related Requests for Comment


* [RFC-0300: The Digital Assets Network](RFCD-0300_DAN.md)
* [RFC-0301: Namespace Registration](RFCD-0301_NamespaceRegistration.md)
* [RFC-0311 Asset templates](RFC-0311_AssetTemplates.md)
* [RFC-0340: Validator Node Consensus](RFC-0340_VNConsensusOverview.md)

## Description

### Introduction

Tari digital assets are created on the [Digital Assets Network](RFCD-0300_DAN.md), managed by a Validator Node (VN)
committee, see [RFC-0340](RFC-0340_VNConsensusOverview.md), and are operated under the rules of the
[template](RFC-0311_AssetTemplates.md) that governs the asset.

A _given version_ of a template provides an immutable definition of the type of information (the state) that is managed
during the lifetime of the asset. _All_ the rules that govern reading and updating of the asset state, and rules
regarding any transfers of tokens that may be defined as part of the asset are governed by the template code.

This immutability is what allows VN committees to reach rapid consensus about what an asset’s state looks  
like after [instructions] are executed.

However, immutability is a major problem when faced with typical software and business development challenges such as
software bugs, or changes in legal and operational requirements.

The Tari network is designed to accommodate these requirements by offering a migration path for assets from one version
of a template to another version of a template.

### Asset migration
To carry out a successful migration, the following requirements must be met:

* The validator node committee for the asset must support the migration path. This entails that _every_ VN in the
  committee has a `MigrateAsset` class from the `template_type.version` combination of the existing asset to the
  `template_type.version` of the new asset.
* The original asset issuer provides a valid `migrate_asset` instruction to the DAN.
  * The asset issuer MUST provide any additional state that exists in the new template version and not the original.
  * The original asset SHOULD be marked as `retired`. If so, the `superseded_by` field in the old asset will carry the
    new asset id once the new asset has been confirmed. We recommend retiring the old asset because all the keys that
    indicate ownership of tokens will be copied over; effectively re-using them; which can damage privacy.
  * A policy is provided to determine the course of action to follow if any state from the old asset is illegal under
    the new template rules (e.g. If a new rule requires `token.age` to be > 21; what happens to any tokens where this
    condition fails?)


As part of the migration,

1. An entirely new asset is created with the full state of the old asset copied over into the new asset; supplemented
   with any additional state required in the new template.
2. A state validation run is performed; and any invalid state from the old asset is modified according to the migration
  policy.
3. Step 2 is repeated until a valid initial state is produced.
4. If Step 2 has run `STATE_VALIDATION_LIMIT` times and the initial state is still not valid, the migration instruction
   is rejected; the migrate_asset instruction will advise what should be done with the original asset in this case:
   either allow the original asset to continue as before, or retire it.
5. Once a valid initial state is produced, a new `create_asset` instruction is generated from the initial
   state and the `migrate_asset` instruction. Typically the same VN committee will be used for the new asset, but this
   needn’t be the case.

Once a successful migration has completed, any instructions to the old asset can return a simple `asset_migrated`
response with the new asset ID, which will allow clients and wallets to seamlessly update their records.

### Retiring Assets

Retiring an asset follows the same procedure as when as asset reaches its natural end-of-life: A final checkpoint is
posted to the base layer and a grace period is given to allow DAN nodes and clients to take a snapshot of the final
state if desired.

### Resurrecting assets

It’s unreasonable to expect VNs to hold onto large chunks of state for assets that are effectively dead (e.g. ticket
stubs long after the event is over). For this reason, assets are allowed to expire after which VNs can forget about the
state and use that storage for something else.

However, it may be that interest in an asset resurfaces long after the asset expires (nostalgia being the multi-billion
dollar industry it is today). The `resurrect_asset` instruction provides a mechanism to bring an asset back to life.

To resurrect an asset, the following conditions must be met:

* The asset must have expired.
* It must not be currently active (i.e. it hasn’t already been resurrected).
* An asset issuer (not necessarily the original asset issuer) must provide funding for the new lifetime of the asset.
* The asset issuer needs to have a copy of the state corresponding to the final asset checkpoint of the original asset.
* The new asset issuer transmits a `resurrect_asset` instruction to the network. This instruction is identical to the
  original `create_asset` instruction with the following exceptions:
   *   The “initial state” merkle root must be equal to the final state checkpoint merkle root.
   *  The asset owner public key will be provided by the new asset issuer.
*  Third parties can interrogate the new committee asking them to provide a Merkle Proof for pieces of state that the
   third party (perhaps former asset owners) knows about. This can mitigate fraud attempts where parties can attempt to
   resurrect assets without actually having a copy of the smart contract state. If enough random state proofs are
   requested, or a single proof of enough random pieces of state, we can be confident that the asset resurrection is
   legitimate.

The VN committee for the resurrected asset need not bear any relation to the original VN committee.
Once confirmed, the resurrected asset operates exactly like any other asset.
An asset can expire and be resurrected multiple times (sequentially).

[instructions]: Glossary.md#instructions

