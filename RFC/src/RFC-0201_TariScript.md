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


$$
\newcommand{\script}{\alpha} % utxo script
\newcommand{\scripthash}{ \sigma }
\newcommand{\input}{ \theta }
\newcommand{\HU}{\mathrm{HU}} % UTXO hash
\newcommand{\cat}{\Vert}
\newcommand{\so}{\gamma} % script offset
\newcommand{\rpc}{\beta} % Range proof commitment
\newcommand{\hash}[1]{\mathrm{H}\bigl(#1\bigr)}
$$

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
use cases will collapse and can be achieved under a single set of (relatively minor) modifications and additions to the
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

Note that range proofs can be discarded after a UTXO is spent. This entails that the global security guarantees of
Mimblewimble are not that every transaction in history was valid from an inflation perspective, but that the net effect
of all transactions lead to zero spurious inflation. This sounds worse than it is, since locally, every individual
transaction is checked for validity at the time of inclusion in the blockchain.

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

* A commitment to the spending script is recorded in the transaction UTXO.
* UTXOs also define a new, _receiver public key_.
* After the script is executed, the execution stack must either be empty or contain exactly one value that will be interpreted as a public key.
  One can prove ownership of a UTXO by demonstrating knowledge of both the commitment blinding factor, _and_ the script key.
  If the stack is empty, then any private key will satisfy the script key requirement.
* The script key signs the script input data.
* The receiver and script keys are used in conjunction to create a _script offset_, which used in the consensus balance to prevent a
  number of attacks.

### UTXO data commitments

The script, as well as other UTXO metadata, such as the output features are committed to in the range proof. As we will
describe later, the notion of a script offset is introduced to prevent cut-through and forces the preservation of these
commitments until they are recorded into the blockchain.
 
There are several changes to the protocol data structures that must be made to allow this scheme to work. 

The first is a relatively minor adjustment to the transaction output definition.
The second is the inclusion of script input data and an additional public key in the transaction input field.
Third is the way we calculate and validate the range proof.

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

Under TariScript, this definition changes to accommodate the script and the receiver public key:

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
    /// The receiver pubkey, K_R
    receiver_pub_key: PublicKey
}
```

We now introduce some [Notation](#notation).

The commitment definition is unchanged:

$$
C_i = v_i \cdot H  + k_i \cdot G
$$

We update \\( \rpc_i \\), the range proof commitment, to be the hash of the serialised script, output features and
receiver public key as follows:

$$
  \rpc_i = \hash{\script_i \cat \mathrm{F_i} \cat K_{Ri}}
$$

Wallets now generate the range proof with

$$ k_i + \rpc_i $$

rather than just \\( k_i \\).

Note that:
* The UTXO has a positive value `v` like any normal UTXO. 
* The script and the output features can no longer be changed by the miner or any other party. Once mined, the owner can
  also no longer change the script or output features without invalidating the range proof.
* We dont need to provide the complete script on the output, only the script hash.

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

The updated input definition is:

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
    /// The block height that the UTXO was mined 
    height : u64
    /// A signature with K_s, signing the script and input data
    script_signature: Signature,
    /// The receiver pubkey, K_R
    receiver_pubkey: PublicKey
}
```

The input data to the script is signed with resolving script public key \\(K_s \\), which proves that the spender provides
the input to the script.

The height is the height this UTXO was mined at. This is to stop replay attacks, see [Replay attacks](#Replay attacks).

The `script_signature` is a Schnorr signature. It signs the script, the script input and the height the UTXO was originally mined:

$$
  s_{Si} = r_{Si} + \hash{\alpha_i \cat \input_i \cat h_i} k_{Si}
$$ 

Note that it is possible to reduce the wire size of the input by sending only a hash of input bundled with rest of the script information.

### Consensus changes

The Mimblewimble balance for blocks and transactions stays the same.

The range proof commits all the values comprising the transaction output. So now instead of verifying the range proof
using the standard output commitment, \\( C_i \\), we use the modified commitment,

$$ \hat{C_i} = v_i\cdot H  + \bigl( k_i + \rpc_i \bigr) \cdot G  $$

We can then verify the range proof. If the range proof is valid, we know that the value v, is positive and that none of
the values have been changed.

The new receiver pubkey ( \\(K_R \\) )  and the script public key ( \\(K_S \\) ) are combined to create a script offset,
\\( \so \\).

\\( \so\\) is calculated and verified as part of every block and transaction validation. This is calculated as follows:

$$
\so = \sum_i\mathrm{k_{Si}} - \sum_j(\mathrm{k_{Rj}  \HU_j})\; \text{for each input}, i\, \text{and each output}, j
$$

where \\(  \HU_i \\)  is the serialized hash of the entire output _sans_ the range proof.

For every block and/or transactions, an accompanying \\( \so \\) needs to be provided.

**TODO** Define changes to block and transaction structures to accommodate the script offsets.

In addition to the changes given above, there are consensus rule changes for transaction and block validation.

For every valid block or transaction,

1. Validate range proofs against \\( \hat{C_i} \\) rather than \\( C_i \\).
2. Check that the script signature, \\( s_{Si} \\) is valid for every input.
3. The script offset is valid for every block and transaction.

## Examples

Let's cover a few examples to illustrate the new scheme and provide justification for the additional requirements and
validation steps.

### Standard MW transaction

For this use case we have Alice, who sends Bob some Tari in the transaction \\( C_a \Rightarrow C_b \\).
Bob's wallet is  online and is able to countersign the transaction.

This can be done in two ways.

#### Method 1
Alice creates a new transaction spending \\( C_a \\) to a new output \\( C_b \\).

To spend \\( C_a \\), she provides

* A script, \\( \alpha_a \\) such that the script hash, \\( \scripthash_a = \hash{ \alpha_a }\\) matches the blockchain
  record for the UTXO containing \\( C_a \\).
* The script input, \\( \input_a \\).
* The height, \\( h \\), that the UTXO matching \\( C_a \\) was mined.
* A valid signature, \\( s_{Sa} \\) proving that she knows the private key, \\( k_{Sa} \\), corresponding to
  \\( K_{Sa} \\), the public key left on the stack after executing \\( \script_a \\) with \\( \input_a \\).

Since Bob will be countersigning the transaction, Alice can essentially construct a traditional MW output for Bob:

She creates a new proto-output:
* with the value _v_; Bob will provide the blinding factor.
* a `NO_OP` script.
* her public nonce, \\( R_a \\) for the excess signature.

Bob will complete his side of the transaction by completing the output:

* Calculating the commitment, \\( C_b = k_b \cdot G + v \cdot H \\),
* Choosing a private receiver key and providing the receiver public key, \\( K_{Rb} \\),
* Choosing a private script key, \\( k_{Sb} \\),
* Creating a range proof for \\( \hat{C}_b = (k_b + \rpc_b) \cdot G + v \cdot H \\),
* Signing the kernel excess as usual:
$$
  s_b = r_b + k_b \hash{R_a + R_b \cat f \cat m }
$$
* And returning the partial signature along with his nonce, \\( R_b \\), back to Alice.

Any BaseNode can validate the transaction as with normal MW transactional rules. And any node can validate the script and script signature. As well as validate the \\( \so\\) with:
$$
\so \cdot{G} = K_{Sa} - K_{Rb}* \HU_b
$$



When bob spends this output, he will use \\( K_{Sb}\\) as his script input and sign it with his private key
\\( k_{Sb}\\) and construct the script offset, \\( \so_b \\) as follows:

$$
\so_b = k_{Sb} - k_{Rb} \HU_b
$$

**TODO** - How can either party construct \\( \so_t \\) for this transaction? Alice knows k_sa and Bob knows k_rb?

#### Method 2
Alice stored her her commitment \\( C_a\\) with script. This means that the script needs to be provided by some pubkey before unlocking and resolving to a known pubkey.

Alice and Bob both create a  \\( K_R\\) key, with:
$$
K_R = K_{Rb-Alice} + K_{Rb-Bob}
$$

In this case, Alice and Bob both create the normal transaction.  Except here Bob has the fill in the aggregated \\( K_{Rb}\\) inside of his commitment \\( C_b\\). Alice will fill in the script with her \\( K_s\\) to unlock the commitment \\( C_a\\). 
Alice will construct her part of the \\( \so\\) with:
$$
\so_a = k_{Sa} - k_{Rb-Alice} * \HU_b
$$

Bob will construct his part of the \\( \so\\) with:
$$
\so_b = 0 - k_{Rb-Bob} * \HU_b
$$
The \\( \so\\) can then be constructed as:
$$
\so = \so_a + \so_b
$$

In this method it is crucial that the Alice's script key \\( k_{Sa}\\) keeps hidden. But with the method provided Bob cannot construct \\( k_{Sa}\\) as he only sees the public part\\( K_{Sa}\\). Alice helps create the \\( \so\\) and although her key is now part of the commitment \\( C_b\\) this key is not used after transaction mining. Because Alice owns \\(C_a\\), she needs to sign the input on the transaction. They can then both publish the completed transaction with the completed \\( \so\\).

Any BaseNode can now validate the \\( \so\\) and the normal MW transaction. They can also check and prove that Alice did sign the script and provided the correct key.

### One sided payment

For this use case we have Alice, who pays Bob. But Bob's wallet is not online. In this scenario, the transaction will be:

$$
C_a > C_b
$$

Alice owns \\( C_a \\) and in this case the attached script it not important and has zero effect on the transactions. Because Bob is offline at the time of the transaction, Alice has to create the entire transaction herself. But a one sided transaction needs some out of bound communication. Alice requires a Public key from Bob and needs to supply the blinding factor \\( k_b\\) from the Commitment \\( C_b\\) to Bob. 

Alice knowns the blinding factor  \\( k_{Rb}\\) and knowns the script redeeming private key \\( k_{Sa}\\). Alice and Bob needs to know the blinding factor \\( k_b\\) but Bob does not need to know the receiver pubkey \\( k_{Rb}\\). 

Alice will create the entire transaction including the \\( \so\\). Bob is not required for any part of this transaction. But Alice will include a script on \\( C_b\\) of (`CheckSigVerify`) with the public key Bob provided out of band for her.

Any baseNode can now verify that the transaction is complete, verify the signature on the script, and verify the \\( \so\\).

For Bob to claim his commitment, \\( C_b\\) he requires the blinding factor \\( k_b\\) and he requires his own public key for the script.
Although Alice knowns the blinding factor \\( k_b\\), once mined she cannot claim this as she does not know the private key part fo the of script (`CheckSigVerify`) to unlock the script. 

### HTLC like script

In this use case we have a script that controls to whom it is spend. We have Alice and Bob. Alice owns the commitment \\( C_a). She and Bob work together to create \\( C_s\\). But we dont yet know hom can spend the newly created \\( C_s\\). 

$$
C_a > C_s > C_x
$$


In this use case Alice and Bob work together to create \\( C_s\\). 
Because Alice owns \\( C_a\\) she should have the blinding factor \\( C_a.k\\) and know the script spending conditions. 
Alice and Bob both create a  \\( K_{Rs}\\) key, with:
$$
K_{Rs} = K_{R-Alice} + K_{R-Bob}
$$

In this case, Alice and Bob both create the normal transaction.  Alice and Bob have to ensure that \\( K_{Rs}\\) is inside of the commitment \\( C_s\\). Alice will fill in the script with her \\( k_{Sa}\\) to unlock the commitment \\( C_a\\). 
Alice will construct her part of the \\( \so\\) with:
$$
\so_{Alice} = k_{Sa} - k_{R-Alice} * \HU_s
$$

Bob will construct his part of the \\( \so\\) with:
$$
\so_{Bob} = 0 - k_{R-Bob} * \HU_s
$$
The \\( \so\\) can then be constructed as:
$$
\so = \so_{alice} + \so_{Bob}
$$

The blinding factor \\( k_s\\) can be safely shared between Bob and Alice. And because both use the \\( \HU_s\\) in the construction of their \\( \so\\) parts. Both can know that neither party can change any detail of \\( C_s\\) including the script.

As soon as \\( C_s\\) is mined, Alice and Bob now have a combined Commitment on the blockchain with some spending conditions that require the fulfillment of the script conditions to spend. 

The spending case of either Alice or Bob claiming the commitment \\( C_s\\) is not going to be handled here as it is exactly the same as all the above cases. But The case of Alice and Bob spending this together is going to be explained here. 

In this case, both Alice and Bob want to spend to one or more utxo together. Alice and Bob both create a \\( k_{Rx}\\) and need to know their own \\( k_{Rx}\\)

Alice will construct her part of the \\( \so\\) with:
$$
\so_{Alice} = k_{Ss-Alice} - k_{Rx-Alice} * \HU_x
$$

Bob will construct his part of the \\( \so\\) with:
$$
\so_{Bob} = k_{Ss-Bob} - k_{Rx-Bob} * \HU_x
$$
The \\( \so\\) can then be constructed as:
$$
\so = \so_{alice} + \so_{Bob}
$$

With this both Alice and Bob have agreed to the terms of commitment \\( C_x\\) lock that in. Both need to sign the input script with their respective \\( k_S\\) keys. And Both need to create their Offset. In this case, both \\( K_S\\) and \\( K_R\\) are aggregate keys. 
Because the script resolves to an aggregate key \\( K_s\\) neither Alice nor Bob can claim the commitment \\( C_s\\) without the other party's key. 

A BaseNode validating the transaction will also not be able to tell this is an aggregate transaction as all keys are aggregated schnorr signatures. But it will be able to validate that the script input is correctly signed, thus the output public key is correct.  And that the \\( \so\\) is correctly calculated, meaning that the commitment \\( C_x\\) is the correct UTXO for the transaction.
### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is spent
in the same block it was created. Tari_script with its script offset will stop cut-through completely as it currently works. It will still allow pruning thou. Cut-through is still possible if the original owner participates. Example Alice, pays Bob, who pays Carol. Cut-through can happen only if Alice and Carol negotiate a new transaction.

This will ensure that the original owner is happy with the spending the transaction to a new party, eg verified the spending conditions like a script.

### Script lock key generation

At face value, it looks like the burden for wallets has doubled, since each UTXO owner has to remember three private keys,
the spend key, \\( k_i \\) and the receiver key \\( k_{R} \\) and the script key \\( k_{S} \\) . In practice, the other keys can be
deterministically derived from the spend key. For example, the \\( k_{R} \\) can be equal to the hash of the \\( k_i \\). The the receiver key \\( k_{R} \\) is also not required to be stored as this key is only used in creation of the script offset with the purpose of proving that the correct output is included.

### Replay attacks

With a lot of these schemes it is possible to perform replay attacks. Look at the following scenario. We have Alice, Bob and Carol. To make the use case more clear lets assume Bob is a merchant. Alice buys some stuff from Bob. And on a later point in time Bob pays Carol:

$$
C_a > C_b > C_c
$$

This is all fine and secure. But lets say at a later stage. Alice pays Bob again. But Alice uses the exact same commitment, script and public keys to pay Bob.

$$
C_a' > C_b'
$$

After Bob ships his goods to Alice, Carol can just take the commitment \\( C_b \\) because \\( C_b == C_b' \\) and she already has a transaction with all the correct signatures to claim \\( C_b \\) to a commitment under her control \\( C_c' \\).

But to ensure that a script is only valid once, we need to sign the block height that the original UTXO was mined at. So going back to the case of:
$$
C_b > C_c
$$
 Bob would have signed that block height \\( C_b \\) was mined. This means that when Carol tries to publish a transaction for:
$$
C_b' > C_c'
$$

She cant. Because she would need to sign the input of the script with the block height \\( C_b' \\) was mined at. She cant do it, since she does not have the private key for the script. And the signing data changed between :
$$
C_b > C_c
$$
and$$
C_b' > C_c'
$$
so her old transaction data is not valid for the new transaction. And for her to be able to spend \\( C_b' \\). Bob would have to unlock the script and spend it to her with his approval. 
### Blockchain bloat

The most obvious drawback to TariScript is the effect it will have on blockchain size. The addition of the script and script signature, it also adds a public key to every UTXO. This can eventually be pruned, but will increase storage and bandwidth requirements.

Input size in a block will now be much bigger as each input was previously just a commitment and an OuputFeatures. Each input now includes a script,input_data, script_signature and extra public key. This could be improved by not sending the input again, but just sending the hash of the input, input_data and script_signature. But this will still be larger than inputs are currently. 

The additional range proof validations and signature checks significantly hurt performance. Range proof checks are particularly expensive but we dont increase the number. To improve overall block validation, batch range proof validations should be employed to mitigate this expense.

### Fodder for chain analysis

Another potential drawback of TariScript is the additional information that is handed to entities wishing to perform chain
analysis. Having scripts attached to outputs will often clearly mark the purpose of that UTXO. Users may wish to re-spend
outputs into vanilla, default UTXOs in a mixing transaction to disassociate Tari funds from a particular script.

## Notation


Where possible, the "usual" notation is used to denote terms commonly found in cryptocurrency literature. New terms introduced by Tariscript are assigned greek lowercase letters in most cases.  
The capital letter subscripts, _R_ and _S_ refer to a UTXO _receiver_ and _script_ respectively.

| Symbol                  | Definition                                                                                                                         |
|:------------------------|:-----------------------------------------------------------------------------------------------------------------------------------|
| \\( \script_i \\)       | An output script for output _i_, serialised to binary                                                                              |
| \\( h_i \\)             | Block height that UTXO \\(i\\) was previously mined.                                                                               |
| \\(  \HU_i \\)          | The hash of the full UTXO _i_ _sans_ range proof.                                                                                  |
| \\( F_i \\)             | Output features for UTXO _i_.                                                                                                      |
| \\( f_t \\)             | transaction fee for transaction _t_.                                                                                               |
| \\( m_t \\)             | metadata for transaction _t_. Currently this includes the lock height.                                                             |
| \\( \alpha_i \\)        | The 256-bit Blake2b hash of an output script, \\( \script_i \\)                                                                    |
| \\( k_{Ri}\, K_{Ri} \\) | The private - public keypair for the UTXO receiver.                                                                                |
| \\( k_{Si}\, K_{Si} \\) | The private - public keypair for the script key. The script, \\( \script_i \\) resolves to \\( K_S \\) after completing execution. |
| \\( \rpc_i \\)          | Auxilliary data committed to in the range proof. \\( \rpc_i = \hash{ \script_i \cat F_i \cat K_{Ri} } \\)                          |
| \\( \so_t \\)           | The script offset for transaction _t_. \\( \so_t = \sum_j{ k_{Sjt}} - \sum_j{k_{Rjt}\cdot\HU_i} \\)                                |
| \\( C_i \\)             | A Pedersen commitment,  i.e. \\( k_i \cdot{G} + v_i \cdot H \\)                                                                    |
| \\( \hat{C}_i \\)       | A modified Pedersen commitment, \\( \hat{C}_i = (k_i + \rpc_i)\cdot{G} + v_i\cdot H  \\)                                           |
| \\( \input_i \\)        | The serialised input for script \\( \script_i \\)                                                                                  |
| \\( s_{Si} \\)          | A script signature for output \\( i \\). \\( s_{Si} = r_{Si} + k_{Si}\hash{\alpha_i \cat \theta_i \cat h_i} \\)                    |


### Credits

[@CjS77](https://github.com/CjS77)
[@philipr-za](https://github.com/philipr-za) 
[@SWvheerden](https://github.com/SWvheerden) 

[data commitments]: https://phyro.github.io/grinvestigation/data_commitments.html
[LIP-004]: https://github.com/DavidBurkett/lips/blob/master/lip-0004.mediawiki
[Scriptless script]: https://tlu.tarilabs.com/cryptography/scriptless-scripts/introduction-to-scriptless-scripts.html
[Handshake white paper]: https://handshake.org/files/handshake.txt
