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
* An additional secret key is provided by UTXO owner.
* The script resolves to a public key that provides ownership and that signs the script input data to stop malleability and prove ownership of the corresponding private key.
* These two keys are also used to created aggregate body owner offsets providing security against spending.

In addition, the script must be attached to the UTXO. As will be demonstrated later, if this is not the case, the UTXO 
owner can ignore the script conditions completely.

### UTXO data commitments

One approach is to commit to the script hashes and extra data into the rangeproof and bundle all of these together with the utxo. This provides security against malleability of the UTXO and or script. 
 
There are several changes to the protocol data structures that must be made to allow this scheme to work. 

The first is a relatively minor adjustment to the transaction output definition.
The second is the inclusion of script input data and additional public key in the transaction input field.

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
    /// The receiver pubkey, K_a
    receiver_pub_key: PublicKey
}
```


We now introduce some [Notation](#notation).

We define the commitment \\( C_i \\) as

$$
C_i = v_i \cdot{H} + k_i \cdot{G}
$$

We define \\( \sigma_i \\) to be the hash of the serialised script, output features and receiver public key

$$
  \sigma_i = \mathrm{H}(s_i \Vert \mathrm{features_i}\Vert ReceiverPubKey)
$$

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
    /// A signature with K_s, signing the script and input data
    script_signature: Signature,
    /// The receiver pubkey, K_a
    receiver_pubkey: PublicKey
}
```
The input data to the script is signed with resolving script public key \\(K_s \\). This is to ensure that resulting public key has a known private key.

The script_signature is a Schnorr signature and is defined as follows:
$$
signature = r + (H(Script)||InputData))*k_s
$$ 



### Consensus changes
While normal block and/or transactions balance stays the same and uses the normal commitment below for balance:

$$ C_i = v_i\cdot{H} + k_i\cdot{G} $$


We use the rangeproof to lock all the values inside of the transactional output. Before we can calculate and verify the rangeproof we need to calculate the new "rangeproof" commitment \\ ( \hat{C_i} ) \\. 

$$ \hat{C_i} = v_i\cdot{H} + k_i\cdot{G} + \sigma_i\cdot{G} $$

We can then verify the rangeproof. If the rangeproof is valid, we know that the value v, is positive and that none of the values have been changed.

And we use the new receiver pubkey ( \\(K_a \\) )  and the script public key ( \\(K_s \\) ) to create a \\( ScriptOffset\\).
The \\( ScriptOffset\\) must match for every block. This can be calculated as follows:
$$
ScriptOffset\cdot{G} = \sum\mathrm{K_s} - \sum(\mathrm{K_a * Hash(UTXO)}) 
$$
The \\(Hash(UTXO)\\) is the serialized hash of the entire output.


For every block and/or transactions, an accompanying \\( \mathrm ScriptOffset \\) needs to be provided. 



In addition to the changes given above, there are consensus rule changes for transaction and block validation.

For every valid block or transaction,

1. Validate range proofs against \\( \hat{C_i} \\) rather 
    than \\( C_i \\).
2. Check that the script signature is valid for every input.
3. The Coinbase output MUST use a default script (`PUSH_EMPTY`) and not be counted for the \\( \mathrm ScriptOffset \\). 
4. The ownersig is valid for every block and transaction. 

## Use cases

Lets investigate a few use cases to ensure that this scheme is secure

### Normal MW transaction

For this use case we have Alice, who pays Bob. But Bob's wallet is  online. In this scenario, the transaction will be:

$$
C_a > C_b
$$

This can be done in two ways.
#### Method 1
Alice stored her her commitment \\( C_a\\) with a script of (`PUSH_EMPTY`). This means that the script will technically do nothing and will only output the input public key. So technically any person can claim the script. 

But this UTXO is still secure as the UTXO is still locked with the normal MW blinding factors. Alice and Bob's wallets communicate to complete a normal MW transaction with normal MW signing. After all this is completed. Bob's wallet will choose a private keys \\( k_s\\) and \\( k_a\\). 
Bob's wallet will use the public \\( K_s\\) as input to the script. He can then sign the input of the script with his private key \\( k_s\\). 
On his UTXO \\( C_b\\) he attaches his second public key \\( K_a\\). 

Bob will then construct his \\( ScriptOffset\\) with:
$$
ScriptOffset = k_s - C_b.k_a * Hash(UTXO)
$$

