# RFC-0201/TariScript

## TariScript

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2020 The Tari Development Community

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
SPECIAL, EXEMPLARY OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT
RECOMMENDED", "MAY" and "OPTIONAL" in this document are to be interpreted as described in
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all
capitals, as shown here.

## Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update without
notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community of the
technological merits of the potential system outlined herein.

## Goals

This Request for Comment (RFC) presents a proposal for introducing Tari Script into the Tari base layer protocol. Tari
Script aims to provide a general mechanism for enabling further extensions such as side chains, the DAN, one-sided
payments and atomic swaps.

## Related Requests for Comment

* [RFC-0200: Base Layer Extensions](BaseLayerExtensions.md)
* [RFC-0202: Tari Script Opcodes](RFC-0202_TariScriptOpcodes.md)
* [RFC-0300: The Tari Digital Assets Network](RFC-0300_DAN.md)

## Introduction

It is hopefully clear to anyone reading these RFCs that the ambitions of the Tari project extend beyond a
Mimblewimble-clone-coin.
It should also be fairly clear that vanilla Mimblewimble does not have the feature set to provide functionality such as:

* One-sided payments
* Multiparty side-chain peg outs and peg-ins
* Generalised smart contracts

Extensions to Mimblewimble have been proposed for most of these features, for example, David Burkett's one-sided payment
proposal for LiteCoin ([LIP-004]), this project's [HTLC RFC](RFC-0230_HTLC.md) and the pegging proposals for the
Clacks side-chain.

Some smart contract features are possible, or partly possible in vanilla Mimblewimble using [Scriptless scripts], such as

* Atomic swaps 
* Hash time-locked contracts

This RFC makes the case that if Tari were to implement a scripting language similar to Bitcoin script, then all of these
use cases collapse and can be achieved under a single set of (relatively minor) modifications and additions to the
current Tari and Mimblewimble protocol.

## Scripting on Mimblewimble

To the author's knowledge, none of existing Mimblewimble projects have employed a scripting language, nor are there 
ambitions to do so. 
 
