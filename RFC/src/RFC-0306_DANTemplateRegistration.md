# RFC-0306/DANTemplateRegistration

## Digital Asset Network (DAN) Template Registration

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Stanley Bondi](https://github.com/sdbondi)

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

The aim of this Request for Comment (RFC) is to describe the layer 2 code template registration procedure.
Code templates are composable and reusable building blocks that define state and operations on any given 
side-chain contract.

Registering these on the base layer accomplishes these goals:

1. Provides a public register of code templates with an immutable notary (the Tari base layer).
2. Provides a _source of truth_ that [Validator Nodes] and potentially others can use to ensure the correct code templates are being run.
3. Provides reusable building-blocks available to anyone wishing to build side-chain contracts.
4. Provides other metadata, such as versioning and authorship.

This registration scheme seeks to enable template indexing services and possibly marketplaces for template authors.

## Related Requests for Comment

* [RFC-0302: Validator Nodes](RFC-0302_ValidatorNodes.md)

## Description

A template author registers a template on the [Base layer] creating a special [UTXO] on the base layer.
The registration [transaction] requires the spending of a certain minimum deposit amount of [Tari coin], in addition to
weighted [UTXO] fees, to discourage spam.

It is helpful to describe what a template is and how it relates to a running side-chain contract. Rather than 
being a fully-fledged smart-contract, templates define single-concern state and state-transition functions that can be
composed with other templates and run, by a set of layer 2 [validator node]s. 

This RFC is primarily concerned with the mechanism for making these templates available to other parties wishing to
build contracts and run them. The most important aspect of this is to allow any [validator node] to verify that they 
are running the same code as other members of the same committee. 

## Template Registration UTXO fields

A base-layer template registration [UTXO] MUST have the `TEMPLATE_REGISTRATION` output flag set and contain the following data:

| Name              | Type               | Description                                                                                                                           |
|-------------------|--------------------|---------------------------------------------------------------------------------------------------------------------------------------|
| author_public_key | ECC public key     | Public key of the author                                                                                                              |
| author_signature  | Schnorr signature  | Signature that signs remaining fields                                                                                                 |
| template_name     | String (255)       | A descriptive name for the template                                                                                                   |
| template_version  | Varint             | Code version as a single number                                                                                                       |
| build_info        | BuildInfo struct   | Information on the binary build                                                                                                       |                                                                                                                             |
| binary_checksum   | SHA2 checksum      | A SHA2 checksum of the WASM binary.                                                                                                   |
| binary_address    | [Multiaddr]        | A [multiaddr] pointing to the WASM module binary download. This may be an HTTP, ONION, p2p, or IPFS address. Maximum byte length: 255 |

The `author_signature` is a Schnorr proof that commits to the template fields contained in the [UTXO], namely
`template_name`, `template_version`, `build_info`, `binary_checksum`, and `binary_address`.

The base node acts as a notary for this data, it is not responsible for the validity of the template fields. However, it must
not allow malformed/invalid data to be committed to the blockchain.

Therefore, some additional base-layer consensus rules are required for a `TEMPLATE_REGISTRATION` [UTXO]:
* A base node MUST validate the `author_signature`; and
* The [UTXO] MUST have a relative time-lock of 100 blocks.
* The `binary_checksum` MUST be unique to the current [UTXO] set. 
  * This prevents copies of the same template from being added to the blockchain; and
  * prevents ambiguity for [validator node]s when obtaining the binary.
  
Alternatives: 
* no unique constraint on the `binary_checksum`, the [contract definition] includes a reference to the specific template registration [UTXO].
  
A base node SHOULD NOT check that the `binary_address` points to a valid template binary.

** BuildInfo struct **

| Name        | Type      | Description                                          |
|-------------|-----------|------------------------------------------------------|
| build_image | Url       | A docker build environment used to build the binary  |
| repo_link   | Url       | A public link to the source code repository          |
| commit_hash | SHA2 hash | The commit hash of the code used to build the binary |

The `build_image` field SHOULD contain a link to a publicly-accessible docker image that contains the exact build 
environment used to build the binary. The build environment refers to the specific compiler and OS used. Typically,
this will be a docker image with a specific version of the rust compiler, LLVM and the `wasm-unknown-unknown` target
provided by the Tari community. 

Anyone wishing to execute the template MAY build the binary themselves and compare checksums.
It's worth noting that identical build environments do not guarantee deterministic builds. 
If you're curious about the kinds of issues encountered with deterministic builds using the rust language, 
[read this post](https://dev.to/gnunicorn/hunting-down-a-non-determinism-bug-in-our-rust-wasm-build-4fk1).

## Obtaining the Template Binary

The [contract definition] specifies the [binary_checksum] for each template. Once the [validator node] has been 
assigned a contract via the [contract definition], the [validator node] performs the following actions to obtain a template:

1. It scans the blockchain for a template matching the `binary_checksum`;
2. it downloads the WASM binary and verifies the checksum and stores the binary and associated registration [UTXO] metadata;
3. a validator node operator may choose to build the binary from source as per `BuildInfo` and use the resulting binary;

In the event that a new validator is added to [contract constitution], but the original template registration is unavailable,
the [validator node] SHOULD make the template available to new committee members. New committee members SHOULD confirm the checksum
with a 2/3 majority of the validator committee to ensure that the correct copy is received.

## Spending the Template Registration UTXO

These cases apply to spending the template registration [UTXO]:
1. The author MAY spend into another template registration [UTXO]
  * This effectively withdraws (yanks) the previous version of the template.
  * The template name and author SHOULD be identical. This MAY be enforced by covenant.
  * The template version SHOULD be incremented.
  * Any live contracts SHOULD upgrade their definitions to run the new template.
2. The author MAY spend to a "vanilla" [UTXO] to reclaim their deposit.
  * This effectively withdraws (yanks) the template.
  * The template MAY remain in use on existing contracts. In fact, anyone may now re-register the template.

Alternatives:
* Consensus rule that prevents spending of the template while used by other contracts
* The contract definition may have copy the template binary_checksum etc. Validator nodes may mirror the code for new VNs

## Upgrading a Template 

If a [validator node] detects an update to the [contract definition] that includes a template update, the [validator node]

* MUST fetch the new template(s) as per the previous procedure.

[Tari Coin]: Glossary.md#tari-coin
[transaction]: Glossary.md#transaction
[multiaddr]: https://multiformats.io/multiaddr/
[utxo]: Glossary.md#unspent-transaction-outputs
[validator node]: RFC-0302_ValidatorNodes.md