Any BaseNode can validate the transaction as with normal MW transactional rules. And any node can validate the script and script signature. As well as validate the \\( ScriptOffset\\) with: 
$$
ScriptOffset \cdot{G} = K_s - C_b.K_a * Hash(C_b)
$$

This allows that Alice can give over the UTXO to Bob, Bob cannot claim the UTXO without alice giving it to Bob as he does not know the blinding factor of \\( C_a\\). 
#### Method 2
Alice stored her her commitment \\( C_a\\) with a script of (`CheckSigVerify`). This means that the script needs to be provided by some pubkey before unlocking and resolving to a known pubkey.

Alice and Bob both create a  \\( K_a\\) key, with:
$$
K_a = K_{a-Alice} + K_{a-Bob}
$$

In this case, Alice and Bob both create the normal transaction.  Except here Bob has the fill in the aggregated \\( K_a\\) inside of his commitment \\( C_b\\). Alice will fill in the script with her \\( K_s\\) to unlock the commitment \\( C_a\\). 
Alice will construct her part of the \\( ScriptOffset\\) with:
$$
scriptoffset_{Alice} = k_s - k_{a-Alice} * H(C_b)
$$

Bob will construct his part of the \\( ScriptOffset\\) with:
$$
scriptoffset_{Bob} = 0 - k_{a-Bob} * H(C_b)
$$
The \\( ScriptOffset\\) can then be constructed as:
$$
scriptoffset = scriptoffset_{alice} + scriptoffset_{Bob}
$$

In this method it is crucial that the Alice's script key \\( k_s\\) keeps hidden. But with the method provided Bob cannot construct \\( k_s\\) as he only sees the public part\\( K_s\\). Alice helps create the \\( ScriptOffset\\) and although her key is now part of the commitment \\( C_b\\) this key is not used after transaction mining. They can then both publish the completed transaction with the completed \\( ScriptOffset\\).

Any BaseNode can now validate the \\( ScriptOffset\\) and the normal MW transaction. They can also check and prove that Alice did sign the script and provided the correct key. 

### One sided payment

For this use case we have Alice, who pays Bob. But Bob's wallet is not online. In this scenario, the transaction will be:

$$
C_a > C_b
$$

Alice owns \\( C_a \\) and in this case the attached script it not important and has zero effect on the transactions. Because Bob is offline at the time of the transaction, Alice has to create the entire transaction herself. But a one sided transaction needs some out of bound communication. Alice requires a Public key from Bob and needs to supply the blinding factor \\( C_b.k\\) from the Commitment \\( C_b\\) to Bob. 

Alice knowns all the blinding factor  \\( C_a.k\\) and known the script redeeming private key \\( k_s\\). Alice and Bob needs to know the blinding factor \\( C_b.k\\) but Bob does not need to know the receiver pubkey \\( C_b.k_a\\). 

Alice will create the entire transaction including the \\( ScriptOffset\\). Bob is not required for any part of this transaction. But Alice will include a script on \\( C_b\\) of (`CheckSigVerify`) with the public key Bob provided out of band for her. 

Any baseNode can now verify that the transaction is complete, verify the signature on the script, and verify the \\( ScriptOffset\\).

For Bob to claim his commitment, \\( C_b\\) he requires the blinding factor \\( C_b.k\\) and he requires his own public key for the script.
Although Alice knowns the blinding factor \\( C_b.k\\), once mined she cannot claim this as she does not know the private key part fo the of script (`CheckSigVerify`) to unlock the script. 

### HTLC like script

In this use case we have a script that controls to whom it is spend. We have Alice and Bob. Alice owns the commitment \\( C_a). She and Bob work together to create \\( C_c\\). But we dont yet know hom can spend the newly created \\( C_s\\). 

$$
C_a > C_s > C_x
$$


In this use case Alice and Bob work together to create \\( C_s\\). 
Because Alice owns \\( C_a\\) she should have the blinding factor \\( C_a.k\\) and know the script spending conditions. 
Alice and Bob both create a  \\( K_a\\) key, with:
$$
K_a = K_{a-Alice} + K_{a-Bob}
$$

In this case, Alice and Bob both create the normal transaction.  Alice and Bob have to ensure that \\( K_a\\) is inside of the commitment \\( C_s\\). Alice will fill in the script with her \\( K_s\\) to unlock the commitment \\( C_a\\). 
Alice will construct her part of the \\( ScriptOffset\\) with:
$$
scriptoffset_{Alice} = k_s - k_{a-Alice} * H(C_s)
$$