[Grin](https://github.com/mimblewimble/grin) styles itself as a "Minimal implementation of the Mimblewimble protocol",
so one might infer that this status is unlikely to change soon.

Beam [recently announced](https://github.com/BeamMW/beam/wiki/Beam-Smart-Contracts) the inclusion of a smart contract
protocol, which allows users to execute arbitrary code (shaders) in a sandboxed Beam VM and have the results of that 
code interact with transactions.

[Mimblewimble coin](https://github.com/mwcproject/mwc-node/blob/master/doc/roadmap.md) is a fork of Grin and "considers
the protocol ossified".

Litecoin is in the process of adding Mimblewimble as a
[side-chain](https://github.com/litecoin-project/lips/blob/master/lip-0003.mediawiki). As of this writing, there appear
to be no plans to include general scripting into the protocol.

### Scriptless scripts

[Scriptless script] is a wonderfully elegant technology and inclusion of Tari script does not preclude the use of
Scriptless script in Tari. However, scriptless scripts have some disadvantages:

* They are often difficult to reason about, with the result that the development of features based on scriptless scripts
  is essentially in the hands of a very select group of cryptographers and developers.
* The use case set is impressive considering that the "scripts" are essentially signature wrangling, but is still 
  somewhat limited.
* Every feature must be written and implemented separately using the specific and specialised protocol designed for that
  feature. That is, it cannot be used as a dynamic scripting framework on a running blockchain.

## Tari script - a brief motivation

The essential idea of Tari script is as follows:

Given a standard Tari UTXO, we add _additional restrictions_ on whether that UTXO can be included as a valid input in a
transaction.

As long as those conditions are suitably committed to, are not malleable throughout the existence of the UTXO, and one
can prove that the script came from the UTXO owner, then these conditions are not that different to the 
requirement of having range proofs attached to UTXOs, which require that the value of Tari commitments is non-negative.

This argument is independent of the nature of the additional restrictions. Specifically, if these restrictions are
manifested as a script that provides additional constraints over whether a UTXO may be spent, the same arguments apply.

This means that in a very hand wavy sort of way, there ought to be no reason that Tari Script is not workable.

Note that range proofs can be discarded after a UTXO is spent. This entails that the global security guarantees of Mimblewimble are
not that every transaction in history was valid from an inflation perspective, but that the net effect of all
transactions lead to zero spurious inflation. This sounds worse than it is, since locally, every individual transaction is
checked for validity at the time of inclusion in the blockchain. 

If it somehow happened that two illegal transactions made it into the blockchain (perhaps due to a bug), and the two
cancelled each other out such that the global coin supply was still correct, one would never know this when doing a
chain synchronisation in pruned mode.

But if there was a steady inflation bug due to invalid range proofs making it into the blockchain, a pruned mode sync
would still detect that _something_ was awry, because the global coin supply balance acts as another check.

With Tari script, once the script has been pruned away, and then there is a re-org to an earlier point on the chain,
then there's no way to ensure that the script was honoured unless you run an archival node.

This is broadly in keeping with the Mimblewimble security guarantees that, in pruned-mode synchronisation, individual 
transactions are not necessarily verified during chain synchronisation.

However, the guarantee that no additional coins are created or destroyed remains intact.

Put another way, the blockchain relies on the network _at the time_ to enforce the Tari script spending rules. 
This means that the scheme may be susceptible to certain _horizon attacks_.

Incidentally, a single honest archival node would be able to detect any fraud on the same chain and provide a simple proof
that a transaction did not honour the redeem script.

### Additional requirements

The assumptions that broadly equate scripting with range proofs in the above argument are:

* The script (hash) must be committed to the blockchain.
* The script must not be malleable in any way without invalidating the transaction. This restriction extends to all 
  participants, including the UTXO owner.
* We must be able to prove that the UTXO owner provides the script hash and no-one else.
* The scripts and their redeeming inputs must be stored on the block chain. In particular, the input data must not be
  malleable.

The next section discusses the specific proposals for achieving these requirements.

## Protocol modifications

At a high level, Tari script works as follows:

* The script commitment, which can be adequately represented by the hash of the canonical serialisation of the script in
binary, is recorded in the transaction kernel. 
* An additional two secret keys are provided by UTXO owners that signs, the script and the input data.
* These two keys are also used to created aggregate body owner offsets providing security against spending.

In addition, the script must be attached to the UTXO. As will be demonstrated later, if this is not the case, the UTXO 
owner can ignore the script conditions completely.

### UTXO data commitments

One approach to commit to the script hashes and extra data into the rangeproof and bundle all of these together with the utxo. 
 
There are several changes to the protocol data structures that must be made to allow this scheme to work. 

The first is a relatively minor adjustment to the transaction output definition.
The second is the inclusion of script input data and additional public keys in the transaction input field.

### Transaction output changes

The current definition of a Tari UTXO is:

```rust,ignore
pub struct TransactionOutput {
    /// Options for an output's structure or use
    features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    commitment: Commitment,
    /// A proof that the commitment is in the right range
    proof: RangeProof,
}
```
_Note:_ Currently, the output features are actually malleable. TariScript fixes this.

Under Tari script, this slightly definition changes to accommodate the script:

```rust,ignore
pub struct TransactionOutput {
    /// Options for an output's structure or use
    features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    commitment: Commitment,
    /// A proof that the commitment is in the right range
    proof: RangeProof,
    /// The serialised script
    script_hash: Vec<u8>,
    /// The sender pubkey, K_a
    sender_pub_key: PublicKey
    /// The receiver pubkey, K_b
    receiver_pub_key: PublicKey
}
```


We now introduce some [Notation](#notation).


We define \\( \sigma_i \\) to be the hash of the serialised script and output features,

$$
  \sigma_i = \mathrm{H}(s_i \Vert \mathrm{features_i}\Vert SenderPubKey\Vert ReceiverPubKey)
$$


We refer to the term \\( \sigma_i k_{i'} \\) as the _script product_ and \\( \sigma_i L_i \\) as the _public script 
product_.  

Wallets now generate the range proof with

$$ k_i  +\sigma_i $$

rather than just \\( k_i \\).

Note that:
* The UTXO has a positive value `v` like any normal UTXO. 
* The script nor the output features can be changed by the miner or any other party. Once mined, the owner can no longer
  change the script or output features without invalidating the range proof.
* We dont need to provide the complete script only the script hash. 

### Transaction input changes

The current definition of an input is

```rust,ignore
pub struct TransactionInput {
    /// The features of the output being spent. We will check maturity for all outputs.
    pub features: OutputFeatures,
    /// The commitment referencing the output being spent.
    pub commitment: Commitment,
}
```

In standard Mimblewimble, an input is the same as an output _sans_ range proof. The range proof doesn't need to be checked
again when spending inputs, so it is dropped. 



```rust,ignore
pub struct TransactionInput {
    /// Options for an output's structure or use
    features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    commitment: Commitment,
    /// The serialised script
    script: Vec<u8>,
    /// The script input data, if any
    input_data: Vec<u8>
    /// A signature with K_b, signing the script and input data
    script_signature: Signature,
    /// The sender pubkey, K_a
    sender_pub_key: PublicKey
    /// The receiver pubkey, K_b
    receiver_pub_key: PublicKey
}
```
The input data to the script is sign with the receiver pub key of the Input, as the person spending the input should have control of it. 


### Consensus changes
While normal block and/or transactions balance stays the same and uses the normal commitment below for balance:

$$ C_i = v_i\cdot{H} + k_i\cdot{G} $$


We use the rangeproof to lock all the values inside of the transactional output. Before we can calculate and verify the rangeproof we need to calculate the new "rangeproof" commitment \\ ( \hat{C_i} ) \\. 

$$ C_i = v_i\cdot{H} + k_i\cdot{G} + \sigma_i\cdot{G} $$

We can then verify the rangeproof. If the rangeproof is valid, we know that the value v, is positive and that none of the values have been changed.

And we use the new receiver/sender pubkeys to create a owner_offset.
The owner_offset must match for every block. This can be calculated as follows:
$$
owneroffset\cdot{G} = \sum\mathrm{Outputs.senderPubKey} - \sum(\mathrm{Inputs.receiverPubKey * Hash(script||OutputFeatures)}) 
$$

For every block and/or transactions, an accompanying \\( \mathrm owneroffset \\) needs to be provided. 



In addition to the changes given above, there are consensus rule changes for transaction and block validation.

For every valid block or transaction,

1. Validate range proofs against \\( \hat{C_i} \\) rather 
    than \\( C_i \\).
2. Check that the script signature is valid for every input.
3. The Coinbase output MUST use a default script (`PUSH_ZERO`) and not be counted for the \\( \mathrm owneroffset \\). 
4. The ownersig is valid for every block and transaction. 

## Use cases
Lets investigate a few use cases to see how and who selects which public key. As per specification, there are three public keys per UTXO. We need to see who selects what to see if it remains secure and how it will work. 

Note on the notation:
$$
 x \cdot{k}
$$
This means the value x over curve K. 
$$
 x.k
$$
This means the value k from the value x
### One sided payment. 

For this use case we have Alice, who pays Bob. But Bob's wallet is not online. In this scenario, the transaction will be:

$$
C_a > C_b
$$

Alice owns  \\( C_a \\), her wallet knowns the blinding factor  \\( C_a .k \\) and she knows the receiver private key \\( C_a .k_b \\). She does not know what the sender private key \\( C_a .k_a\\) is. But this is not required in the current transaction. This belongs to the person who send Alice the coins. 

To create the transaction, we need to construct \\( C_b \\). This UTXO has 3 public keys as well. Alice needs to choose the sender private key \\( C_b .k_a\\). The receiver private key is chosen by Bob and Alice does not need to know it. She only needs to know the public key \\(C_b .K_b\\). This Bob can give to her. The one problem key to transfer is the blinding factor \\(C_b .k\\). This needs to be known by both Alice and Bob. Some out of band transfer of keys like diffie_hellman needs to be done. 

Now Alice has all the required information to create and sign for this transaction. She can sign the kernel as well as the OwnerOffset. But crucially, once mined, only Bob can claim the coins since to claim \\(C_b\\), Alice would need to know the receiver private key \\(C_b .k_b\\), which only Bob knows. 

### HTLC like script
In this use case we have a script that controls to whom it is spend. We also have Alice and Bob, but on creation we dont yet know whom can claim the coins. This is controlled via the script. 

$$
C_a > C_b
$$

Alice owns  \\( C_a \\), her wallet knowns the blinding factor  \\( C_a .k \\) and she knows the receiver private key \\( C_a . k_b \\). She does not know what the sender private key \\( C_a .k_a\\) is. But this is not required in the current transaction. This belongs to the person who send Alice the coins. 

To create the transaction, we need to construct \\( C_b \\). This UTXO has 3 public keys as well. Alice needs to choose the sender private key \\( C_b .k_a\\). Here both Alice and Bob needs to know the receiver private key \\( C_b .k_b\\) and the blinding factor \\(C_b .k\\). Again we require some sort of out of band transfer of keys like diffie_hellman.

Now Alice has all the required information to create and sign for this transaction. She can sign the kernel as well as the OwnerOffset. But crucially, once mined, neither Alice or Bob can claim the coins since to claim \\(C_b\\), they would need to provide the correct information to the script. If one of them can provide the correct information to the script they can claim as they both know the blinding factor \\(C_b .k)\\ and the receiver private key \\(C_b .k_b\\). 

TODO malleability here. 

If we where to create a HTLC type utxo between Alice and Bob, and this is mined. The problem becomes who can spend this. In a one sided payment of Alice to Bob: Alice gives permission to Bob to spend it, and only he can spend it. 
With the "OwnerOffset" the owner needs to provide a key to spend this. With an HTLC, the owner becomes Alice and Bob. Lets assume that they both cooperate to create a new UTXO. If Bob was malicious, he could modify the transaction and create a new UTXO, replacing the one he and Alice created. Now one should be detect this happened apart form Alice. 

The only way to stop this, is to create an aggregate blinding factor. But then we are implementing a HTLC the way normal MW does with scriptless scripts. 

## Checking the requirements

Let us evaluate this scheme against the [Additional requirements](#additional-requirements) listed earlier.

### Malleability

Malleability can be divided into three parts. These are handled below in the following cases:

#### Miner Malleability

This case is for when the miner changes certain parameters or values of a transaction. In its current form, Tari does allow a miner to change OutputFeatures of an UTXO. This is not detectable. 

With tari_script, the OutputFeatures are locked by the rangeproof. If a miner wanted to change the OutputFeatures, it requires it to generate a new rangeproof. This is not possible since generating a new rangeproof requires knowledge of the blinding factor k. Something it does not know or can. 

#### Receiver Malleability

This is the case where the receiver can change some part of the output. In normal Tari this is not an issue, as the output is fully owned by the receiver. So the receiver can change what he wants. 

With tari_script we need to stop this to a certain amount as the receiver might not be able to spend the utxo yet, pending some script, or it needs to be paid back to the sender, for example a HTLC contract. This is stopped via the OwnerOffset. This value will confirm the script and OuputFeatures of a utxo. 

#### Sender Malleability

While the sender can change much of the transaction. He is allowed to do so as he is the owner. Any change with the sender will cause a normal double spend transaction. And there is nothing much that can be done to stop this. 

But if a transaction is mined. The values are locked and no one can change any value. 

TODO: Investigate sender malleability with multiple multi party scripts like HTLCs. 


### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is spent
in the same block it was created. Tari_script with its OwnerOffset will stop cut-through completely as it currently works. It will still allow pruning thou. Cut-through is still possible if the original owner participates. Example Alice, pays Bob, who pays Carol. Cut-through can happen only if Alice and Carol negotiate a new transaction. 

This will ensure that the original owner is happy with the spending the transaction to a new party, eg verified the spending conditions like a script.

### Script lock key generation

At face value, it looks like the burden for wallets has doubled, since each UTXO owner has to remember three private keys,
the spend key, \\( k_i \\) and the sender key \\( k_{b} \\) and the original receiver key \\( k_{a} \\) . In practice, the other keys can be
deterministically derived from the spend key. For example, the \\( k_{b} \\) can be equal to the hash of the \\( k_i \\).

### Replay attacks
TODO

### Blockchain bloat

The most obvious drawback to TariScript is the effect it will have on blockchain size. The addition of the script and 
script signature, it also adds two public keys to every UTXO. This can eventually be pruned, but will increase
storage and bandwidth requirements.

Input size in a block will now be much bigger as each input was previously just a commitment and an OuputFeatures. Each input now includes a script,input_data, script_signature and two public keys. This could be improved by not sending the input again, but just sending the hash of the input, input_data and script_signature. But this will still be larger than inputs are currently. 

The additional range proof validations and signature checks significantly hurt performance. Range proof checks are 
particularly expensive. To improve overall block validation, batch range proof validations should be employed to mitigate 
this expense.

### Fodder for chain analysis

Another potential drawback of TariScript is the additional information that is handed to entities wishing to perform chain
analysis. Having scripts attached to outputs will often clearly mark the purpose of that UTXO. Users may wish to re-spend
outputs into vanilla, default UTXOs in a mixing transaction to disassociate Tari funds from a particular script.

## Notation

| Symbol            | Definition                                                                 |
|:------------------|:---------------------------------------------------------------------------|
| \\( s_a \\)       | An output script for output _a_, serialised to binary                      |
| \\( \sigma_a \\)  | The 256-bit Blake2b hash of an output script, \\( s_a \\)                  |
| \\( k_{a'} \\)    | The script private key                                                     |                          |
| \\( C_a \\)       | A Pedersen commitment, i.e. \\( k_a.G + v_a.H \\)                          |
| \\( \hat{C_a} \\) | A script-modified Pedersen commitment, i.e. \\( (k_a + k_{a'}).G + v.H \\) |
| \\( \delta \\)    | The transaction offset                                                     |


### Credits

Thanks to [@philipr-za](https://github.com/philipr-za) and [@SWvheerden](https://github.com/SWvheerden) for their input
and contributions to this RFC.

[data commitments]: https://phyro.github.io/grinvestigation/data_commitments.html
[LIP-004]: https://github.com/DavidBurkett/lips/blob/master/lip-0004.mediawiki
[Scriptless script]: https://tlu.tarilabs.com/cryptography/scriptless-scripts/introduction-to-scriptless-scripts.html
[Handshake white paper]: https://handshake.org/files/handshake.txt
