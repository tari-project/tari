# RFC-0310/AssetImplementation

## Asset Implementation Details

![status: draft](theme/images/status-draft.svg)

Maintainer(s): [mikethetike](https://github.com/mikethetike)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright <YEAR> <COPYRIGHT HOLDER | The Tari Development Community>

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

## Abstract

## Related Requests for Comment

## Problem Statement

This RFC details the specifics of how NFTs are stored and represented on 
the base layer and the digital assets layer.
If I own an NFT, how can I provide a proof that I own it?
Transferring of NFTs must be censorship resistant. 
How are operations on the sidechain managed.


* [How are NFTs stored on the base layer]
* [How do sidechains store data on the base layer] 
* [what nodes want to see this information]
* [how are peg ins and peg outs achieved]


## Open problems
* [covenants]

## Goals

* [privacy concerns]
* [ownership]

## Requirements
* Only the Asset Issuer can mint a new NFT for a token

## Proposal

### Changes to outputs
The following fields must be included in the TransactionOutput, inside the OutputFeatures:

New flags to be added to OutputFlags

| Name | Value | Description |
| --- | --- | --- |
| NON_FUNGIBLE | 0b1000_1000 | This UTXO contains Non Fungile data and must not be combined with other fungible UTXOs |
| ASSET_REGISTRATION | 0b1100_1000 | This UTXO contains registration information for a new asset |
| MINT |0b0100_0000 | This UTXO represents the creation of a new NFT, and is used during the  |
| BURN | 0b0010_0000 | The `unique_id` in this UTXO can be spent as a normal input and must not appear in the output set unless accompanied by a `MINT` flag |
| SIDECHAIN_CHECKPOINT | 0b1001_1000 | This UTXO is a checkpoint for a sidechain | 


New fields added to `OutputFeatures`

Note: this replaces some information in [RFC-0311](RFC-0310_AssetImplementation.md)

| Name | Type | Description |
| --- | --- | --- |
| unique_id | bytes(32) | A unique id representing a token. This value must be present only when the NON_FUNGIBLE flag is set. The pair (parent_pub_key, unique_id) will be unique across the unspent set as described below |
| parent_pub_key | bytes(32) |  A namespacing field that also allows a heirachy of assets. In most cases this will be the asset that a token belongs to, but future uses may be possible as well |
| asset_registration | struct | Data for registering of an asset. This value **MUST** be present if ASSET_REGISTRATION flag is set |
| asset_registration.name | bytes(64) | utf8 name of the asset. Optional |
| asset_registration.template_ids | Vec<bytes(32)> |The templates that this asset exposes. Usually, at least one will be specified |
| asset_registration.creator_sig | bytes(64) (32 Nonce + 32 sig) | A signature of the UTXO's `commitment` and `script` using the public key in `unique_id` to prove this registration was created in this UTXO |
| asset_registration.checkpoint_unique_id | bytes(32) | A reference to the unique id reserved for checkpoint UTXOs. Optional |
| mint.issuer_proof | bytes(96) (64 nonce + 32 sig) | A _ComSig_ proof proving that the owner of the UTXO containing the asset registration created this token 
| checkpoint.merkle_root | bytes(32) | Merkle root of the sidechain data. The format and meaning of this data is specific to the sidechain implementation |


Note: `unique_id` is not always required to be a public key

## Gram weight for new features
[Mint flag should be heavier]

```rust

pub struct OutputFeatures {
    flags: OutputFlags,
    maturity: u64,
    
    // New token fields
    unique_id: Vec<u8>,
    parent_public_key: Vec<u8>
    
    // New instruction features
}
```


### New Base Layer Consensus rules

**New Consensus Rule**

If a UTXO is received in the mempool or block with the `MINT` bit set in the flags, the containing transaction or block is valid only if 
no other UTXO exists in the unspent set with the same `unique_id` and `parent_public_key`. 

**New Consensus Rule**

If a UTXO is received in the mempool or block with the `NON_FUNGIBLE` bit set and the `MINT` bit unset, the containing transaction 
or block is valid only if:
1. It contains exactly one input with the same `unique_id` and `parent_public_key`
2. It contains no other UTXOs with the same `unique_id` and `parent_public_key`

Note that the `ASSET_REGISTRATION` and `SIDE_CHAIN_CHECKPOINT` contain the `NON_FUNGIBLE` bit and will also contain 
the `MINT` bit for the first instance of each of these.

### Registering an asset

A new asset is registered by creating a UTXO with the ASSET_REGISTRATION flag set, a new public key in `unique_id`,
and the data in `asset_registration` populated.

**New Consensus Rule** 

When receiving a UTXO with the `ASSET_REGISTRATION` flag set, the `creator_sig` must contain a
Schnorr signature using the public key in `unique_id` and the following challenge:

```
hash(unique_id, commitment, script)
```

This ensures that a third party cannot remove the asset registration transaction and register it 
under their UTXO. The other fields in the `OutputFeatures` struct are already locked by the `metadata_signature`
and don't need to be included.

A key part of this proposal relies on the owner of a utxo to be determined via a commitment signature, so if a malicious 
actor is able to switch this data into a new commitment, that actor gains control of the asset or token.


Note: because this requires extra validation, the gram weight of a UTXO that carries the `ASSET_REGISTRATION` flag should be higher.

### Minting a unique token

A new token is created for an asset by creating a UTXO with the `MINT` and `NON_FUNGIBLE` flags set
and the `mint_nonfungible` feature data set.

**New Consensus Rule**

When receiving a UTXO with the `MINT` and `NON_FUNGIBLE` flags set, either the `parent_public_key` must be populated, 
or the `ASSET_REGISTRATON` flag must also be set.

Note: If the `parent_public_key` is populated and the `ASSET_REGISTRATION` flag is set, it must be treated as an asset registration 
instruction. This allows sub assets to be grouped under a parent asset.

**New Consensus Rule**

When receiving a UTXO with the `MINT` and `NON_FUNGIBLE` flags set and the `ASSET_REGISTRATION` unset, the `minting_proof_signature`
must be a valid commitment signature using the commitment of the UTXO that contains the currently valid asset registration feature
for with the `unique_id` = `parent_public_key`. The message signed must be `Hash(commitment|script|unique_id|parent_public_key)`

Note: because this requires extra validation, the gram weight of a UTXO that carries the `MINT` flag should be higher.

### Transferring a unique token

The `MINT` flag is only set on the first UTXO a token appears in. When it is transferred to a new UTXO, it should only have the `NON_FUNGIBLE` flag set.

### Burning a unique token

A token can be burnt by setting the `BURN` flag and then spending the UTXO. A token with the `BURN` set should be considered as valid as long as it is in the unspent set
and is only destroyed once the UTXO is spent.

**New Consensus Rule**
When receiving a transaction or block with an input with the `BURN` flag set, there must not exist a UTXO in the output set containing the same `unique_id`,`parent_public_key` pair, unless 
that output has the `MINT` flag set.

### Sidechain Checkpoints

A sidechain checkpoint is a special type of NFT UTXO. Extra sidechain data can be specified in the `sidechain` field of the output features. Like 
other NFT's, only one sidechain checkpoint UTXO with that unique_id can exist in the unspent set at one time. The conditions of 
who can spend a checkpoint are determined by the TariScript on the checkpoint. In most cases it will be an `n of m` multisig condition 
using the members of the Hotstuff committee, with `m` equal to the number of members of the committee and `n` equal to `m` minus 
the number of failures the committee can tolerate.

The unique_id can be a specificly chosen id, but can also be a random public key. 

There is no limit to the number of different sidechain checkpoint tokens an asset has, but it will usually be one. 

## New Base Node GRPC methods

A base node should expose the following methods on via GRPC:

```protobuf

  rpc GetTokens(GetTokensRequest) returns (stream GetTokensResponse);

  message GetTokensRequest {
    bytes asset_public_key = 1;
    // Optionally get a set of specific unique_ids
    repeated bytes unique_ids = 2;
  }

  message GetTokensResponse {
    bytes unique_id = 1;
    bytes asset_public_key = 2;
    bytes owner_commitment = 3;
    bytes mined_in_block = 4;
    uint64 mined_height = 5;
  }


```



* [structure of checkpoint]
* [structure of peg in/out]
* [hotstuff nodes]
* [fees on side chain]
* [example side chains]
* 

## Examples

### NFT with sidechain metadata 

Use case: I want to issue an NFT that has some immutable metadata and some mutable metadata.

In  this example, let's pretend we are making a simple game YachtClicker in 
which every time a user clicks on a Yacht, it increases in XP. The user can change the name of the yacht, 
but only the asset issuer can award XP based on the clicks reported in the app. 

Template used: [EditableMetadata](linktbd) 

Tools used: Wallet CLI, Collectibles CLI

For this example, let's call the asset `yacht_clicker`. 

The first step is to register the asset. 

I create a JSON file specifying the template parameters:

```json
// yacht_clicker_registration.json
{
   "name": "yacht_clicker",
   "parent_public_key": null,
   "create_initial_checkpoint": true,
   "committee": ["<committee_pub_key1>"],
   "templates": [{
     "template_id": "editable_metadata",
     "fields": [
       "issuer.num_clicks",
       "locked.uri",
       "owner.boat_name"
     ]}]
}
```

Using a wallet that has sufficient funds to pay for the registration fees, in the CLI I execute the command 

```
tari_console_wallet --command "register-asset yacht_clicker_registration.json"
```

The wallet will create a transaction containing a UTXO with the following `OutputFeatures` and publish
to a base node.

```json
{
  TransactionOutput: {
    "features": {
      "flags": "0b1100_1000", // ASSET_REGISTRATION
      "unique_id": [
        generated_pubkey
      ],
      "parent_public_key": null,
      "asset_registration": {
        "name": "yacht_clicker",
        "templates": [
          id
          for
          editable_metadata
          template
        ],
        "creator_sig": [
          signature
          of
          h(commitment
          |
          script)
        ]
      },
      "metadata": [
        protobuf
        serialized
        template
        data
        as
        Vec<u8>
      ]
    },
    "script": "Nop"
  }
}
```

This asset has a combination of data on the base layer and metadata on the second layer. Because the ownership of the token is controlled by the base layer, it is fully decentralized and censorship resistant.
The editing of metadata is done by a permissioned committee of validator nodes, which has a quicker consensus mechanism through HotstuffBFT

The `fields` specified in registration JSON file are specific to the [EditableMetadata](tbd) template and take the form 
of `<owner>.<field_name>`. We have 3 owners here, the `issuer`, `locked` and `owner`. When processing instructions, the DAN layer will check
whether the caller has access to edit these fields. More on that later.

The `create_initial_checkpoint` option instructs the wallet to create a UTXO that the committee will use to create
checkpoints

The UTXO will look as follows:

```json
{
   TransactionOutput: {
      "features": {
         "flags": "0b1101_1000",
         // SIDECHAIN | MINT
         "unique_id": "0x0000000000000000"
         // Special unique id,
         "parent_public_key": "<yacht_clicker_pub_key",
         "mint": {
            "issuer_proof": "<com_sig of h(commitment |script) using asset registration commitment>"
         },
         "checkpoint": {
            "merkle_root": "0x00000000000"
         },
         "metadata": []
      },
      "script": "CheckMultisig 1 of 1 <committee_pub_key1>"
   }
}
```


### Minting the yacht

In our game, we now need to create a new yacht that the player can click and start upgrading. This will be a two step process,
firstly the yacht NFT UTXO must be minted, and then it can be transferred to the new owner. 

> Note that this could be done in one step if there is an out of band communication that provides the 
> issuer with the commitment for the utxo





## Example 2: Importing an ERC20
