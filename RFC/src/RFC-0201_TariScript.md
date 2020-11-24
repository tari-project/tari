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
* An additional secret key is provided by UTXO owners that signs both the script and the input data (if any) when it is spent.

Note that, at the outset, the signatures must be present in the _kernel_ in some form, otherwise miners will be able to 
remove the script via cut-through, whereas kernels never get pruned.

In addition, the script must be attached to the UTXO. As will be demonstrated later, if this is not the case, the UTXO 
owner can ignore the script conditions completely.

### UTXO data commitments

One approach to commit to the script hashes is to modify the output commitments using a variation of the [data commitments] approach
first suggested by [Phyro](https://github.com/phyro).
 
There are several changes to the protocol data structures that must be made to allow this scheme to work. 

The first is a relatively minor adjustment to the transaction output definition.
The second is the inclusion of script input data and the retention of the range proof in the transaction input field.
The third is an addition of several pieces of data to the transaction kernel.

Finally, the balance and signature consensus rules must be updated to account for these changes.

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
    script: Vec<u8>,
    /// The public script key, L
    script_pub_key: PublicKey
}
```

We now introduce some [Notation](#notation).

The script public key, \\( L_i \\) is the public counterpart of a key chosen by the creator of the UTXO (the script 
key, \\( k_{i'} \\) ).

We define \\( \sigma_i \\) to be the hash of the serialised script and output features,

$$
  \sigma_i = \mathrm{H}(s_i \Vert \mathrm{features_i})
$$


We refer to the term \\( \sigma_i k_{i'} \\) as the _script product_ and \\( \sigma_i L_i \\) as the _public script 
product_.  

Wallets now generate the range proof with

$$ k_i  + k_{a'}\sigma_i $$

rather than just \\( k_i \\).

Note that:
* The UTXO has a positive value `v` like any normal UTXO. 
* The script nor the output features can be changed by the miner or any other party. Once mined, the owner can no longer
  change the script or output features without invalidating the range proof.
* Currently, the output features are actually malleable. TariScript fixes this by committing to the features in \\( \sigma \\).

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

In standard Mimblewimble, an input is the same as an output _sans_ range proof. Under this scheme, the UTXO spender must
supply the input data for the unlocking script, if any, and a signature signing the script and input data.

```rust,ignore
pub struct TransactionInput {
    /// Options for an output's structure or use
    features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    commitment: Commitment,
    /// A proof that the commitment is in the right range
    script: Vec<u8>,
    /// The public script key, L
    script_pub_key: PublicKey
    /// The script input data, if any
    input_data: Vec<u8>
    /// A signature with k', signing the script and input data
    script_signature: Signature
}
```

When nodes validate transactions and blocks, they have the additional burden of _revalidating_ the range proof and 
confirming the script signature is valid.


### Kernel changes

The key components of the current kernel definition are:

* The transaction metadata, including the fee, lock height, and other kernel features,
* The public excess for the transaction,
* A signature from all parties in the transaction signing the excess and committing to the metadata.

We now modify the signature process slightly. Instead of each party signing the kernel with their spend keys, they sign
with the sum of their spend keys and script products. That is,

$$
\sum \pm (k_i + \sigma_i k_{i'})
$$

using a plus or minus for inputs and outputs as appropriate and analogously with the current Mimblewimble signature scheme.

### Consensus changes
Now when calculating the transaction or block balance, the equation must be adapted to account for the changes in the 
kernel signature.

For every commitment,

$$ C_i = v_i.H + k_i.G $$

we add the public script product which can be calculated from the data in the input and output data structures.

$$
\begin{align}
\sigma_i &= \mathrm{H}(s_i \Vert \mathrm{features_i} ) \\\\
L_i &= k_{i'}. G \\\\
\hat{C_i} &= C_i + L_i\sigma_i \\\\
&= k_i.G + k_{i'}\sigma_i . G + v_i . H \\\\
&= P_i + L_i\sigma_i + v_i . H
\end{align}
$$

The overall and block balance checks must also be modified to use \\( \hat{C} \\) rather than \\( C \\).

This is fairly straightforward.

Given a set of inputs, outputs, the transaction fee, \\( f \\), and the transaction offset, \\( \delta \\):

$$
\sum\mathrm{Modified Inputs} - \sum\mathrm{ Modified Outputs} - \mathrm{fee}.H \stackrel?= X_s + \mathrm{offset}.G
$$

The public excess includes one term that has been modified by the offset such that

$$
  X_s = \sum \pm (P_i + L_i\sigma_i) - \delta.G
$$

Continuing with the balance, 

$$
\begin{align}
\sum\mathrm{Modified Inputs} - \sum\mathrm{ Modified Outputs} - \mathrm{fee}.H &\stackrel?= X_s + \mathrm{offset}.G \\\\
\Rightarrow \sum \pm\hat{C_i} - f.H  &\stackrel?= \sum \pm (P_i + L_i\sigma_i) - \delta.G + \delta.G  \\\\
\Rightarrow \sum \bigl( \pm C_i \pm L_i\sigma_i \bigr) - f.H &\stackrel?= \sum \pm (P_i + L_i\sigma_i)
\end{align}
$$

Here the \\( \pm \\) operator is used to simplify the notation and indicates the addition operation when the following 
term is associated with an input and subtraction when the term related to an output.

If the accounting is correct, all values will cancel out, leaving:

$$
\begin{align}
\sum(\pm k_i.G \pm L_i\sigma_i) &\stackrel?= \sum \pm (P_i + L_i\sigma_i)  \\\\
\sum \pm (P_i + L_i\sigma_i) &\stackrel?= \sum \pm (P_i + L_i\sigma_i)  \\\\
\end{align}
$$

and the balance holds if the transaction is valid.

If the balance does not hold for whatever reason, the transaction is rejected, as per the _status quo_. 

A similar exercise for the block validation will illustrate that the overall balance holds too.

### Additional consensus rules

In addition to the changes given above, there are consensus rule changes for transaction and block validation.

For every valid block or transaction,

1. Validate the range proofs for _both_ inputs and outputs. Validate range proofs against \\( \hat{C_i} \\) rather 
    than \\( C_i \\).
2. Check that the script signature is valid for every input.
3. The Coinbase output MUST use a default script (`PUSH_ZERO`).

## Checking the requirements

Let us evaluate this scheme against the [Additional requirements](#additional-requirements) listed earlier.

### Malleability

Let's say a malicious party, Bob, can modify a transaction before it enters the blockchain. Any changes to a Pedersen
commitment would break the kernel signature, which cannot be updated without counterparty cooperation as is currently
the case in Mimblewimble.
 
If Bob is the only party in the transaction, he could modify aspects of the transaction successfully. However, this is
essentially a double spend. If any parties are relying on this transaction for any reason, they must wait for it to
be mined (with an appropriate number of confirmations) before considering it complete.

Once the transaction is mined, changes to any of the parts of the input or output structure will be detected by the 
overall balance, the range proof validation, or the script signature.

### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is spent
in the same block it was created.

TODO

### Script lock key generation

At face value, it looks like the burden for wallets has doubled, since each UTXO owner has to remember two private keys,
the spend key, \\( k_i \\) and the script key \\( k_{i'} \\). In practice, the script lock key can be
deterministically derived from the spend key. For example, the script key can be equal to the hash of the spend key.

## Disadvantages

### Blockchain bloat

The most obvious drawback to TariScript is the effect it will have on blockchain size. The addition of the script and 
script signature, adds at least 33 bytes of data to every UTXO. This can eventually be pruned, but will increase
storage and bandwidth requirements.

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
| \\( k_{a'} \\)    | The script private key                                                     |
| \\( L_a \\)       | The script public key, i.e. \\( k_{a'}.G \\)                               |
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

