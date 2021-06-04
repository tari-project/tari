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
\newcommand{\input}{ \theta }
\newcommand{\cat}{\Vert}
\newcommand{\so}{\gamma} % script offset
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

[Scriptless script] is a wonderfully elegant technology and inclusion of Tari Script does not preclude the use of
Scriptless script in Tari. However, scriptless scripts have some disadvantages:

* They are often difficult to reason about, with the result that the development of features based on scriptless scripts
  is essentially in the hands of a very select group of cryptographers and developers.
* The use case set is impressive considering that the "scripts" are essentially signature wrangling, but is still 
  somewhat limited.
* Every feature must be written and implemented separately using the specific and specialised protocol designed for that
  feature. That is, it cannot be used as a dynamic scripting framework on a running blockchain.

## Tari Script - a brief motivation

The essential idea of Tari Script is as follows:

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

With Tari Script, once the script has been pruned away, and then there is a re-org to an earlier point on the chain,
then there's no way to ensure that the script was honoured unless you run an archival node.

This is broadly in keeping with the Mimblewimble security guarantees that, in pruned-mode synchronisation, individual 
transactions are not necessarily verified during chain synchronisation.

However, the guarantee that no additional coins are created or destroyed remains intact.

Put another way, the blockchain relies on the network _at the time_ to enforce the Tari Script spending rules. 
This means that the scheme may be susceptible to certain _horizon attacks_.

Incidentally, a single honest archival node would be able to detect any fraud on the same chain and provide a simple 
proof that a transaction did not honour the redeem script.

### Additional requirements

The assumptions that broadly equate scripting with range proofs in the above argument are:

* The script must be committed to the blockchain.
* The script must not be malleable in any way without invalidating the transaction. This restriction extends to all 
  participants, including the UTXO owner.
* We must be able to prove that the UTXO originator provides the script and no-one else.
* The scripts and their redeeming inputs must be stored on the block chain. In particular, the input data must not be
  malleable.

The next section discusses the specific proposals for achieving these requirements.

## Protocol modifications

