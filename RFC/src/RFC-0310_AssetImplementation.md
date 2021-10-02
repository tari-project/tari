# RFC-0310/AssetImplementation

## Asset Implementation Details

![status: draft](theme/images/status-draft.svg)

Maintainer(s): [mikethetike](https://github.com/mikethetike)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2021 The Tari Development Community

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

## Related Requests for Comment

* [RFC-0300: The Digital Assets Network](RFC-0300_DAN.md)
* [RFC-0340: Validator Node Consensus](RFC-0340_VNConsensusOverview.md)
* [RFC-0345: Asset Life cycle](RFC-0345_AssetLifeCycle.md)

## Problem Statement

This RFC details the specifics of how NFTs are stored and represented on 
the base layer and the digital assets layer.

## Proposal

### Changes to outputs
The following fields must be included in the TransactionOutput, inside the OutputFeatures:

New flags to be added to OutputFlags

| Name | Value | Description |
| --- | --- | --- |
| NON_FUNGIBLE | 0b1000_1000 | This UTXO contains Non Fungible data and must not be combined with other fungible UTXOs |
| ASSET_REGISTRATION | 0b1100_1000 | This UTXO contains registration information for a new asset |
| MINT |0b0100_0000 | This UTXO represents the creation of a new NFT |
| BURN | 0b0010_0000 | The `unique_id` in this UTXO can be spent as a normal input and must not appear in the output set unless accompanied by a `MINT` flag |
| SIDECHAIN_CHECKPOINT | 0b1001_1000 | This UTXO is a checkpoint for a sidechain | 


New fields added to `OutputFeatures`

Note: this replaces some information in [RFC-0311](RFC-0311_AssetTemplates.md)

| Name | Type | Description |
| --- | --- | --- |
| unique_id | bytes(32) | A unique id representing a token. This value must be present only when the NON_FUNGIBLE flag is set. The pair (parent_pub_key, unique_id) will be unique across the unspent set as described below |
| parent_pub_key | bytes(32) |  A namespacing field that also allows a heirachy of assets. In most cases this will be the asset that a token belongs to, but future uses may be possible as well |
| asset_registration | struct | Data for registering of an asset. This value **MUST** be present if ASSET_REGISTRATION flag is set |
| asset_registration.name | bytes(64) | utf8 name of the asset. Optional |
| asset_registration.template_ids | Vec<bytes(32)> |The templates that this asset exposes. Usually, at least one will be specified |
| asset_registration.creator_sig | bytes(64) (32 Nonce + 32 sig) | A signature of the UTXO's `commitment` and `script` using the public key in `unique_id` to prove this registration was created in this UTXO |
| asset_registration.checkpoint_unique_id | bytes(32) | A reference to the unique id reserved for checkpoint UTXOs. Optional |
| asset_registration.checkpoint_frequency | uint32 | The frequency, in sidechain blocks (or other measure of time the sidechain uses) in which checkpoints are created |
| mint.issuer_proof | bytes(96) (64 nonce + 32 sig) | A _ComSig_ proof proving that the owner of the UTXO containing the asset registration created this token 
| checkpoint.merkle_root | bytes(32) | Merkle root of the sidechain data. The format and meaning of this data is specific to the sidechain implementation |


Note: `unique_id` is not always required to be a public key

### New Base Layer Consensus rules

**New Consensus Rule**

If a UTXO is received in the mempool or block with the `MINT` bit set in the flags, the containing transaction or block is valid only if 
no other UTXO exists in the unspent set with the same `unique_id`,  `parent_public_key` pair. 

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
with the `unique_id`  that is the same as this minted output's `parent_public_key`. The message signed must be `Hash(commitment|script|unique_id|parent_public_key)`

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
other NFTs, only one sidechain checkpoint UTXO with that unique_id can exist in the unspent set at one time. The conditions of 
who can spend a checkpoint are determined by the TariScript on the checkpoint. In most cases it will be an `m of n` multisig condition 
using the members of the Hotstuff committee, with `m` equal to the number of members of the committee and `n` equal to `m` minus 
the number of failures the committee can tolerate.

The unique_id can be a specifically chosen id, but can also be a random public key. 

There is no limit to the number of different sidechain checkpoint tokens an asset has, but it will usually be one. 

## New Base Node GRPC methods

A base node should expose the following methods via GRPC:

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

## Validator Node Instructions

> This section is specific to the Tari Validator Node. Other sidechains may make use of 
> the base layer mechanisms for sidechains as well

Callers may issue instructions to a validator node committee member using GRPC. Instructions take the form:

```protobuf

message Instruction{
    // The public key of the asset being invoked
    bytes asset_public_key =1;
    // The template being invoked
    uint32 template_id = 2;
    // The method on the template being invoked.
    string method =3;
    // The position based arguments
    repeated bytes args = 4;
    // A commitment that exists on the base layer that identifies
    // the caller. For example, the asset issuer could use the 
    // commitment that holds the asset registration, while the owner
    // could use the commitment on the UTXO containing the NFT
    bytes caller_commitment = 4;
    // A unique number to identify this instruction
    uint64 id = 5;
    // A comsig proving the caller owns the commitment in `caller_commitment`
    // The challenge is the hash of the above fields, in order
    bytes caller_sig = 6;
    
    // Fee fields TBD
}

```

Upon receiving an instruction, a validator node must store it in its mempool and broadcast it to all other members of
the committee. 

As per Hotstuff and [RFC-0340], when a validator node becomes the leader, it selects a list of instructions from its 
mempool and creates a proposal. 

> The leader can use any algorithm to select instructions from the mempool and it is 
valid for it to select no instructions

The proposal must conform to the below protobuf

```protobuf

message HotStuffTreeNode {
    // The parent node, as per Hotstuff Specification
    bytes parent = 1;
    
    // The payload
    InstructionSet payload = 2;
}

message InstructionSet{
   // A list of instructions
   repeated Instruction instructions = 1;
   // The hash of the block at the tip of the base layer, according to the leader's view of the base layer
   bytes base_layer_block = 3;
}

```

> Note that the base layer tip used by the leader may be a number of blocks below the actual tip to minimise the 
> number of reorgs and increase the chance that all committee members have the same view of the base layer

The committee members process the proposal via Hotstuff, except that during the prepare and pre-commit stages, they must
confirm that block in `base_layer_block` is still present in the base node that they are connected to. 

If a validator node has reached the Commit stage, it must prevent the base layer from reorging by issuing a GRPC call to 
`LockBlock`

**New RPC Method**
The base node must implement a GRPC method as follows:

```protobuf

rpc LockBlock(LockBlockRequest) LockBlockResponse;

message LockBlockRequest {
  bytes block_hash = 1;
  uint64 until_height = 2;
}

message LockBlockResponse {
  uint64 current_height= 1;
}
```

Upon receiving the command, the base node must keep the chain up to the block specified until it reaches height
`until_height`, at which point it may discard the chain if the block is no longer in the main chain. Note: The base node
may continue to process blocks and reorgs, but must be able to respond to queries as though the specified chain is its
main chain.

Upon entering the Prepare phase the validator node *should* issue an RPC request to release all locked blocks

```protobuf
rpc ReleaseLockedBlocks(ReleaseLockedBlocksRequest) ReleaseLockedBlocksResponse;

message ReleaseLockedBlocksRequest{
  
}

message ReleaseLockedBlocksResponse {
  
}
```

Upon entering the Decide phase, all committee members validate and execute the instructions in the proposal.

For each instruction:
1. The base node to retrieve the UTXO the caller specified from the Unspent set (as at the chain in `LockedBlock`). 
2. If the UTXO does not exist the instruction fails
3. Checks the caller_sig is valid
4. Attempts to execute the instruction
5. At this point the instruction may fail, based on the specific rules of the template, for example:
   1. The instruction requires the role of AssetIssuer, but the owner commitment was provided
   2. The instruction is not valid for the current state of the NFT
6. No state data must be changed in the event of a failed instruction

Instructions, failed or successful, and their results, are added to the InstructionMerkleMountainRange.
The root of the InstructionMerkleMountainRange and the root of the current state data must be hashed together and
stored with a reference to the HotstuffNode. This data will be aggregated when creating the sidechain checkpoint.

The state data must be stored as a Patricia Trie, with each key being the `unique_id` and the leaf being a serialized protobuf 
of the following schema:

```protobuf

message TokenState {
  repeated TokenStatePair pairs = 1;
}

message TokenStatePair {
  string name = 1;
  bytes value = 2;
}
```

## Checkpoints

As part of the Commit phase, a validator node includes in its vote, a signature of the current state and instruction set
in `locked_qc` that can be used as part of the threshold signature to spend the current checkpoint. This signs the state of the chain before the current instruction proposal is executed. Before
sending out the Decide messages, the current leader looks at the base layer and determines if `checkpoint_frequency`
blocks in the sidechain have been created since the last checkpoint. If so, the leader assembles a transaction
spending the previous checkpoint using the signatures it has obtained. 

## Examples

### NFT with sidechain metadata 

Use case: I want to issue an NFT that has some immutable metadata and some mutable metadata.

In this example, let's pretend we are making a simple game called YachtClicker in 
which every time a user clicks on a Yacht, it increases in XP. The user can change the name of the yacht, 
but only the asset issuer can award XP based on the clicks reported in the app. 

Template used: [EditableMetadata](RFC-0312_AssetTemplateEdtitableMetadata.md) 

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
         // SIDECHAIN | MINT
         "flags": "0b1101_1000",
         // Special unique id,
         "unique_id": "0x0000000000000000",
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

> Note that this could be done in one step if there is an out-of-band communication that provides the 
> issuer with the commitment for the utxo

Usually this process would be done via GRPC by the software running YachtClicker, but we'll do it manually for the first token.

Again using the console wallet I run:
```
tari_console_wallet --command "mint-tokens <yacht_clicker_pub_key> yacht1"
```

This creates and broadcasts a transaction with the following UTXO:

```json
{
   TransactionOutput: {
      "features": {
         // NON_FUNGIBLE | MINT
         "flags": "0b1100_0000",
         "unique_id": "hash('yacht1')",
         "parent_public_key": "<yacht_clicker_pub_key",
         "mint": {
            "issuer_proof": "<com_sig of h(commitment |script) using asset registration commitment>"
         },
         "metadata": []
      },
      "script": "Nop"
   }
}
```

Note that this must be called from Asset Issuer's wallet so that it can create the commitment signature. 



### Transferring the token to a new owner

At some point the owner will be assigned to a new owner that is not the issuer. This might happen during a sale or airdrop. In the case
of YachtClicker we will airdrop a token to the new owner when they first sign up for the game. Let's call the new user Bob.

When Bob signs in, he provides his Tari wallet address. We can then transfer the token to that address interactively or non-interactively 
using the current Tari base layer transactions, with a few small changes:
1. The current UTXO that contains the NFT must be spent as an input
2. Exactly one of the UTXOs in the outputs Bob provides must have the same `unique_id` and `parent_public_key` as the UTXO being spent.
3. The `MINT` flag must be unset.
4. The minting proof and other feature data may be copied to the new UTXO but this is not mandatory.

The UTXO created may look like this:
```json
{
   TransactionOutput: {
      "features": {
         // NON_FUNGIBLE 
         "flags": "0b1000_0000",
         "unique_id": "hash('yacht1')",
         "parent_public_key": "<yacht_clicker_pub_key",
         "metadata": []
      },
      "script": "Nop"
   }
}
```

Note that if interactive Mimblewimble Tari transactions are used, the new owner of the UTXO is not revealed, only that it 
has passed to a new owner. Also, we cannot see which other Yachts are owned by Bob.

At this point anyone observing the token will know of its presence, but can't use it practically since there is no metadata associated with it.
If you are only interested in persisting a number to the blockchain to prove ownership, you can stop here.

### Editing metadata

There are a few actors involved in the metadata that makes up the Yacht. The URI that the Yacht can be found at is 
designed to be immutable by the asset issuer. This may be an IPFS link or standard link. The [EditableMetadata] template 
specification allows for special prefixes to determine who owns and can update the metadata. 

| prefix | Who can issue instructions to update |
| --- | --- | 
| `issuer` | The owner of the asset registration UTXO |
| `owner` | The owner of the NFT UTXO |
| `locked` | This data cannot be updated |

When a user clicks on the image of the Yacht in the game app, the app determines if they are legit and
submits an instruction to a ValidatorNode that looks as follows:

```json
{
   "Instruction": {
      "unique_id": "hash('yacht1')",
      "parent_public_key": "<yacht_clicker_pub_key",
      "template_id": "editable_metadata",
      "method": "update_metadata",
      "arguments": [
         {
            "name": "issuer.num_clicks",
            "value": "37"
         }
      ],
      "caller": "<asset_utxo_commitment>",
      "caller_sig": "<comsig with value in 'caller'>"
   }
}
```

The user has control of fields prefixed with `owner` for as long as they control the UTXO with the NFT. An example instruction
may look like the following:

```json
{
   "Instruction": {
      "unique_id": "hash('yacht1')",
      "parent_public_key": "<yacht_clicker_pub_key",
      "template_id": "editable_metadata",
      "method": "update_metadata",
      "arguments": [
         {
            "name": "owner.boat_name",
            "value": "The lone wanderer"
         }
      ],
      "caller": "<nft_utxo_commitment>",
      "caller_sig": "<comsig with value in 'caller'>"
   }
}
```

Upon receiving an instruction the validator node processes it as described above and will periodically create a checkpoint.

## Example 2: Importing an ERC20