Bob will construct his part of the \\( ScriptOffset\\) with:
$$
scriptoffset_{Bob} = 0 - k_{a-Bob} * H(C_s)
$$
The \\( ScriptOffset\\) can then be constructed as:
$$
scriptoffset = scriptoffset_{alice} + scriptoffset_{Bob}
$$

The blinding factor \\( C_s.k\\) can be safely shared between Bob and Alice. And because both use the \\( Hash(C_s)\\) in the construction of their \\( ScriptOffset\\) parts. Both can know that neither party can change any detail of \\( C_s\\) including the script. 

As soon as \\( C_s\\) is mined, Alice and Bob now have a combined Commitment on the blockchain with some spending conditions that require the fulfillment of the script conditions to spend. 

The spending case of either Alice or Bob claiming the commitment \\( C_s\\) is not going to be handled here as it is exactly the same as all the above cases. But The case of Alice and Bob spending this together is going to be explained here. 

In this case, both Alice and Bob want to spend to one or more utxo together. Alice and Bob both create a \\( k_a\\) and need to know their own \\( k_s\\)

Alice will construct her part of the \\( ScriptOffset\\) with:
$$
scriptoffset_{Alice} = k_{s-Alice} - k_{a-Alice} * H(C_x)
$$

Bob will construct his part of the \\( ScriptOffset\\) with:
$$
scriptoffset_{Bob} = k_{s-Bob} - k_{a-Bob} * H(C_x)
$$
The \\( ScriptOffset\\) can then be constructed as:
$$
scriptoffset = scriptoffset_{alice} + scriptoffset_{Bob}
$$

With this both Alice and Bob have agreed to the terms of commitment \\( C_x\\) lock that in. Both need to sign the input script with their respective \\( k_s\\) keys. And Both need to create their Offset. In this case, both \\( K_s\\) and \\( K_a\\) are aggregate keys. 
Because the script resolves to an aggregate key \\( K_s\\) neither Alice nor Bob can claim the commitment \\( C_s\\) without the other party's key. 

A BaseNode validating the transaction will also not be able to tell this is an aggregate transaction as all keys are aggregated schnorr signatures. But it will be able to validate that the script input is correctly signed, thus the output public key is correct.  And that the \\( ScriptOffset\\) is correctly calculated, meaning that the commitment \\( C_x\\) is the correct UTXO for the transaction. 
### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is spent
in the same block it was created. Tari_script with its ScriptOffset will stop cut-through completely as it currently works. It will still allow pruning thou. Cut-through is still possible if the original owner participates. Example Alice, pays Bob, who pays Carol. Cut-through can happen only if Alice and Carol negotiate a new transaction. 

This will ensure that the original owner is happy with the spending the transaction to a new party, eg verified the spending conditions like a script.

### Script lock key generation

At face value, it looks like the burden for wallets has doubled, since each UTXO owner has to remember three private keys,
the spend key, \\( k_i \\) and the receiver key \\( k_{a} \\) and the script key \\( k_{s} \\) . In practice, the other keys can be
deterministically derived from the spend key. For example, the \\( k_{a} \\) can be equal to the hash of the \\( k_i \\).

### Replay attacks
TODO

### Blockchain bloat

The most obvious drawback to TariScript is the effect it will have on blockchain size. The addition of the script and 
script signature, it also adds two public keys to every UTXO. This can eventually be pruned, but will increase
storage and bandwidth requirements.

Input size in a block will now be much bigger as each input was previously just a commitment and an OuputFeatures. Each input now includes a script,input_data, script_signature and extra public key. This could be improved by not sending the input again, but just sending the hash of the input, input_data and script_signature. But this will still be larger than inputs are currently. 

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

Note on the notation used in this document:
$$
 x \cdot{k}
$$
This means the value x over curve K. 
$$
 x.k
$$
This means the value k from the value x

### Credits

Thanks to [@philipr-za](https://github.com/philipr-za) and [@SWvheerden](https://github.com/SWvheerden) for their input
and contributions to this RFC.

[data commitments]: https://phyro.github.io/grinvestigation/data_commitments.html
[LIP-004]: https://github.com/DavidBurkett/lips/blob/master/lip-0004.mediawiki
[Scriptless script]: https://tlu.tarilabs.com/cryptography/scriptless-scripts/introduction-to-scriptless-scripts.html
[Handshake white paper]: https://handshake.org/files/handshake.txt