Please refer to [Notation](#notation), which provides important pre-knowledge for the remainder of the report.

At a high level, Tari Script works as follows:

* The spending script is recorded in the transaction UTXO.
* UTXOs also define a new, _offset public key_ (\\(K\_{O}\\)).
* After the script is executed, the execution stack must contain exactly one value that will be interpreted as a public
  key (\\(K\_{S}\\)). One can prove ownership of a UTXO by demonstrating knowledge of both the commitment _blinding factor_ (\\(k\\)), _and_ the _script key_ (\\(k_\{S}\\)).
* The _script key_, commitment _blinding factor_ and commitment _value_ (\\(v\\)) signs the script input data.
* The _script offset keys_ and _script keys_ are used in conjunction to create a _script offset_ (\\(\so\\)), which used in the 
  consensus balance to prevent a number of attacks.

### UTXO data commitments

The script, as well as other UTXO metadata, such as the output features are signed for with the script offset key to 
prevent malleability. As we will describe later, the notion of a script offset is introduced to prevent cut-through 
and forces the preservation of these commitments until they are recorded into the blockchain.
 
There are two changes to the protocol data structures that must be made to allow this scheme to work. 

The first is a relatively minor adjustment to the transaction output definition.
The second is the inclusion of script input data and an additional public key in the transaction input field.

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

_Note:_ Currently, the output features are actually malleable. Tari Script fixes this.

Under Tari Script, this definition changes to accommodate the script and the offset public keys:

```rust,ignore
pub struct TransactionOutput {
    /// Options for an output's structure or use
    features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    commitment: Commitment,
    /// A proof that the commitment is in the right range
    proof: RangeProof,
    /// The serialised script
    script: Vec<u8>,
    /// The script offset pubkey, K_O
    script_offset_public_key: PublicKey
    /// UTXO signature with the script offset private key, k_O
    sender_metadata_signature : Signature
}
```

The commitment definition is unchanged:

$$
\begin{aligned}
C_i = v_i \cdot H  + k_i \cdot G
 \end{aligned}
 \tag{1}
$$

The sender signature signs the  metadata of the UTXO with the script offset private key \\( k_{Oi} \\) and this stops 
malleability of the UTXO metadata.

$$
\begin{aligned} 
s_{Mi} = r_{Mi} + k_{Oi} \hash{ \script_i \cat F_i \cat R_{Mi} }
 \end{aligned}
 \tag{2}
$$


Note that:
* The UTXO has a positive value `v` like any normal UTXO. 
* The script and the output features can no longer be changed by the miner or any other party. Once mined, the owner can
  also no longer change the script or output features without invalidating the meta data signature.
* We provide the complete script on the output.

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

In standard Mimblewimble, an input is the same as an output _sans_ range proof. The range proof doesn't need to be 
checked again when spending inputs, so it is dropped. 

The updated input definition is:

```rust,ignore
pub struct TransactionInput {
    /// Options for an output's structure or use
    features: OutputFeatures,
    /// The homomorphic Pedersen commitment representing the output amount
    commitment: Commitment,
    /// The serialised script
    script: Vec<u8>,
    /// The script input data, if any
    input_data: Vec<u8>,
    /// Signature signing the script, input data, public script key and the homomorphic commitment with a combination 
    /// of the homomorphic commitment private values (amount and blinding factor) and the private script key.
    script_signature: CommitmentSignature,
    /// The script offset pubkey, K_O
    script_offset_public_key: PublicKey
}
```

The `script_signature` is an aggregated Schnorr signature signed with a combination of the homomorphic commitment 
private values \\( (v\_i \\, , \\, k\_i )\\) and private script key \\(k\_{Si}\\) to prove ownership of thereof, see 
[Signature on Commitment values] by F. Zhang et. al. and [Commitment Signature] by G. Yu. It signs the script, the 
script input, public script key and the commitment:

$$
\begin{aligned}
 s_{Si} = (a_{Si}, b_{Si}, R_{Si} )
 \end{aligned}
 \tag{3}
$$
Where
$$
\begin{aligned}
R_{Si} &= r_{Si_a} \cdot H + r_{Si_b} \cdot G \\\\
a_{Si}  &= r_{Si_a} +  e(v_{i}) \\\\
b_{Si} &= r_{Si_b} +  e(k_{Si}+k_i) \\\\
e &= \hash{ R_{Si} \cat \alpha_i \cat \input_i \cat K_{Si} \cat C_i} \\\\
\end{aligned}
\tag{4}
$$

This is verified by the following:
 $$
\begin{aligned}
a_{Si} \cdot H + b_{Si} \cdot G = R_{Si} + (C_i+K_{Si})e
 \end{aligned}
 \tag{5}
$$

This signature ensures that only the owner can provide the input data  \\(\input_i\\) to the TransactionInput.


### Script Offset

For every transaction an accompanying script offset \\( \so \\) needs to be provided. This is there to prove that every 
public script key \\( K\_{Sj} \\) and every public script offset key \\( K\_{Oi} \\) supplied with the UTXOs are the 
correct ones. The sender will know and provide script offset private keys \\(k_{Oi} \\) and script private keys 
\\(k_{Si} \\); these are combined to create the script offset \\( \so \\), which is calculated as follows:

$$
\begin{aligned}
\so = \sum_j\mathrm{k_{Sj}} - \sum_i\mathrm{k_{Oi}} \\; \text{for each input}, j,\\, \text{and each output}, i
 \end{aligned}
 \tag{6}
$$

Verification of (6) will entail:
$$
\begin{aligned}
\so \cdot G = \sum_j\mathrm{K_{Sj}} - \sum_i\mathrm{K_{Oi}} \\; \text{for each input}, j,\\, \text{and each output}, i
 \end{aligned}
 \tag{7}
$$

We modify the transactions to be:

```rust,ignore
pub struct Transaction {
    
    ...
    
    /// A scalar offset that links outputs and inputs to prevent cut-through, enforcing the correct application of
    /// the output script.
    pub script_offset: BlindingFactor,
}
```

All script offsets (\\(\so\\)) from (6) contained in a block is summed together to create a total script offset (8) 
so that algorithm (6) still holds for a block.

$$
\begin{aligned}
\so_{total} = \sum_k\mathrm{\so_{k}}\\; \text{for every transaction}, k
 \end{aligned}
 \tag{8}
$$

Verification of (8) will entail:

$$
\begin{aligned}
\so_{total} \cdot G = \sum_j\mathrm{K_{Sj}} - \sum_i\mathrm{K_{Oi}} \\; \text{for each input}, j,\\, \text{and each output}, i
 \end{aligned}
 \tag{9}
$$

As can be seen all information required to verify (8) is contained in a block's inputs and outputs. One important 
distinction to make is that the Coinbase output in a coinbase transaction does not count towards the script offset. 
This is because the Coinbase UTXO already has special rules accompanying it and it has no input, thus we cannot generate a 
script offset \\( \so \\). The coinbase output can allow any script \\(\script_i\\) and  script offset public key 
\\( K\_{Oi} \\) as long as it does not break any of the rules in [RFC 120](RFC-0120_Consensus.md) and the script is 
honored at spend. If the coinbase is used as in input, it is treated exactly the same as any other input.

We modify Blockheaders to be:
```rust,ignore
pub struct BlockHeader {
    
    ...
    
    /// Sum of script offsets for all kernels in this block.
    pub total_script_offset: BlindingFactor,
}
```

This notion of the script offset \\(\so\\) means that the no third party can remove any input or output from a 
transaction or the block, as that will invalidate the script offset balance equation, either (7) or (9) depending on 
whether the scope is a transaction or block. It is important to know that this also stops 
[cut&#8209;through](#cut-through) so that we can verify all spent UTXO scripts. Because the private script key and private 
script offset key is not publicly known, its impossible to create a new script offset.

Certain scripts may allow more than one valid set of input data. Users might be led to believe that this will allow a 
third party to change the script keypair \\((k\_{Si}\\),\\(K\_{Si})\\). If an attacker can change the \\(K\_{Si}\\) 
keys of the input then he can take control of the \\(K\_{Oi}\\) as well, allowing the attacker to change the metadata of 
the UTXO including the script. But as shown in [Script Offset security](#script-offset-security), this is not possible.

If equation (7) or (9) balances then we know that every included input and output in the transaction or block has its 
correct public script key and public script offset key. Signatures (2) & (3) are checked independently from script 
offset verification (7) and (9), and looked at in isolation those could verify correctly but can still be signed by fake 
keys. When doing verification in (7) and (9) you know that the signatures and the message/metadata signed by the private 
keys can be trusted.

### Consensus changes

The Mimblewimble balance for blocks and transactions stays the same.

In addition to the changes given above, there are consensus rule changes for transaction and block validation.

For every valid transaction or block,

1. Check the sender signature \\(s\_{Mi}\\) is valid for every output.
2. The script executes successfully using the given input script data.
3. The result of the script is a valid public key, \\( K\_S \\).
4. The script signature, \\( s\_{Si} \\) is valid for every input.
5. The script offset is valid for every transaction and block.

## Examples

Let's cover a few examples to illustrate the new scheme and provide justification for the additional requirements and
validation steps.

### Standard MW transaction

For this use case we have Alice who sends Bob some Tari.
Bob's wallet is  online and is able to countersign the transaction.

Alice creates a new transaction spending \\( C\_a \\) to a new output \\( C\_b \\) (ignoring fees for now).
Because Alice is spending the transaction she chooses the script \\( \script_b \\), she can either ask Bob for one, or 
choose something akin to a `NOP` script.

To spend \\( C\_a \\), she provides

* An input that contains \\( C\_a \\).
* The script input, \\( \input_a \\).
* A valid script signature, \\( (a_{Sa}, b_{Sa}, R_{Sa}) \\) as per (3),(4) proving that she owns the commitment 
  \\( C\_a \\), knows the private key, \\( k_{Sa} \\), corresponding to \\( K_{Sa} \\), the public key left on the stack 
  after executing \\( \script_a \\) with \\( \input_a \\).
* An offset public key, \\( k_{Ob} \\).
* The script offset, \\( \so\\) with:
$$
\begin{aligned}
\so  = k_{Sa} - k_{Ob}
 \end{aligned}
 \tag{10}
$$
* The sender signature \\( s_{Mb} \\) with: 
$$
\begin{aligned}
  s_{Mb} = r_{mb} + k_{Ob} \hash{ \script_b \cat F_b \cat R_{Mb} }
   \end{aligned}
 \tag{11}
$$

Alice sends her signature nonce, as per the standard Mimblewimble protocol. However, she also 
provides Bob with the offset public key \\( k_{Ob} \\) as wel as a meta_signature \\(s_{Mb}\\) for his output \\(C_b\\).

Bob can then complete his side of the transaction by completing the output:

* Calculating the commitment, \\( C_b = k_b \cdot G + v \cdot H \\),
* Adding in the data from Alice: meta_signature\\(s_{Mb}\\), offset public key \\( k_{Ob} \\),

Bob then signs the kernel excess as usual:
$$
\begin{aligned}
  s_b = r_b + k_b \hash{R_a + R_b \cat f \cat m } 
  \end{aligned}
 \tag{12}
$$

Bob returns the UTXO and partial signature along with his nonce, \\( R_b \\), back to Alice.

Alice then adds in the the script offset \\( \so \\) after which she can construct and broadcast the transaction to the 
network as per standard Mimblewimble.

#### Transaction validation

Base nodes validate the transaction as follows:

* They check that the usual Mimblewimble balance holds by summing inputs and outputs and validating against the excess
  signature. This check does not change nor do the other validation rules, such as confirming that all inputs are in
  the UTXO set etc.
* The sender signature \\(s_{Ma}\\) on Bob's output,
* The input script must execute successfully using the provided input data; and the script result must be a valid 
  public key,
* The script signature on Alice's input is valid by checking:
  $$
\begin{aligned}
    a_{Sa} \cdot H + b_{Sa} \cdot G = R_{Sa} + (C_a + K_{Sa})* \hash{ R_{Sa} \cat \alpha_a \cat \input_a \cat K_{Sa} \cat C_a}
  \end{aligned}
 \tag{13}
  $$
* The script offset is verified by checking that the balance holds:
  $$
\begin{aligned}
    \so \cdot{G} = K_{Sa} - K_{Ob}
  \end{aligned}
 \tag{14}
  $$

Finally, when Bob spends this output, he will use \\( K\_{Sb} \\) as his script input and sign it with his private key
\\( k\_{Sb} \\). He will choose a new \\( K\_{Oc} \\) to give to the recipient, and he will construct the 
script offset, \\( \so_b \\) as follows:

$$
\begin{aligned}
\so_b = k_{Sb} - k_{Oc}
  \end{aligned}
 \tag{15}
$$

### One sided payment

In this example, Alice pays Bob, who is not available to countersign the transaction, so Alice initiates a one-sided 
payment,

$$
C_a \Rightarrow  C_b
$$

Once again, transaction fees are ignored to simplify the illustration.

Alice owns \\( C_a \\) and provides the required script to spend the UTXO as was described in the previous cases.

Alice needs a public key from Bob, \\( K_{Sb} \\) to complete the one-sided transaction. This key can be obtained
out-of-band, and might typically be Bob's wallet public key on the Tari network.

Bob requires the value \\( v_b \\) and blinding factor \\( k_b \\) to claim his payment, but he needs to be able to 
claim it without asking Alice for them.

This information can be obtained by using Diffie-Hellman and Bulletproof rewinding. If the blinding factor \\( k\_b \\) 
was calculated with Diffie-Hellman using the offset keypair, (\\( k\_{Ob} \\),\\( K\_{Ob} \\)) as the sender keypair 
and the script keypair, \\( (k\_{Sb} \\),\\( K\_{Sb}) \\) as the receiver keypair, the blinding factor \\( k\_b \\) 
can be securely calculated without communication.

Alice uses Bob's public key to create a shared secret, \\( k\_b \\) for the output commitment, \\( C\_b \\), using
Diffie-Hellman key exchange.

Alice calculates \\( k_b \\) as
$$
\begin{aligned}
    k_b = k_{Ob} * K_{Sb}
  \end{aligned}
 \tag{16}
$$

Next Alice next uses Bulletproof rewinding, see [RFC 180](RFC-0180_BulletproofRewinding.md), to encrypt the value 
\\( v_b \\) into the the Bulletproof for the commitment \\( C_b \\). For this she uses 
\\( k_{rewind} =  \hash{k_{b}} \\) as the rewind_key and \\( k_{blinding} =  \hash{\hash{k_{b}}} \\) as the blinding 
key.

Alice knows the script-redeeming private key \\( k_{Sa}\\) for the transaction input.

Alice will create the entire transaction including, generating a new offset keypair and calculating the 
script offset,

$$
\begin{aligned}
    \so = k_{Sa} - k_{Ob}
  \end{aligned}
 \tag{17}
$$

She also provides a script that locks the output to Bob's public key, `PushPubkey(K_Sb)`.
This will only be spendable if the sender can provide a valid signature as input that demonstrates proof
of knowledge of \\( k_{Sb}\\) as well as the value and blinding factor of the output \\(C_b\\). Although Alice knowns 
the value and blinding factor of the output \\(C_b\\) only Bob knows \\( k_{Sb}\\).

Any base node can now verify that the transaction is complete, verify the signature on the script, and verify the 
script offset.

For Bob to claim his commitment he will scan the blockchain for a known script because he knowns that the script will 
be `PushPubkey(K_Sb)`. In this case, the script is analogous to an address in Bitcoin or Monero. Bob's wallet can scan 
the blockchain looking for scripts that he would know how to resolve.

When Bob's wallet spots a known script, he requires the blinding factor, \\( k_b \\) and the value \\( v_b \\). First he 
uses Diffie-Hellman to calculate \\( k_b \\). 

Bob calculates \\( k_b \\) as
$$
\begin{aligned}
    k_b = K_{Ob} * k_{Sb}
  \end{aligned}
 \tag{18}
$$

Next Bob's wallet calculates \\( k_{rewind} \\), using \\( k_{rewind} = \hash{k_{b}}\\) and 
(\\( k_{blinding} = \hash{\hash{k_{b}}} \\), using those to rewind the Bulletproof to get the value \\( v_b \\). 

Because Bob's wallet already knowns the script private key \\( k_{Sb} \\), he now knows all the values required to 
spend the commitment \\( C_b \\)

For Bob's part, when he discovers one-sided payments to himself, he should spend them to new outputs using a traditional
transaction to thwart any potential horizon attacks in the future.

To summarise, the information required for one-sided transactions is as follows:

| Transaction input | Symbols                               | Knowledge                                                                                     |
|-------------------|---------------------------------------|-----------------------------------------------------------------------------------------------|
| commitment        | \\( C_a = k_a \cdot G + v \cdot H \\) | Alice knows the blinding factor and value                                                     |
| features          | \\( F_a \\)                           | Public                                                                                        |
| script            | \\( \alpha_a \\)                      | Public                                                                                        |
| script input      | \\( \input_a \\)                      | Public                                                                                        |
| height            | \\( h_a \\)                           | Public                                                                                        |
| script signature  | \\( a_{Sa},b_{Sa}, R_{Sa} \\)         | Alice knows \\( k_{Sa},\\, r_{Sa} \\) and \\( k_{a},\\, v_{a} \\) of the commitment \\(C_a\\) |
| offset public key | \\( K_{Oa} \\)                        | Not used in this transaction                                                                  |

| Transaction output | Symbols                               | Knowledge                                                              |
|---------------------------|---------------------------------------|------------------------------------------------------------|
| commitment                | \\( C_b = k_b \cdot G + v \cdot H \\) | Alice and Bob know the blinding factor and value           |
| features                  | \\( F_b \\)                           | Public                                                     |
| script                    | \\( \script_b \\)                     | Script is public. Only Bob knows the correct script input. |
| range proof               |                                       | Alice and Bob know opening parameters                      |
| offset public key         | \\( K_{Ob} \\)                        | Alice knows \\( k_{Ob} \\)                                 |
| sender metadata signature | \\( s_{Mb}, R_{Mb} \\)                | Alice knows \\( k_{Ob} \\)  and the metadata)              |


### HTLC-like script

In this use case we have a script that controls where it can be spent. The script is out of scope for this example, but
has the following rules:

* Alice can spend the UTXO unilaterally after block _n_, **or**
* Alice and Bob can spend it together.

This would be typically what a lightning-type channel requires.

Alice owns the commitment \\( C_a \\). She and Bob work together to create \\( C_s\\). But we don't yet know who can 
spend the newly created \\( C_s\\) and under what conditions this will be.

$$
C_a \Rightarrow  C_s \Rightarrow  C_x
$$

Alice owns \\( C_a\\), so she knows the blinding factor \\( k_a\\) and the correct input for the script's spending 
conditions. Alice also generates the offset keypair, \\( (k_{Os}, K_{Os} )\\).

Now Alice and Bob proceed with the standard transaction flow.

Alice ensures that the script offset public key \\( K_{Os}\\) is part of the output metadata that contains commitment 
\\( C_s\\). Alice will fill in the script with her \\( k_{Sa}\\) to unlock the commitment \\( C_a\\). Because Alice 
owns \\( C_a\\) she needs to construct \\( \so\\) with:

$$
\begin{aligned}
\so = k_{Sa} - k_{Os}
  \end{aligned}
 \tag{19}
$$


The blinding factor, \\( k_s\\) can be generated using a Diffie-Hellman construction. The commitment \\( C_s\\) needs to 
be constructed with the script that Bob agrees on. Until it is mined, Alice could modify the script via double-spend and 
thus Bob must wait until the transaction is confirmed before accepting the conditions of the smart contract between 
Alice and himself.

Once the UTXO is mined, both Alice and Bob possess all the knowledge required to spend the \\( C_s \\) UTXO. It's only
the conditions of the script that will discriminate between the two.

The spending case of either Alice or Bob claiming the commitment \\( C_s\\) follows the same flow described in the 
previous examples, with the sender proving knowledge of \\( k_{Ss}\\) and "unlocking" the spending script.

The case of Alice and Bob spending \\( C_s \\) together to a new multiparty commitment requires some elaboration.

Assume that Alice and Bob want to spend  \\( C_s \\) co-operatively. This involves the script being executed in such a 
way that the resulting public key on the stack is the sum of Alice and Bob's individual script keys, \\( k_{SsA} \\) and 
\\( k_{SaB} \\).

The script input needs to be signed by this aggregate key, and so Alice and Bob must each supply a partial signature 
following the usual Schnorr aggregate mechanics, but one person needs to add in the signature of the blinding factor and 
value.

In an analogous fashion, Alice and Bob also generate an aggregate script offset private key \\( k_{Ox}\\), each using
their own \\( k_{OxA} \\) and \\( k_{OxB}\\).

To be specific, Alice calculates her portion from

$$
\begin{aligned}
\so_A = k_{SsA} - k_{OxA}
  \end{aligned}
 \tag{20}
$$

Bob will construct his part of the \\( \so\\) with:
$$
\begin{aligned}
\so_B = k_{SsB} - k_{OxB}
  \end{aligned}
 \tag{21}
$$

And the aggregate \\( \so\\) is then:

$$
\begin{aligned}
\so = \so_A + \so_B
  \end{aligned}
 \tag{22}
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

| Transaction input           | Symbols                               | Knowledge                                                                                     |
|-----------------------------|---------------------------------------|-----------------------------------------------------------------------------------------------|
| commitment                  | \\( C_a = k_a \cdot G + v \cdot H \\) | Alice knows the blinding factor and value                                                     |
| features                    | \\( F_a \\)                           | Public                                                                                        |
| script                      | \\( \alpha_a \\)                      | Public                                                                                        |
| script input                | \\( \input_a \\)                      | Public                                                                                        |
| height                      | \\( h_a \\)                           | Public                                                                                        |
| script signature            | \\( (a_{Sa},b_{Sa}, R_{Sa}) \\)     | Alice knows \\( k_{Sa},\\, r_{Sa} \\) and \\( k_{a},\\, v_{a} \\) of the commitment \\(C_a\\) |
| offset&nbsp;public&nbsp;key | \\( K_{Oa} \\)                        | Not used in this transaction                                                                  |

<br>

| Transaction output          | Symbols                                                                | Knowledge                                                                                  |
|-----------------------------|------------------------------------------------------------------------|--------------------------------------------------------------------------------------------|
| commitment                  | \\( C_s = k_s \cdot G + v \cdot H \\)                                  | Alice and Bob know the blinding factor and value                                           |
| features                    | \\( F_s \\)                                                            | Public                                                                                     |
| script                      | \\( \script_s \\)                                                      | Script is public. Alice and Bob only knows their part of the  correct script input.        |
| range proof                 |                                                                        | Alice and Bob know opening parameters                                                      |
| offset&nbsp;public&nbsp;key | \\( K_{Os} = K_{OsA} + K_{OsB}\\)                                      | Alice knows \\( k_{OsA} \\), Bob knows \\( k_{OsB} \\). Neither party knows \\( k_{Os} \\) |
| sender&nbsp;signature       | \\( s_{Ms} = s_{MsA} + s_{MsB}, \\, \\, R_{Ss} = R_{SsA} + R_{SsB} \\) | Alice knows \\( (s_{MsA}, R_{SsA}) \\), Bob knows \\( (s_{MsB}, R_{SsB}) \\)               |

When spending the multi-party input:

| Transaction input           | Symbols                                 | Knowledge                                                                                                                                                          |
|-----------------------------|-----------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| commitment                  | \\( C_s = k_s \cdot G + v_s \cdot H \\) | Alice and Bob know the blinding factor and value                                                                                                                   |
| features                    | \\( F_s \\)                             | Public                                                                                                                                                             |
| script                      | \\( \alpha_s \\)                        | Public                                                                                                                                                             |
| script input                | \\( \input_s \\)                        | Public                                                                                                                                                             |
| height                      | \\( h_a \\)                             | Public                                                                                                                                                             |
| script&nbsp;signature       | \\( (a_{Ss} ,b_{Ss} , R_{Ss}) \\)     | Alice knows \\( (k_{SsA},\\, r_{SsA}) \\), Bob knows \\( (k_{SsB},\\, r_{SsB}) \\). Both parties know \\( (k_{s},\\, v_{s}) \\). Neither party knows \\( k_{Ss}\\) |
| offset&nbsp;public&nbsp;key | \\( K_{Os} \\)                          | As above, Alice and Bob each know part of the offset key                                                                                                           |


### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is 
spent in the same block it was created. This makes it so that the intervening UTXO never existed; along with any checks 
and balances carried in that UTXO. It's also impossible to prove without additional information that cut-through even 
occurred (though one may suspect, since the "one" transaction would contribute two kernels to the block).

In particular, cut-through is devastating for an idea like Tari Script which relies on conditions present in the UTXO 
being enforced.

This is a reason for the presence of the script offset in the Tari Script proposal. It mathematically links all inputs 
and outputs of all the transactions in a block and that tallied up to create the script offset. Providing the script 
offset requires knowledge of keys that miners do not possess; thus they are unable to produce the necessary script 
offset when attempting to perform cut-through on a pair of transactions.

Lets show by example how the script offset stops cut-through. For this example, ignoring fees, we have: 
$$
C_a \Rightarrow  C_b \Rightarrow  C_c
$$
In standard Mimblewimble this [cut-through] can be applied to get:
$$
C_a \Rightarrow  C_c
$$

With the script offset we have the following:
$$
\begin{aligned}
\so_1 = k_{Sa} - k_{Ob}\\\\
\so_2 = k_{Sb} - k_{Oc}\\\\
\end{aligned}
$$
$$
\begin{aligned}
\so_t = \so_1 + \so_2 =  (k_{Sa} + k_{Sb}) - (k_{Ob} + k_{Oc})\\\\
\end{aligned}
$$

If we apply cut-through we need: 

$$
\begin{aligned}
\so'\_t = k\_{Sa} - k\_{Oc}\\\\
\end{aligned}
$$

As we can see:
$$
\begin{aligned}
 \so\_t\ \neq \so'\_t \\\\
\end{aligned}
$$

A User also cannot generate a new script offset as only the original owner can provide the private script key \\(k\_{Sa}\\) 
to create a new script offset.Cut-through is only possible if the original owner participates. In this example cut-through 
can happen only if Alice and Carol negotiate a new transaction. This will ensure that the original owner (Alice) is happy 
with the spending of the transaction to a new party, e.g. she has verified the spending conditions like a script.

### Script Offset security

If all the inputs in a transaction or a block contain scripts such as just `NOP` or `CompareHeight` commands, then the 
hypothesis is that it is possible to recreate a false script offset. Lets show by example why this is not possible. In 
this Example we have Alice who pays Bob with no change output:
$$
C_a \Rightarrow  C_b
$$

Alice has an output \\(C\_{a}\\) which contains a script that only has a `NOP` command in it. This means that the 
script \\( \script\_a \\) will immediately exit on execution leaving the entire input data \\( \input\_a \\)on the 
stack. She sends all the required information to Bob as per the [standard mw transaction](#standard-mw-transaction), who 
creates an output \\(C\_{b}\\). Because of the `NOP` script \\( \script\_a \\), Bob can change the public script key 
\\( K\_{Sa}\\) contained in the input data. Bob can now use his own \\(k'\_{Sa}\\) as the script private key. He 
replaces the script offset public key with his own \\(K'\_{Ob}\\) allowing him to change the script 
\\( \script\_b \\) and generate a new signature as in (2). Bob cab now generate a new script offset with 
\\(\so' = k'\_{Sa} - k'\_{Ob} \\). Up to this point, it all seems valid. No one can detect that Bob changed the script 
to \\( \script\_b \\).

But what Bob also needs to do is generate the signature in (3). For this signature Bob needs to know 
\\(k\_{Sa}, k\_a, v\_a\\). Because Bob created a fake script private key, and there is no change in this transaction, 
he does know the script private key and the value. But Bob does not know the blinding factor \\(k\_a\\) of Alice's 
commitment and thus cannot complete the signature in (3). Only the rightful owner of the commitment, which in 
Mimblewimble terms is the  person who knows \\( k\_a, v\_a\\), can generate the signature in (3).


### Script lock key generation

At face value, it looks like the burden for wallets has tripled, since each UTXO owner has to remember three private 
keys, the spend key, \\( k_i \\), the offset key \\( k_{O} \\) and the script key \\( k_{S} \\). In practice, the script 
key will often be a static key associated with the user's node or wallet. Even if it is not, the script and offset keys
can be deterministically derived from the spend key. For example, \\( k_{S} \\) could be 
\\( \hash{ k_i \cat \alpha} \\).

### Blockchain bloat

The most obvious drawback to Tari Script is the effect it will have on blockchain size. UTXOs are substantially larger,
with the addition of the script, script signature, and a public key to every output.

These can eventually be pruned, but will increase storage and bandwidth requirements.

Input size of a block will now be much bigger as each input was previously just a commitment and output features.
Each input now includes a script, input_data, the script signature and an extra public key. This could be compacted by
just broadcasting input hashes along with the missing script input data and signature, instead of the full input in
transaction messages, but this will still be larger than inputs are currently.

Every header will also be bigger as it includes an extra blinding factor that will not be pruned away.

### Fodder for chain analysis

Another potential drawback of Tari Script is the additional information that is handed to entities wishing to perform 
chain analysis. Having scripts attached to outputs will often clearly mark the purpose of that UTXO. Users may wish to 
re-spend outputs into vanilla, default UTXOs in a mixing transaction to disassociate Tari funds from a particular 
script.

## Notation


Where possible, the "usual" notation is used to denote terms commonly found in cryptocurrency literature. Lower case characters are used as private keys, while uppercase characters are used as public keys. New terms 
introduced by Tari Script are assigned greek lowercase letters in most cases. The capital letter subscripts, _R_ and _S_ 
refer to a UTXO _receiver_ and _script_ respectively.

| Symbol                    | Definition                                                                                                                                                                                                                            |
|---------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| \\( \script_i \\)         | An output script for output _i_, serialised to binary.                                                                                                                                                                                |
| \\( F_i \\)               | Output features for UTXO _i_.                                                                                                                                                                                                         |
| \\( f_t \\)               | Transaction fee for transaction _t_.                                                                                                                                                                                                  |
| \\( m_t \\)               | Metadata for transaction _t_. Currently this includes the lock height.                                                                                                                                                                |
| \\( (k_{Oi}\, K_{Oi}) \\) | The private - public keypair for the UTXO script offset key.                                                                                                                                                                          |
| \\( (k_{Si}\, K_{Si}) \\) | The private - public keypair for the script key. The script, \\( \script_i \\) resolves to \\( K_S \\) after completing execution.                                                                                                    |
| \\( \so_t \\)             | The script offset for transaction _t_, as \\( \so_t = \sum_j{ k_{Sjt}} - \sum_i{k_{Oit}}\\)                                                                                                                                           |
| \\( C_i \\)               | A Pedersen commitment to a value \\( v_i \\), as \\( C_i = k_i \cdot{G} + v_i \cdot H \\)                                                                                                                                             |
| \\( \input_i \\)          | The serialised input for script \\( \script_i \\)                                                                                                                                                                                     |
| \\( s_{Si} \\)            | A script signature for output \\( i \\), as \\( s_{Si} = (a_{Si}, b_{Si}, R_{Si} ) = (r_{Si_a} +  e(v_{i})), (r_{Si_b} + e(k_{Si}+k_i)) \\; \text{where} \\; e = \hash{ R_{Si} \cat \script_i \cat \input_i \cat K_{Si} \cat C_i} \\) |
| \\( s_{Mi} \\)            | A sender signature for output \\( i \\), as \\( s_{Mi} = r_{Mi} + k_{Oi}\hash{ \script_i \cat F_i \cat R_{Mi}  } \\)                                                                                                                  |

## Extensions

### Covenants

Tari Script places restrictions on _who_ can spend UTXOs. It will also be useful for Tari digital asset applications to
restrict _how_ or _where_ UTXOs may be spent in some cases. The general term for these sorts of restrictions are termed
_covenants_. The [Handshake white paper] has a fairly good description of how covenants work.

It is beyond the scope of this RFC, but it's anticipated that Tari Script would play a key role in the introduction of
generalised covenant support into Tari.

### Lock-time malleability

The current Tari protocol has an issue with Transaction Output Maturity malleability. This output feature is enforced in
the consensus rules, but it is actually possible for a miner to change the value without invalidating the transaction.

With Tari Script, output features are properly committed to in the transaction and verified as part of the script offset
validation.

### Credits

- [@CjS77](https://github.com/CjS77)
- [@hansieodendaal](https://github.com/hansieodendaal)
- [@philipr-za](https://github.com/philipr-za) 
- [@SWvheerden](https://github.com/SWvheerden)

Thanks to David Burkett for proposing a method to prevent cut-through and willingness to discuss ideas.

[data commitments]: https://phyro.github.io/grinvestigation/data_commitments.html
[LIP-004]: https://github.com/DavidBurkett/lips/blob/master/lip-0004.mediawiki
[Scriptless script]: https://tlu.tarilabs.com/cryptography/scriptless-scripts/introduction-to-scriptless-scripts.html
[Handshake white paper]: https://handshake.org/files/handshake.txt
[Signature on Commitment values]: https://documents.uow.edu.au/~wsusilo/ZCMS_IJNS08.pdf
[Commitment Signature]: https://eprint.iacr.org/2020/061.pdf
[cut-through]: https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#cut-through
