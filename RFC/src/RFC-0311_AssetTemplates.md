# RFC-0311/AssetTemplates

## Digital Asset templates

![status: draft](https://github.com/tari-project/tari/raw/master/RFC/src/theme/images/status-draft.svg)

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

Describe the Tari Digital Asset templating system for smart contract definition.

The term “smart contracts” in this document is used to refer to a set of rules enforced by computers. These smart
contracts are not Turing complete, such as those executed by the Ethereum VM.


## Related RFCs

* [RFC-0300: The Digital Assets Network](RFC-0300_DAN.md)
* [RFC-0301: Namespace registration](RFC-0301_NamespaceRegistration.md)
* [RFC-0340: Validator Node Consensus](RFC-0340_VNConsensusOverview.md)
* [RFC-0374: Asset Creation and management](RFC-0374_TCAssetManagement.md)

## Description

### Motivation

The reasons for issuing assets on Tari under a templating system, rather than a scripting language (whether Turing
complete or not) are manifold:

* A scripting language, irrespective of how simple it is, limits the target market for asset issuers to developers, or
  people who pay developers.
* The market doesn’t want general smart contracts. This is evidenced by the fact that the vast majority of Ethereum
  transactions go through ERC-20 or ERC-721 contracts, which are literally contract templates.
* The attack surface for smart contracts is reduced considerably; to the node software itself.
* Bugs can be fixed for all contracts simultaneously by using a template versioning system. Existing assets can opt-in
  to fixes by migrating assets to a new version of the contract.
* Contracts will have better QA since more eyes are looking at fewer contract code sets.
* Transmission, storage and processing of contracts will be more efficient as one only has to deal with the parameters
  and not the logic of the contract. Furthermore, the cost for users is usually lower, since there's no need to add
  friction / extra costs to contract execution (e.g. ethereum gas) to work around the halting problem.

### Implementation

Assets are created on the Tari network by issuing a `create_asset` instruction from a wallet or client and broadcasting
it to the Tari Digital Assets Network (DAN).

The instruction is in JSON format and MUST contain the following fields:
(PKH = Public Key Hash)

| Name                               | Type          | Description                                                                                                                 |
|:-----------------------------------|:--------------|:----------------------------------------------------------------------------------------------------------------------------|
| **Asset description**              |               |                                                                                                                             |
| issuer                             | PKH           | The public key of the creator of the asset. See [issuer](#issuer)                                                           |
| name                               | `string[64]`  | The name or identifier for the asset. See [Name and Description](#name-and-description)                                     |
| description                        | `string[216]` | A short description of the asset - with name, fits in a tweet. See [Name and Description](#name-and-description)            |
| raid                               | `string[15]`  | The [Registered Asset Issuer Domain (RAID_ID)](#raid-id) for the asset.                                                     |
| template_id                        | `u64`         | The template descriptor. See [Template ID](#template-id)                                                                    |
| asset_expiry                       | `u64`         | A timestamp or block height after which the asset will automatically expire. Zero for arbitrarily long-lived assets         |
| **Validation Committee selection** |               |                                                                                                                             |
| committee_mode                     | `u8`          | The validation committee mode, either `CREATOR_ASSIGNED` (0) or `NETWORK_ASSIGNED` (1)                                      |
| committee_parameters               | Object        | See [Committee Parameters](#committee-parameters).                                                                          |
| asset_creation_fee                 | `u64`         | The fee the issuer is paying, in microTari, for the asset creation process                                                  |
| commitment                         | `u256`        | A time-locked commitment for the asset creation fee                                                                         |
| initial_state_hash                 | `u256`        | The hash of the canonical serialisation of the initial template state (of the template-specific data)                       |
| initial_state_length               | `u64`         | Size in bytes of initial state                                                                                              |
| **template-specific data**         | Object        | Template-specific metadata can be defined in this section                                                                   |
| **Signatures**                     |               |                                                                                                                             |
| metadata_hash                      | `u256`        | A hash of the previous three sections' data, in canonical format (`m`)                                                      |
| creator_sig                        | `u256`        | A digital signature of the message `H(R ‖ P ‖ m)` using the asset creator’s private key corresponding to the `issuer` PKH |
| raid_signature                     | `u256`        | Digital signature for the given RAID, signing the message `H(R ‖ P ‖ RAID_ID ‖ m)`                                       |
| commitment_sig                     | `u256`        | A signature proving the issuer is able to spend the commitment to the asset fee                                             |

#### Committee parameters

If `committee_mode` is `CREATOR_ASSIGNED` the `committee_mode` object is

| Name             | Type         | Description |
|:-----------------|:-------------|:------------|
| trusted_node_set | Array of PKH | See below   |

Only the nodes in the trusted node set will be allowed to execute instructions for this asset.

If `committee_mode` is `NETWORK_ASSIGNED` the `committee_mode` object is

| Name                    | Type  | Description                                                                                                           |
|:------------------------|:------|:----------------------------------------------------------------------------------------------------------------------|
| node_threshold          | `u32` | The required number of validator nodes that must register to execute instructions for this asset                      |
| minimum_collateral      | `u64` | The minimum amount of Tari a validator node must put up in collateral in order to execute instructions for this asset |
| node_selection_strategy | `u32` | The selection strategy to employ allowing nodes to register to manage this asset                                      |


#### Issuer

Anyone can create new assets on the Tari network from their Tari Collections client. The client will provide the PKH and
sign the instruction. The client needn’t use the same private key each time.

#### Name and Description

These fields are purely for informational purposes and do not need to be unique, and do not act as an asset ID.

#### RAID ID

The RAID_ID is a 15 character string that associates the asset issuer with a registered internet domain name on DNS.

If it is likely that a digital asset issuer will be issuing many assets on the Tari Network (hundreds, or thousands),
the issuer should strongly consider using a registered domain (e.g. `acme.com`). This is
done via OpenAlias on the domain owner's DNS record, as described in [RFC-0301](RFC-0301_NamespaceRegistration.md). A
 RAID prevents spoofing of assets from copycats or other malicious actors. It also makes asset discovery
simpler.

Assets from issuers that do not have a RAID are all grouped under the default RAID.

RAID owners must provide a valid signature proving that they own the given domain when creating assets.

#### Asset identification

Assets are identified by a 64-character string that uniquely identifies an asset on the network:

| Bytes | Description                                 |
|:------|:--------------------------------------------|
| 8     | Template type (hex)                         |
| 4     | Template version (hex)                      |
| 4     | Feature flags (hex)                         |
| 15    | RAID identifier (Base58)                    |
| 1     | A period character, `.`                     |
| 32    | Hash of `create_asset` instruction (Base58) |

This allows assets to be deterministically identified from their initial state. Two different creation instructions
leading to the same hash refer to the same single asset, by definition. Validator Nodes maintain an index of assets and
their committees, and so can determine whether a given asset already exists; and MUST reject any `create_asset`
instruction for an existing asset.

#### Template ID

Tari uses templates to define the behaviour for its smart contracts. The template id refers to the type of digital asset
begin created.

**Note:** Integer values are given in _little-endian_ format; i.e. the least significant bit is _first_.

The template number is a 64-bit unsigned integer and has the following format, with 0 representing
the least significant bit.

| Bit range | Description                       |
|:----------|:----------------------------------|
| 0 - 31    | Template type (0 - 4,294,967,295) |
| 32 - 47   | Template version (0 - 65,535)     |
| 48        | Beta Mode flag                    |
| 49        | Confidentiality flag              |
| 50 - 63   | Reserved  (Must be 0)             |

The lowest 32 bits refer to the canonical smart contract type; the qualitative types of contracts the network supports.
Many assets can be issued from a single template.

Template types below 65,536 (2<sup>16</sup>) are public, community-developed templates.
All validator nodes MUST implement and be able to interpret instructions related to these templates.

Template types 65,536 and above are opt-in or proprietary templates. There is no guarantee that any given validator node
will be able to manage assets on these templates. Part of the committee selection and confirmation process for new
assets will be an attestation by validator nodes that they are willing and able to manage the asset under the designated
template rules.

A global registry of opt-in template types will be necessary to prevent collisions (public templates existence will be
evident from the Validator Node source code), possibly implemented as a special transaction type on the base layer; the
base layer being perfectly suited for hosting such a registry. The details of this will be covered in a separate
proposal.

Examples of template types may be

| Template type | Asset                    |
|:--------------|:-------------------------|
| 1             | Simple single-use tokens |
| 2             | Simple coupons           |
| 20            | ERC-20-compatible        |
| ...           | ...                      |
| 120           | Collectible Cards        |
| 144           | In-game items            |
| 721           | ERC-721-compatible       |
| ...           | ...                      |
| 65,537        | Acme In game items       |
| 723,342       | CryptoKitties v8         |

The template id may also set one or more feature flags to indicate that the contract is
* experimental, or in testing phase, (bit 48).
* confidential. The definition of confidential assets and their implementation is not finalised at the time of writing.

Wallets / client apps SHOULD have settings to allow or otherwise completely ignore asset types on the network that have
certain feature flags enabled. For instance, most consumer wallets should never interact with templates that have the
“Beta mode” bit set. Only developer’s wallets should ever even see that such assets exist.

#### Asset expiry

Asset issuers can set a future expiry date or block height after which the asset will expire and nodes will be free to
expunge any/all state relating to the asset from memory after a fixed grace period. The grace period is to allow
interested parties (e.g. the issuer) to take a snapshot of the final state of the contract if they wish (e.g. proving
that you had a ticket for that epic World Cup final game, even after the asset no longer exists on the DAN).

Nodes will publish a final checkpoint on the base layer soon after expiry and before purging an asset.

The expiry_date is a Unix epoch, representing the number of seconds since 1 January 1970 00:00:00 UTC if the value is
greater than 1,500,000,000; or a block height if it is less than that value (with 1 min blocks this scheme is valid
until the year 4870).

Expiry times should not be considered exact, since nodes don’t share the same clocks and block heights as time proxies
become more inaccurate the further out you go (since height in the future is dependent on hash rate).
