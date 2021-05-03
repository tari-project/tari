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
\newcommand{\HU}{\mathrm{U}} % UTXO hash
\newcommand{\cat}{\Vert}
\newcommand{\so}{\gamma} % script offset
\newcommand{\rpc}{\beta} % Range proof commitment
\newcommand{\hash}[1]{\mathrm{H}\bigl({#1}\bigr)}
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

Some smart contract features are possible, or partly possible in vanilla Mimblewimble using [Scriptless script], such as

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

This means that in a very hand-wavy sort of way, there ought to be no reason that Tari Script is not workable.

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
* We must be able to prove that the UTXO originator provides the script hash and no-one else.
* The scripts and their redeeming inputs must be stored on the block chain. In particular, the input data must not be
  malleable.

The next section discusses the specific proposals for achieving these requirements.

## Protocol modifications

At a high level, Tari script works as follows:

* A commitment to the spending script is recorded in the transaction UTXO.
* UTXOs also define a new, _offset public key_.
* After the script is executed, the execution stack must contain exactly one value that will be interpreted as a public key.
  One can prove ownership of a UTXO by demonstrating knowledge of both the commitment blinding factor, _and_ the script key.
* The script key signs the script input data.
* The offset and script keys are used in conjunction to create a _script offset_, which used in the consensus balance to prevent a
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

Under TariScript, this definition changes to accommodate the script and the offset public keys:

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
    /// The offset pubkey, K_O
    script_offset_public_key: PublicKey
}
```

We now introduce some [Notation](#notation).

The commitment definition is unchanged:

$$
C_i = v_i \cdot H  + k_i \cdot G
$$

We update \\( \rpc_i \\), the range proof commitment, to be the hash of the script hash, output features and
offset public key as follows:

$$
  \rpc_i = \hash{\scripthash_i \cat \mathrm{F_i} \cat K_{Oi}}
$$

Wallets now generate the range proof with

$$ k_i + \rpc_i $$

rather than just \\( k_i \\).

Note that:
* The UTXO has a positive value `v` like any normal UTXO. 
* The script and the output features can no longer be changed by the miner or any other party. Once mined, the owner can
  also no longer change the script or output features without invalidating the range proof.
* We don't provide the complete script on the output, just the script hash.

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
    /// A signature with k_s, signing the script, input data, and mined height
    script_signature: Signature,
    /// The offset pubkey, K_O
    script_offset_public_key: PublicKey
}
```

The input data to the script is signed with resolving script public key \\(K_s \\), which proves that the spender provides
the input to the script.

The `height` field is the height this UTXO was mined at. This is to stop [Replay attacks](#replay-attacks).

The `script_signature` is a Schnorr signature. It signs the script, the script input, and the height the UTXO was originally mined:

$$
  s_{Si} = r_{Si} + \hash{ R_{Si} \cat \alpha_i \cat \input_i \cat h_i} k_{Si}
$$ 

### Consensus changes

The Mimblewimble balance for blocks and transactions stays the same.

The range proof commits all the values comprising the transaction output. So now instead of verifying the range proof
using the standard output commitment, \\( C_i \\), we use the modified commitment,

$$ \hat{C_i} = v_i\cdot H  + \bigl( k_i + \rpc_i \bigr) \cdot G  $$

We can then verify the range proof. If the range proof is valid, we know that the value v, is positive and that none of
the values have been changed.

The new offset pubkey, \\(K_O \\),  and the script public key, \\(K_S \\), are combined to create a script offset,
\\( \so \\).

\\( \so\\) is calculated and verified as part of every block and transaction validation. This is calculated as follows:

$$
\so = \sum_i\mathrm{k_{Si}} - \sum_j(\mathrm{k_{Oj}  \HU_j}) \\; \text{for each input}, i,\\, \text{and each output}, j
$$

where \\(  \HU_i \\)  is the serialized hash of the entire output _sans_ the range proof.

Usually, the spenders will know and provide the  \\( k_{Oi} \\), and the new UTXO owners (receivers) will provide the
\\( k_{Sj} \\), but this is not necessarily always the case (see the [Examples](#examples)).

For every block and/or transactions, an accompanying \\( \so \\) needs to be provided.


Currently, Tari, like vanilla Mimblewimble, has a transaction offset in each transaction instance, and an aggregate offset
stored in the header of the block. To accommodate the _script offset_ we need to add a `script_offset` to transactions,
and an aggregated `total_script_offset` to the header.

We modify the transactions to be:

```rust,ignore
pub struct Transaction {
    
    ...
    
    /// A scalar offset that links outputs and inputs to prevent cut-through, enforcing the correct application of
    /// the output script.
    pub script_offset: BlindingFactor,
}
```

And Blockheaders need to be modified to:
```rust,ignore
pub struct BlockHeader {
    
    ...
    
    /// Sum of script offsets for all kernels in this block.
    pub total_script_offset: BlindingFactor,
}
```
One important distinction to make is that the coinbase utxo does not count towards the _script offset_. This is because the coinbase UTXO already has special rules accompanying it and it has no input. Thus we cannot generate a \\( \so \\) for a coinbase transaction.
The coinbase can allow any script and \\( k_{O} \\) as long as the range proof is validly constructed for \\( \hat{C_i} \\) and it does not break any of the rules in [RFC 120](RFC-0120_Consensus.md).

In addition to the changes given above, there are consensus rule changes for transaction and block validation.

For every valid block or transaction,

1. Validate range proofs against \\( \hat{C_i} \\) rather than \\( C_i \\).
2. Check that the script signature, \\( s_{Si} \\) is valid for every input.
3. The script offset is valid for every block and transaction.
4. The script executes successfully using the given input script data.
5. The result of the script is a valid public key, \\( K_S \\).
6. The script signature, \\( s_S \\), is a valid signature for \\( K_S \\) and message \\( \hash{ R_S \cat \script \cat \input \cat h } \\).

## Examples

Let's cover a few examples to illustrate the new scheme and provide justification for the additional requirements and
validation steps.

### Standard MW transaction

For this use case we have Alice who sends Bob some Tari.
Bob's wallet is  online and is able to countersign the transaction.

Alice creates a new transaction spending \\( C_a \\) to a new output \\( C_b \\) (ignoring fees for now).

To spend \\( C_a \\), she provides

* A script, \\( \alpha_a \\) such that the script hash, \\( \scripthash_a = \hash{ \alpha_a }\\) matches the blockchain
  record for the UTXO containing \\( C_a \\).
* The script input, \\( \input_a \\).
* The height, \\( h \\), that the UTXO matching \\( C_a \\) was mined.
* A valid signature, \\( (s_{Sa}\, R_{Sa}) \\) proving that she knows the private key, \\( k_{Sa} \\), corresponding to
  \\( K_{Sa} \\), the public key left on the stack after executing \\( \script_a \\) with \\( \input_a \\).
* An offset public key, \\( k_{Ob} \\).

Since Bob will be countersigning the transaction, Alice can essentially construct a traditional MW output for Bob:

She creates a new proto-output:
* with the value _v_; Bob will provide the blinding factor (as per vanilla Mimblewimble),
* her public nonce, \\( R_a \\) for the excess signature (also as per vanilla Mimblewimble),
* and the hash of a `NO_OP` script (See [RFC 202](RFC-0202_TariScriptOpcodes.md)).

Alice sends Bob this proto-UTXO, along with her signature nonce, as per the standard Mimblewimble protocol. However, she
also provides Bob with \\( K_{Ob} \\), the script offset public key.

Bob can then complete his side of the transaction by completing the output:

* Calculating the commitment, \\( C_b = k_b \cdot G + v \cdot H \\),
* Choosing a private script key, \\( k_{Sb} \\),
* Creating a range proof for \\( \hat{C}_b = (k_b + \rpc_b) \cdot G + v \cdot H \\), with

  $$
    \rpc_b = \hash{\scripthash_b \cat F_b \cat K_{Ob} }
  $$

Bob then signs the kernel excess as usual:
$$
  s_b = r_b + k_b \hash{R_a + R_b \cat f \cat m }
$$

Bob returns the UTXO and partial signature along with his nonce, \\( R_b \\), back to Alice.

Alice will then complete the transaction by calculating the script offset, \\( \so\\):
$$
\so  = k_{Sa} - k_{Ob} \HU_b
$$

She can then construct and broadcast the transaction to the network.

#### Transaction validation

Base nodes validate the transaction as follows:

* They check that the usual Mimblewimble balance holds by summing inputs and outputs and validating against the excess
  signature. This check does not change. Nor do the other validation rules, such as confirming that all inputs are in
  the UTXO set etc.
* The range proof of Bob's output is validated with \\( \hat{C}_b \\) rather than \\( C_b \\),
* The script signature on Alice's input is validated against the script, input and mining height,
* The script hash must match the hash of the provided input script,
* The input script must execute successfully using the provided input data; and the script result must be a valid public key,
  \\( K_{Sa} \\).
* The script offset is verified by checking that the balance
  $$
    \so \cdot{G} = K_{Sa} - \HU_b K_{Ob}
  $$
  holds.

When the transaction is included in a block, the total offset for the block is validated, in an analogous fashion to how
the excess offset is used.

Finally, when Bob spends this output, he will use \\( K_{Sb} \\) as his script input and sign it with his private key
\\( k_{Sb} \\). He will choose a new \\( K_{Oc} \\) to give to the recipient, and he will construct the script offset,
\\( \so_b \\) as follows:

$$
\so_b = k_{Sb} - k_{Oc} \HU_b
$$

### One sided payment

In this example, Alice pays Bob, who is not available to countersign the transaction, so Alice initiates a one-sided payment,

$$
C_a \Rightarrow  C_b
$$

Once again, transaction fees are ignored to simplify the illustration.

Alice owns \\( C_a \\) and provides the required script to spend the UTXO as was described in the previous cases.

Alice needs a public key from Bob, \\( K_{Sb} \\) to complete the one-sided transaction. This key can be obtained
out-of-band, and might typically be Bob's wallet public key on the Tari network.

Bob requires the value \\( v_b \\) and blinding factor \\( k_b \\) to claim his payment, but he needs to be able to claim it without asking Alice for them.

This information can be obtained by using Diffie-Hellman and Bulletproof rewinding. If the blinding factor \\( k_b \\) was calculated with Diffie-Hellman using the offset public keypair, (\\( k_{Ob} \\),\\( K_{Ob} \\)) as sender keypair and
the keypair, (\\( k_{Sb} \\),\\( K_{Sb} \\)) as the receiver keypair, the blinding factor \\( k_b \\) can be securely calculated without communication.

Alice uses Bob's public key to create a shared secret, \\( k_b \\) for the output commitment, \\( C_b \\), using
Diffie-Hellman key exchange.

Alice calculates \\( k_b \\) as
$$
    k_b = k_{Ob} * {K_Sb}
$$

Next Alice next uses Bulletproof rewinding to encrypt the value \\( v_b \\) into the the Bulletproof for the commitment \\( C_b \\). For this she uses (\\( k_{rewind} =  Hash(k_{b}) \\) as the rewind_key and (\\( k_{blinding} =  Hash(Hash(k_{b})) \\) as the blinding key.
*Note, deriving the keys here should be secure, but should be confirmed before mainnet.

Alice knows the script-redeeming private key \\( k_{Sa}\\) for the transaction input.

Alice will create the entire transaction including, generating a new offset keypair and calculating the script offset,

$$
    \so = k_{Sa} - k_{Ob} \cdot \HU_b
$$

For the script hash, she provides the hash of a script that locks the output to Bob's public key, `PushPubkey(K_Sb)`.
This script will only resolve successfully if the spender can provide a valid signature as input that demonstrates proof
of knowledge of \\( k_{Sb} \\) which only Bob knows.

Any base node can now verify that the transaction is complete, verify the signature on the script, and verify the script
offset.

For Bob to claim his commitment he will scan the blockchain for a known script hash because he knowns that the script will be `PushPubkey(K_Sb)` he can scan for that hash. In this case, the script hash is analogous to an address in Bitcoin or Monero. Bob's wallet can scan the blockchain
looking for hashes that he would know how to resolve. For all outputs that he discovers this way, Bob would need to know
who the sender is so that he can derive the shared secret.

When Bob's wallet spots a known hash he requires he requires the blinding factor, \\( k_b \\) and the value \\( v_b \\). First he uses Diffie-Hellman to calculate \\( k_b \\). 

Bob calculates \\( k_b \\) as
$$
    k_b = K_{Ob} * {k_Sb}
$$

Next Bob's wallet calculates \\( k_{rewind} \\), using \\( k_{rewind} =  Hash(k_{b})\\) and (\\( k_{blinding} =  Hash(Hash(k_{b})) \\), using those to rewind the Bulletproof to get the value \\( v_b \\). 

Because Bob's wallet already knowns \\( k_Sb \\), he now knows all the values required to spend the commitment \\( C_b \\)

For Bob's part, when he discovers one-sided payments to himself, he should spend them to new outputs using a traditional
transaction to thwart any potential horizon attacks in the future.

To summarise, the information required for one-sided transactions is as follows:

| Transaction input | Symbols                               | Knowledge                                                       |
|:------------------|:--------------------------------------|:----------------------------------------------------------------|
| commitment        | \\( C_a = k_a \cdot G + v \cdot H \\) | Alice knows spend key and value                                 |
| features          | \\( F_a \\)                           | Public                                                          |
| script            | \\( \alpha_a \\)                      | Public, can verify that \\( \hash{\alpha_a} = \scripthash_a \\) |
| script input      | \\( \input_a \\)                      | Public                                                          |
| height            | \\( h_a \\)                           | Public                                                          |
| script signature  | \\( s_{Sa}, R_{Sa} \\)                | Alice knows \\( k_{Sa},\\, r_{Sa} \\)                           |
| offset public key | \\( K_{Oa} \\)                        | Not used in this transaction                                    |

| Transaction output | Symbols                               | Knowledge                                                              |
|:-------------------|:--------------------------------------|:-----------------------------------------------------------------------|
| commitment         | \\( C_b = k_b \cdot G + v \cdot H \\) | Alice and Bob know the spend key and value                             |
| features           | \\( F_b \\)                           | Public                                                                 |
| script hash        | \\( \scripthash_b \\)                 | Script is effectively public. Only Bob knows the correct script input. |
| range proof        |                                       | Alice and Bob know opening parameters                                  |
| offset public key  | \\( K_{Ob} \\)                        | Alice knows \\( k_{Ob} \\)                                             |


### HTLC-like script

In this use case we have a script that controls where it can be spent. The script is out of scope for this example, but
has applies the following rules:

* Alice can spend the UTXO unilaterally after block _n_, **or**
* Alice and Bob can spend it together.

This would be typically what a lightning-type channel requires.

Alice owns the commitment \\( C_a \\).
She and Bob work together to create \\( C_s\\).
But we don't yet know who can spend the newly created \\( C_s\\) and under what conditions this will be.

$$
C_a \Rightarrow  C_s \Rightarrow  C_x
$$

Alice owns \\( C_a\\), so she knows the blinding factor \\( k_a\\) and the correct input for the script's spending conditions.
Alice also generates the offset keypair, \\( (k_{Os}, K_{Os} )\\).

Now Alice and Bob proceed with the standard transaction flow.

Alice and Bob have to ensure that \\( K_{Os}\\) is inside of the commitment \\( C_s\\).
Alice will fill in the script with her \\( k_{Sa}\\) to unlock the commitment \\( C_a\\).
Because Alice owns \\( C_a\\) she needs to construct \\( \so\\) with:

$$
\so = k_{Sa} - k_{Ob} \cdot \HU_s
$$


The blinding factor, \\( k_s\\) can be generated using a Diffie-Hellman construction.
The commitment \\( C_s\\) needs to be constructed with the script the Bob agrees on. Until it is mined, Alice could modify
the script via double-spend and thus Bob must wait until the transaction is confirmed before accepting the conditions of
the smart contract between Alice and himself.

Once the UTXO is mined, both Alice and Bob possess all the knowledge required to spend the \\( C_s \\) UTXO. It's only
the conditions of the script that will discriminate between the two.

The spending case of either Alice or Bob claiming the commitment \\( C_s\\) follows the same flow described in the previous examples, with
the spender proving knowledge of \\( k_{Ss}\\) and "unlocking" the spending script.

The case of Alice and Bob spending \\( C_s \\) together to a new multiparty commitment requires some elaboration.

Assume that Alice and Bob want to spend  \\( C_s \\) co-operatively.
This involves the script being executed in such a way that the resulting public key on the stack is the sum of Alice and
Bob's individual script keys, \\( k_{SsA} \\) and \\( k_{SaB} \\).

The script input needs to be signed by this aggregate key, and so Alice and Bob must each supply a partial signature following
the usual Schnorr aggregate mechanics.

In an analogous fashion, Alice and Bob also generate an aggregate \\( k_{Ox}\\) from their own \\( k_{Ox}\\)s.

To be specific, Alice calculates her portion from

$$
\so_A = k_{SsA} - k_{OxA} \cdot \HU_x
$$

Bob will construct his part of the \\( \so\\) with:
$$
\so_B = k_{SsB} - k_{OxB} \cdot \HU_x
$$

And the aggregate \\( \so\\) is then:

$$
\so = \so_A + \so_B
$$

Notice that in this case, both \\( K_{Ss} \\) and \\( K_{Ox}\\) are aggregate keys.

Notice also that because the script resolves to an aggregate key \\( K_s\\) neither Alice nor Bob can claim the
commitment \\( C_s\\) without the other party's key. If either party tries to cheat by editing the input, the script
validation will fail.

If either party tries to cheat by creating a new output, the offset will not validate correctly as the offset locks the
output of the transaction.

A base node validating the transaction will also not be able to tell this is an aggregate transaction as all keys are
aggregated Schnorr signatures. But it will be able to validate that the script input is correctly signed, thus the
output public key is correct and that the \\( \so\\) is correctly calculated, meaning that the commitment \\( C_x\\) is
the correct UTXO for the transaction.

To summarise, the information required for creating a multiparty UTXO is as follows:

| Transaction input | Symbols                               | Knowledge                                                       |
|:------------------|:--------------------------------------|:----------------------------------------------------------------|
| commitment        | \\( C_a = k_a \cdot G + v \cdot H \\) | Alice knows spend key and value                                 |
| features          | \\( F_a \\)                           | Public                                                          |
| script            | \\( \alpha_a \\)                      | Public, can verify that \\( \hash{\alpha_a} = \scripthash_a \\) |
| script input      | \\( \input_a \\)                      | Public                                                          |
| height            | \\( h_a \\)                           | Public                                                          |
| script signature  | \\( s_{Sa}, R_{Sa} \\)                | Alice knows \\( k_{Sa},\\, r_{Sa} \\)                           |
| offset public key | \\( K_{Oa} \\)                        | Not used in this transaction                                    |

| Transaction output | Symbols                               | Knowledge                                                                                       |
|:-------------------|:--------------------------------------|:------------------------------------------------------------------------------------------------|
| commitment         | \\( C_s = k_s \cdot G + v \cdot H \\) | Alice and Bob know the spend key and value                                                      |
| features           | \\( F_s \\)                           | Public                                                                                          |
| script hash        | \\( \scripthash_s \\)                 | Script is effectively public. Alice and Bob only knows their part of the  correct script input. |
| range proof        |                                       | Alice and Bob know opening parameters                                                           |
| offset public key  | \\( K_{Os} = K_{OsA} + K_{OsB}\\)     | Alice knows \\( k_{OsA} \\), Bob knows \\( k_{OsB} \\). Neither party knows \\( k_{Os} \\)      |

When spending the multi-party input:

| Transaction input | Symbols                               | Knowledge                                                                                                          |
|:------------------|:--------------------------------------|:-------------------------------------------------------------------------------------------------------------------|
| commitment        | \\( C_s = k_s \cdot G + v \cdot H \\) | Alice and Bob know the spend key and value                                                                         |
| features          | \\( F_s \\)                           | Public                                                                                                             |
| script            | \\( \alpha_s \\)                      | Public, can verify that \\( \hash{\alpha_d} = \scripthash_d \\)                                                    |
| script input      | \\( \input_s \\)                      | Public                                                                                                             |
| height            | \\( h_a \\)                           | Public                                                                                                             |
| script signature  | \\( s_{Sa} , R_{Sa} \\)               | Alice knows \\( k_{SaA},\\, r_{SaA} \\), Bob knows \\( k_{SaB},\\, r_{SaB} \\).  Neither party knows \\( k_{Sa}\\) |
| offset public key | \\( K_{Os} \\)                        | As above, Alice and Bob each know part of the offset key                                                           |




### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is spent
in the same block it was created. This makes it so that the intervening UTXO never existed; along with any checks and
balances carried in that UTXO. It's also impossible to prove without additional information that cut-through even occurred
(though one may suspect, since the "one" transaction would contribute two kernels to the block).

In particular, cut-through is devastating for an idea like TariScript which relies on conditions present in the UTXO being
enforced.

This is the reason for the presence of the script offset in the TariScript proposal. It links a UTXO to the input(s)
that created it, and providing the script offset requires knowledge of keys that miners do not possess; thus they are unable
to produce the necessary script offset when attempting to perform cut-through on a pair of transactions.

Cut-through is still possible if the original owner participates. For example Alice, pays Bob, who pays Carol.
Cut-through can happen only if Alice and Carol negotiate a new transaction.

This will ensure that the original owner is happy with the spending the transaction to a new party, e.g. she has verified
the spending conditions like a script.

### Script lock key generation

At face value, it looks like the burden for wallets has tripled, since each UTXO owner has to remember three private keys,
the spend key, \\( k_i \\), the offset key \\( k_{O} \\) and the script key \\( k_{S} \\). In practice, the script key will
often be a static key associated with the user's node or wallet. Even if it is not, the script and offset keys
can be deterministically derived from the spend key. For example, \\( k_{S} \\) could be  \\( \hash{ k_i \cat \alpha} \\).


### Replay attacks

With a lot of these schemes it is possible to perform replay attacks. Look at the following scenario. We have Alice, Bob
and Carol. Assume Bob is a merchant, and Alice buys some stuff from Bob. Later, Bob pays Carol with the output he got from Alice:

$$
C_a \Rightarrow  C_b \Rightarrow  C_c
$$

This is all fine and secure. But let's say at a later stage, Alice pays Bob again, but Alice uses the
_exact same commitment, script and public keys_ to pay Bob:

$$
C_a' \Rightarrow  C_b'
$$

After Bob ships his goods to Alice, Carol can just take the commitment \\( C_b' \\) because \\( C_b == C_b' \\) and she
already has a transaction with all the correct signatures to claim \\( C_b \\).

To ensure that a script is only valid once, we need to sign the block height that the original UTXO was mined at.
So going back to the case of:

$$
C_b \Rightarrow  C_c
$$

Bob would have signed that \\( C_b \\) was mined at block \\( h \\). This means that when Carol tries to publish a transaction for:

$$
C_b' \Rightarrow  C_c'
$$

she would need to sign the input of the script with the block height \\( h' \\) was mined at. However, the signature she
 is trying to re-use is one for block _h_ and her attack is foiled.

### Blockchain bloat

The most obvious drawback to TariScript is the effect it will have on blockchain size. UTXOs are substantially larger,
with the addition of the script, script signature, and a public key to every output.

These can eventually be pruned, but will increase storage and bandwidth requirements.

Input size in a block will now be much bigger as each input was previously just a commitment and an output features.
Each input now includes a script, input_data, the script signature and an extra public key. This could be compacted by
just broadcasting input hashes along with the missing script input data and signature, instead of the full input in
transaction messages, but this will still be larger than inputs are currently.

Every header will also be bigger as it includes an extra blinding factor that will not be pruned away.

The additional range proof validations and signature checks significantly hurt performance. Range proof checks are particularly
expensive. To improve overall block validation, batch range proof validations should be employed to mitigate this expense.

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
| \\( \scripthash_i \\)   | The 256-bit Blake2b hash of an output script, \\( \script_i \\)                                                                    |
| \\( k_{Oi}\, K_{Oi} \\) | The private - public keypair for the UTXO offset key.                                                                              |
| \\( k_{Si}\, K_{Si} \\) | The private - public keypair for the script key. The script, \\( \script_i \\) resolves to \\( K_S \\) after completing execution. |
| \\( \rpc_i \\)          | Auxilliary data committed to in the range proof. \\( \rpc_i = \hash{ \scripthash_i \cat F_i \cat K_{Oi} } \\)                      |
| \\( \so_t \\)           | The script offset for transaction _t_. \\( \so_t = \sum_j{ k_{Sjt}} - \sum_j{k_{Ojt}\cdot\HU_i} \\)                               |
| \\( C_i \\)             | A Pedersen commitment,  i.e. \\( k_i \cdot{G} + v_i \cdot H \\)                                                                    |
| \\( \hat{C}_i \\)       | A modified Pedersen commitment, \\( \hat{C}_i = (k_i + \rpc_i)\cdot{G} + v_i\cdot H  \\)                                           |
| \\( \input_i \\)        | The serialised input for script \\( \script_i \\)                                                                                  |
| \\( s_{Si} \\)          | A script signature for output \\( i \\). \\( s_{Si} = r_{Si} + k_{Si}\hash{R_i \cat \alpha_i \cat \theta_i \cat h_i} \\)                    |

## Extensions

### Covenants

Tari script places restrictions on _who_ can spend UTXOs. It will also be useful for Tari digital asset applications to
restrict _how_ or _where_ UTXOs may be spent in some cases. The general term for these sorts of restrictions are termed
_covenants_. The [Handshake white paper] has a fairly good description of how covenants work.

It is beyond the scope of this RFC, but it's anticipated that Tari Script would play a key role in the introduction of
generalised covenant support into Tari.

### Lock-time malleability

The current Tari protocol has an issue with Transaction Output Maturity malleability. This output feature is enforced in
the consensus rules, but it is actually possible for a miner to change the value without invalidating the transaction.

With TariScript, output features are properly committed to in the transaction and verified as part of the script offset
validation.

### Credits

[@CjS77](https://github.com/CjS77)
[@philipr-za](https://github.com/philipr-za) 
[@SWvheerden](https://github.com/SWvheerden)

Thanks to David Burkett for proposing a method to prevent cut-through and willingness to discuss ideas.

[data commitments]: https://phyro.github.io/grinvestigation/data_commitments.html
[LIP-004]: https://github.com/DavidBurkett/lips/blob/master/lip-0004.mediawiki
[Scriptless script]: https://tlu.tarilabs.com/cryptography/scriptless-scripts/introduction-to-scriptless-scripts.html
[Handshake white paper]: https://handshake.org/files/handshake.txt
