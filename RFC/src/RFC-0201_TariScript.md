# RFC-0201/TariScript

## Tari Script

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

Some smart contract features are possible in vanilla Mimblewimble using [Scriptless scripts], such as

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

As of this writing, the Beam project also considers Scriptless Script to be the
[extent of their scripting capabilities](https://docs.beam.mw/Beam_lightning_network_position_paper.pdf).

TODO - update EBam

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
then there's no way to ensure that the script was honoured.

This is broadly in keeping with the Mimblewimble security guarantees that individual transactions are not necessarily 
verified during chain synchronisation.

However, the guarantee that no additional coins are created or destroyed remains intact.

Put another way, the blockchain relies on the network _at the time_ to enforce the Tari script spending rules. 
This means that the scheme may be susceptible to certain _horizon attacks_.

Incidentally, a single honest archival node would be able to detect any fraud on the same chain and provide a simple proof
that a transaction did not honour the redeem script.

### Additional requirements

The assumptions that broadly equate scripting with range proofs in the above argument are:

* The script (hash) must be committed to the blockchain.
* The script must not be malleable in any way without invalidating the transaction.
* We must be able to prove that the UTXO owner provides the script hash and no-one else.
* The scripts and their redeeming inputs must be stored on the block chain. TODO - inputs must be committed

The next section discusses the specific proposals for achieving these requirements.

## Protocol modifications

At a high level, Tari script works as follows:

* The script commitment, which can be adequately represented by the hash of the canonical serialisation of the script in
binary, is recorded in the transaction kernel. 
* An additional merkle mountain range committing the script history (including the redeeming inputs) is necessary to prevent miners rewriting the script 
hash history during chain re-orgs.
* An additional secret key is provided by UTXO owners that signs both the script and the input data (if any) when it is spent.

Note that, at the outset, the signatures must be present in the _kernel_ in some form, otherwise miners will be able to 
remove the script via cut-through, whereas kernels never get pruned.

### UTXO data commitments

One approach to commit to the script hashes is to modify the output commitments using a variation of the [data commitments] approach
first suggested by [Phyro](https://github.com/phyro). In this approach, when creating a new UTXO, the owner also calculates
the hash of the locking script, \\( sigma \\), such that \\(\sigma = H(script) \\). The script hash gets stored in the UTXO itself.

Using this approach, the script hash is bound to the UTXO via the UTXO commitment. In combination with the signature, 
this makes it non-malleable insofar as the UTXO itself is non-malleable.

There are several changes to the protocol data structures that must be made to allow this scheme to work. The first is
a relatively minor adjustment to the output commitment.

The second is an addition of several pieces of data to the transaction kernel.

The third is the addition of a new section in the transaction and block definition to hold the script data.

Finally, the balance and signature consensus rules must be updated to account for these changes.

### Output commitment changes

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

Under Tari script, this definition does not change. The information making up the commitment does change slightly, but
from the perspective of the block and transaction storage, an output is identical pre- and post- Tari script.

_Note:_ Under the current consensus rules, the output features are actually malleable. Under Tari script, we can fix 
this by moving all output features to script constraints, with the exception of the coinbase output feature.

Now when calculating the transaction or block balance, we calculate a different set of commitments. The current commitment,

$$ C_a = v_a.H + k_a.G $$

is modified with a commitment to the script hash, as follows (see [Notation](#notation)):

$$
\begin{align}
\sigma_a &= \mathrm{H}(s_a) \\\\
L_a &= k_{a'}. G \\\\
\hat{C_a} &= C_a + L_a\sigma_a \\\\
&= k_a.G + k_{a'}\sigma_a . G + v_a . H \\\\
&= P_a + L_a\sigma_a + v_a . H
\end{align}
$$

Transaction participants will sign the kernel and build the range proof with
$$ k_a  + k_{a'}\sigma_a $$
rather than just \\( k_a \\).

The overall and block balance checks must also be modified to use \\( \hat{C} \\) rather than \\( C \\).

### Kernel changes

The key components of the current kernel definition are:

* The transaction metadata, including the fee, lock height, and other kernel features,
* The public excess for the transaction,
* A signature from all parties in the transaction signing the excess and committing to the metadata.

For Tari script, we require the addition of an additional field:

* Input script hashes. An array of tuples of \\( (L_i, \sigma_i) \\), the script key and script hash, for every input in the transaction,
* Output script hashes. An array of tuples of \\( (L_i, \sigma_i) \\), the script key and script hash, for every output in the transaction.

The script key, \\( L_i \\) is the public counterpart of a key chosen by the creator of the UTXO.
The secret portion of the script key, \\( k_{i'} \\) effectively locks the script hash and prevents third parties from modifying the script portion of the output commitment without detection.
This was a vulnerability in the original [data commitments] scheme which is resolved with this modification.

The product \\( \sigma_i L_i \\) is referred to as the _script product_.

### Script section

A new section in the Mimblewimble aggregate body is an array of script objects. The length of the array is equal to the
length of the input script hashes in the kernel(s).

A script object consists of:
* `script`: The serialised binary representation of the script. The hash of this data MUST equal one of the script
  hashes supplied in the kernel input script hash array,
* `data`: The serialised representation of the data to push onto the stack prior to executing of the script,
* `signature`: A signature of the script + data, signed by \\( k_{a'} \\).

It's possible to have duplicate, non-trivial script hashes in the kernel. Since it is possible to have different inputs
to the script and still be a valid spend condition, these scripts MUST be repeated in the script section, even if they
have the same input, or are the default script (`PUSH(0)`).

_Open question_: Is an MMR of the script section necessary to be included in the header?

### Transaction balance

The Mimblewimble transaction balance must be modified to take the script products into account.
This is fairly straightforward.

Given a set of inputs, outputs, the transaction fee, \\( f \\), and the transaction offset, \\( \delta \\):

$$
\begin{align}
\sum\mathrm{Inputs} - \sum\mathrm{Outputs} - \mathrm{fee}.H &\stackrel?= \mathrm{Excess} \pm \sum\mathrm{script~products} + \mathrm{offset}.G  \\\\
\Rightarrow \sum \pm\hat{C_i} - f.H  &\stackrel?= X_s + \sum \pm L_i\sigma_i + \delta.G  \\\\
\Rightarrow \sum \bigl( \pm C_i \pm L_i\sigma_i \bigr) - f.H &\stackrel?= X_s + \sum \pm L_i\sigma_i + \delta.G
\end{align}
$$

Here the \\( \pm \\) operator is used to simplify the notation and indicates the addition operation when the following 
term is associated with an input and subtraction when the term related to an output.

If the accounting is correct, all values will cancel out:

$$
\begin{align}
\sum(\pm k_i.G \pm L_i\sigma_i) &\stackrel?= X_s + \sum \pm L_i\sigma_i + \delta.G \\\\
\sum \pm P_i + \sum \pm L_i\sigma_i &\stackrel?= X_s + \sum \pm L_i\sigma_i + \delta.G \\\\
\sum \pm P_i +  &\stackrel?= X_s +  \delta.G \\\\
\end{align}
$$

Given the definition of the excess, \\(  \sum \pm P_i = X_s + \delta.G \\), the balance holds.

If the balance does not hold for whatever reason, the transaction is rejected, as per the _status quo_. 

### Kernel signature

The kernel signature must also be adapted to account for the fact that the residual of the block balance is no longer 
simply the public excess, but also includes the script hashes and script keys.

$$
\begin{align}
s_i &= r_i + e.\pm \bigl(k_i + k_{i'}\sigma_i \bigr) \\\\
s_i.G &= r_i.G + e.\pm \bigl(k_i + k_{i'}\sigma_i \bigr).G \\\\
&= R_i + e.\pm \bigl(P_i + L_i\sigma_i \bigr) \\\\
s &= \sum s_i \\\\
s.G  &= \sum R_i + e.\bigl( \sum \pm P_i \pm \sum L_i\sigma_i \bigr) \\\\
&= \sum R_i + e.\bigl( X_s + \sum \pm L_i\sigma_i \bigr) \\\\
\end{align}
$$

The exercise above demonstrates that $$x_s + \sum \pm \sigma_i k_{i'}$$ signs the kernel correctly.

In other words, a validating a transaction or block now checks that 

$$ X_s + \sum \pm \sigma_i L_i $$ 

corresponds to the kernel signature, rather than just \\( X_s \\).

The same treatment extends to be block validation check. Note that since the individual kernel excesses can still be
summed to obtain the overall block balance.
Furthermore, at the block aggregation level, it is non-trivial to match a script hash with a UTXO, and so the
dis-association of kernels and their outputs is maintained.

### Additional consensus rules

In addition to the changes given above, there are consensus rule changes for transaction and block validation.

For every valid block or transaction,

1. Every script hash in the kernel input seet MUST match a script in the script section. If there are
   duplicate non-trivial script hashes present, there MUST be a script entry for every duplicate.
2. The script and its input data, if any, MUST execute successfully, i.e. without any errors. After execution, the stack
   MUST have exactly one element and its value MUST be exactly zero.
3. The Coinbase output MUST use a default script hash.

## Checking the requirements

Let us evaluate this scheme against the [Additional requirements](#additional-requirements) listed earlier.

### Malleability

Let's say a malicious party, Bob, can modify a transaction before it enters the blockchain.

#### Changing script hash

First, Bob simply tries to modify the kernel, replacing one of the kernel script hashes from \\( (L_a, \sigma_a) \\) to
\\( (L_b, \sigma_b) \\). Since \\( k_{a'}\sigma_a \\) is part of the UTXO commitment, the overall balance would fail.

#### Changing commitment

Bob then tries to apply the attack described in [data commitments], and tries to modify the UTXO commitment as follows:

$$
\begin{align}
\hat{C} &= k_a.G + k_{a'}\sigma_a.G + v.H  \\\\
\Rightarrow \hat{C_b} &= k_a.G + k_{a'}\sigma_a.G + v.H - k_{a'}\sigma_a.G + k_{b'}\sigma_b.G \\\\
&= k_a.G + v.H + k_{b'}\sigma_b.G \\\\
&= C_a + L_b\sigma_b
\end{align}
$$

So far, so bad. But Bob is still required to produce a signature. Bob needs to replace the signature with one that signs
with his script:

$$
s \Rightarrow s^* = s - k_{a'}\sigma_a.e + k_{b'}\sigma_b.e
$$

To do this, Bob must know the value of \\( k_{a'} \\) which requires him to find the discrete log of \\( L_a \\), an
infeasable exercise.

### Script product malleability

Since _the sum of script products_ is a term in the balance, in theory, a malicious party could manipulate the individual 
script products, keeping the sum constant. Then the balance would still hold.

Changing any of the script products entails:

* Changing two or more script hashes, 
* and/or changing the script key.

Changing any of the script hashes obliges the attacker to provide a valid script that is the preimage of the new hash.
Assuming this is possible in polynomial time, the attacker must _also_ sign the new hash, proving knowledge of the
script key.

As an illustration, assume Bob wishes to replace a script on an output with one of his own, \\( \sigma_\* \\). 
He tries to do this as follows:

$$
\begin{align}
(L_1, \sigma_1) &\rightarrow (L_1, \sigma_\*) \\\\
(L_2, \sigma_2) &\rightarrow (L_1, \sigma_2 - \sigma_\* + \sigma_1)
\end{align}
$$

Here the overall balance still holds. Unfortunately for Bob, there's no corresponding entry in the script section for the
second script hash that he modified, nor will there be any valid witness signature for the script.

In general, any modification to the Public script key, or script hash will invalidate the overall balance, the script
section, or both.

#### Changing script input data

The presence of the witness signature in the script section prevents anyone from changing the script input data unless
you know the script key.

### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is spent
in the same block it was created.

In this proposal, the script hashes are committed in the kernels as well as the UTXOs and the scripts themselves are
provided in the script section of the body. Therefore, even if the UTXO related to the script has been cut through, the
consensus rules would _still_ require the script to be validated.

Recall that by the time the block is assembled, the link between output and script has been severed. Nodes only have a
list of scripts and their hashes and must validate that they all produce a zero result.

By extension, UTXOs can still be pruned because the \\( L_a\sigma_a \\) values change sign when used as inputs and will
cancel out in the overall balance in the same way that the pruned excesses are.

### Script lock key generation

At face value, it looks like the burden for wallets has doubled, since each UTXO owner has to remember two private keys,
the spend key, \\( k_a \\) and the script key \\( k_{a'} \\). In practice, the script lock key can be
deterministically derived from the spend key. For example, the script key can be equal to the hash of the spend key.

## Disadvantages

### Blockchain bloat

The most obvious drawback to TariScript is the effect it will have on blockchain size. The kernels are never pruned, and
introduce an additional 32 bytes per input and output to the permanent blockchain size.

Assuming a moderately busy network processing 1,000 transactions per day, with an average of 1 input and 2 outputs per transaction,
this would produce growth of around 400 MB per year. The addition of TariScript, using these same assumptions, adds another 67 MB
to the chain, or an increase of about 17%.

The scripts and the input data will further bloat the chain until they are pruned, but do not add to the size of the
blockchain in the long term.

There is the possibility of reducing the chain size increase. If the input script hashes in the kernel _references_
the kernel that created the output with an index to the relevant script hash, rather than duplicating the \\( L_a, \sigma_a \\)
pair, we could almost halve the kernel size increase, and achieve an annual chain size increase of about 9% using the same
assumptions as before.

However, this approach adds a significant degree of linkage to the chain data and so it is debatable whether the space savings
are worthwhile.

### Fodder for chain analysis

Another potential drawback of TariScript is the additional information that is handed to entities wishing to perform chain
analysis. Even though the script hashes are decoupled from the outputs themselves, one may be able to draw inferences
from the distribution of the script hashes committed into the chain. These privacy concerns are relatively minor, but they
certainly add a level of heterogeneity to the blockchain data, which can only serve to aid chainalysis efforts.

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

## Tari Script semantics

The proposal for Tari Script is straightforward. It is based on Bitcoin script and inherits most of its ideas.

The main properties of Tari script are

* The scripting language is stack-based. At redeem time, the UTXO spender must supply an input stack. The script runs by
  operating on the stack contents.
* If an error occurs during execution, the script fails.
* After the script completes, it is successful if and only if it has not aborted, and there is exactly a single element
  on the stack with a value of zero. In other words, the script fails if the stack is empty, or contains more than one
  element, or aborts early.
* It is not Turing complete, so there are no loops or timing functions.
* The opcodes enforce type safety. e.g. A public key cannot be added to an integer scalar. Errors of this kind MUST cause
  the script to fail. The Rust implementation of Tari Script automatically applies the type safety rules.

### Failure modes

A script can fail for a variety of reasons. Most failures are not context-dependent, i.e. they will fail irrespective of
the state of the blockchain.

Some scripts' execution result, such as those that make comparisons to the block height, are dependent on the execution
context. If a script fails, it can fail in a `Failure` mode, indicating that the script will always fail, or it can fail
in `ContextFailure` mode, meaning it is possible that the script could pass in a different context.

The context is invariant in block validations, and so both failure modes are treated identically: The script, and block are
both invalid and MUST be rejected.

But in transaction validations, the mempool SHOULD move a transaction to the Pending pool if a transaction fails
with `ContextFailure`. It MUST reject the transaction if it fails with `Failure`.

Not all context-dependent failures will fail with `ContextFailure`. For example, `CheckHeight(height) ZeroVerify` will
fail with `Failure` even though this script might pass in future. This is because the `CheckHeight(height)` does not have
a context failure mode. The TariScript VM is not smart enough to know that the `ZeroVerify` opcode is context-dependent
by virtue of the preceding instruction. If you wish to write context-dependent scripts, then use opcode that can
fail immediately with the `ContextFailure` state.

### Constraints

* The maximum length of a script when serialised is 1,024 bytes.
* The maximum length of a script's input is 1,024 bytes.
* The maximum stack height is 255.

## Opcodes

Tari Script opcodes range from 0 to 255 and are represented as a single unsigned byte. The opcode set is
limited to allow for the applications specified in this RFC, but can be expanded in the future.

### Block height checks

All these opcodes test the current block height (or, if running the script as part of a transaction
validation, the next earliest block height) against a given value.

##### CheckHeightVerify(height)

Compare this block's height to `height`. This is a no-op opcode.

Fails with `ContextFailure` if:
* the height < `height`.

##### CheckHeight(height)

Compare this block's height to `height`. Push the value of (`height` - the current height) to the stack
if the height <= `height`, otherwise push zero to the stack. In other words, the top of the stack will hold the height
difference between `height` and the current height (with a minimum of zero).

Fails with `Failure` if:
* the stack would exceed the max stack height.

##### CompareHeightVerify

Pop the top of the stack as `height`. The result of this opcode is a no-op.

Fails with `ContextFailure` if:
* the height < `height`.

Fails with `Failure` if:
* there is not a valid integer value on top of the stack,
* the stack is empty.

##### CompareHeight

Pop the top of the stack as `height`. Push the value of (`height` - the current height) to the stack if the height
<= `height`, otherwise push zero to the stack. In other words, this opcode replaces the top of the stack with the
difference between that value and the current height.

Fails with `Failure` if:
* there is not a valid integer value on top of the stack,
* the stack is empty.

### Stack manipulation

##### PushZero

Pushes a zero onto the stack. This is a very common opcode and has the same effect as `PushInt(0)` but is more compact.

The default script is a single `PushZero`.

##### PushOne

Pushes a one onto the stack. This is a very common opcode and has the same effect as `PushInt(1)` but is more compact.

`PushOne` can be used in conditionals and to represent `false` or a failure condition. For example,

    PushOne

is a burn script, meaning that no-one can spend the output, since the script will always fail, similar to `Return`

##### PushHash(HashValue)

Push the associated 32-byte value onto the stack.

Fails with `Failure` if:
* HashValue is not a valid 32 byte sequence
* the stack would exceed the max stack height.

##### PushInt(i64)

Push the associated 64 bit signed integer onto the stack

Fails with `Failure` if:
* HashValue is not a valid 8 byte sequence
* the stack would exceed the max stack height.

##### PushPubKey(PublicKey)

Push the associated 32-byte value onto the stack. It will be interpreted as a public key or a commitment.

Fails with `Failure` if:
* PublicKey is not a valid 32 byte sequence
* the stack would exceed the max stack height.

##### Drop

Drops the top stack item.

Fails with `Failure` if:
* The stack is empty

##### Dup

Duplicates the top stack item.

Fails with `Failure` if:
* The stack is empty
* the stack would exceed the max stack height.

##### RevRot,

Reverse rotation. The top stack item moves into 3rd place, e.g. `abc => bca`.

Fails with `Failure` if:
* The stack has two or fewer elements

### Math operations

##### Add

Pop two items and push their sum

Fails with `Failure` if:
* The stack has a height of one or less

##### Sub

Pop two items and push the second minus the top

Fails with `Failure` if:
* The stack has a height of one or less

##### Equal

Pops the top two items, and pushes 0 to the stack if the inputs are exactly equal, 1 otherwise.

Fails with `Failure` if:
* The stack height is one or less

##### EqualVerify

Pops the top two items, and compare their values.

Fails with `Failure` if:
* The stack height is one or less,
* The top two stack elements are not equal

### Boolean logic

#### Or(n)

`n` items are popped from the stack. The top item must match at least one of those items.

Fails with `Failure` if:
* The stack has height `n` or less
* The top value does not match any of the popped items

### Cryptographic operations

##### HashBlake256

Pop the top element, hash it with the Blake256 hash function and push the result to the stack.

Fails with `Failure` if:
* The stack is empty

##### CheckSig,

Pop the public key and then the signature. If the signature signs the script, push 0 to the stack, otherwise
push 1.

Fails with `Failure` if:
* The stack is of height one or less
* The top stack element is not a PublicKey or Commitment
* The second stack element is not a Signature

##### CheckSigVerify,

Pop the public key and then the signature. A successful execution does not manipulate the stack an further.

Fails with `Failure` if:
* The stack is of height one or less
* The top stack element is not a PublicKey or Commitment
* The second stack element is not a Signature
* The signature does not sign the script

### Miscellaneous

##### Return

Always fails with `Failure`.

##### If-then-else

The if-then-else clause is marked with the `IFTHEN` opcode.
When the `IFTHEN` opcode is reached, the top element of the stack is popped into `pred`.
If `pred` is zero, the instructions between `IFTHEN` and `ELSE` are executed. The instructions from `ELSE` to `ENDIF` are
then popped without being executed.

If `pred` is not zero, instructions are popped until `ELSE` or `ENDIF` is encountered.
If `ELSE` is encountered, instructions are executed until `ENDIF` is reached.
`ENDIF` is a marker opcode and a no-op.

Fails with `Failure` if:
* The instruction stack is empty before encountering `ENDIF`

If any instruction during execution of the clause causes a failure, the script fails with the same mode.


## Serialisation

Tari Script and the execution stack are serialised into byte strings using a simple linear parser. Since all opcodes are
a single byte, it's very easy to read and write script byte strings. If an opcode has a parameter associated with it,
e.g. `PushHash` then it is equally known how many bytes following the opcode will contain the parameter. So for example,
a pay-to-public-key-hash script (P2PKH) script, when serialised is

```text
71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e58170ac
```

which maps to

```text
71  b0           7a       ae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e5  81          70   ac
Dup HashBlake256 PushHash(ae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e5) EqualVerify Drop CheckSig
```

The script input data is serialised in an analogous manner. The first byte in a stream indicates the type of data in the
bytes that follow. The length of each type is fixed and known _a priori_. The next _n_ bytes read represent the data type.

As input data elements are read in, they are pushed onto the stack. This means that the _last_ input element will typically
be operated on _first_!

The types of input parameters that are accepted are:

```rust,ignore
pub enum StackItem {
    Integer(i64),
    Hash(HashValue),
    PublicKey(RistrettoPublicKey),
    Signature(RistrettoSchnorr),
}
```

## Extensions

### Covenants

Tari script places restrictions on _who_ can spend UTXOs. It will also be useful for Tari digital asset applications to
restrict _how_ or _where_ UTXOs may be spent in some cases. The general term for these sorts of restrictions are termed
_covenants_. The [Handshake white paper] has a fairly good description of how covenants work.

It is beyond the scope of this RFC, but it's anticipated that Tari Script would play a key role in the introduction of
generalised covenant support into Tari.

### Lock-time malleability

The current Tari protocol has an issue with Transaction Output Maturity malleability. This output feature is enforced in
the consensus rules but it is actually possible for a miner to change the value without invalidating the transaction.

With TariScript, we can simply remove all the output features, except the coinbase feature completely and rely on
TariScript to reliably enforce UTXO lock times instead and any other features instead.

## Applications

### One-sided transactions

One-sided transactions are Mimblewimble payments that do not require the receiver to interact in the transaction
process. [LIP-004] describes how this will be implemented in Litecoin's Mimblewimble implementation. The main thrust is
that the sender uses Diffie-Hellman exchange to generate a shared private key that is used as the receiver's blinding
factor.

To prevent the sender from spending the coins (since both parties now know the spending key), there is an additional
commitment balance equation that is carried out on the block and transaction that requires the spender to know the
receiver's private key.

To implement one-sided payments in Tari, we propose using Diffie-Hellman exchange in conjunction with Tari Script to
achieve the same thing.

In particular, if Alice is sending some Tari to Bob, she generates a shared private key as follows:

$$ k_s = \mathrm{H}(k_a P_b) $$

where \\( P_b \\) is Bob's Tari node address, or any other public key Bob has shared with Alice. Alice can generate
an ephemeral public-private keypair, \\( P_a = k_a. G \\) for this transaction.

Alice then locks the output with the following script:

```text
Dup PushPubkey(P_B) EqualVerify CheckSig
```

where `P_B` is Bob's public key. As one can see, this Tari script is very similar to Bitcoin script.
The interpretation of this script is, "Given a Public key, and a signature of this
script, the public key must be equal to the one in the locking script, and the signature must be valid using the same
public key".

This is in effect the same as Bitcoin's P2PK script.

This script would be executed with Bob supplying his public key and signature signing the script above.

To illustrate the execution process, we show the script running on the left, and resulting stack on the right:

| Initial script    | Initial Stack         |
|:------------------|:----------------------|
| `Dup`             | Bob's Pubkey          |
| `PushPubkey(P_B)` | Bob's signature (R,s) |
| `EqualVerify`     |                       |
| `CheckSig`        |                       |

Copy Bob's pubkey:

| `Dup`             |                       |
|:------------------|:----------------------|
| `PushPubkey(P_B)` | Bob's Pubkey          |
| `EqualVerify`     | Bob's Pubkey          |
| `CheckSig`        | Bob's signature (R,s) |

Push the pubkey we need to the stack:

| `PushPubkey(P_B)` |                       |
|:------------------|:----------------------|
| `EqualVerify`     | P_b                   |
| `CheckSig`        | Bob's Pubkey          |
|                   | Bob's Pubkey          |
|                   | Bob's signature (R,s) |


Is `P_b` equal to `Bob's pubkey`?

| `EqualVerify` |                       |
|:--------------|:----------------------|
| `CheckSig`    | Bob's Pubkey          |
|               | Bob's signature (R,s) |

Check the signature, and if it is correct:

| `CheckSig` |     |
|:-----------|:----|
|            | `0` |
|            |     |

The script has completed and is successful.

To increase privacy, Alice could also lock the UTXO with a P2PKH
script:

```text
Dup HashBlake256 PushHash(HB) EqualVerify CheckSig
```

where `HB` is the hash of Bob's public key.

In either case, only someone with the knowledge of Bob's private key can generate a valid signature, so Alice will not
be able to unlock the UTXO to spend it.

Since the script is committed to and cannot be cut-through, only Bob will be able to spend this UTXO unless someone is
able to discover the private key from the public key information (the discrete log assumption), or if the majority of
miners collude to not honour the consensus rules governing the successful evaluation of the script (the 51% assumption).

### Non-malleable lock_height

A simple lock height script, of the "you can only spend this UTXO after block 420" variety:

```text
checkHeightVerify(420) PushZero
```

Note that we need the `PushZero` opcode, since the `xxxVerify` opCodes do not push any results to the stack.

###  Hash time-lock contract

Alice sends some Tari to Bob. If he doesn't spend it within a certain timeframe (up till block 4000), then she is also
able to spend it back to herself.

```text
Dup PushPubkey(P_b) CheckHeight(4000) IFTHEN PushPubkey(P_a) Or(2) Drop ELSE EqualVerify ENDIF CheckSig
```

Let's run through this script assuming it's block 3999 and Bob is spending the UTXO. We'll only print the stack this
time:

| Initial Stack   |
|:----------------|
| Bob's pubkey    |
| Bob's signature |

`Dup`:

| Stack           |
|:----------------|
| Bob's pubkey    |
| Bob's pubkey    |
| Bob's signature |

`PushPubkey(P_b)`:

| Stack           |
|:----------------|
| `P_b`           |
| Bob's pubkey    |
| Bob's pubkey    |
| Bob's signature |

`CheckHeight(4000)`. The block height height is 3999, so `max(0, 4000 - 3999)` is pushed to the stack:

| Stack           |
|:----------------|
| 1               |
| `P_b`           |
| Bob's pubkey    |
| Bob's pubkey    |
| Bob's signature |

`IFTHEN` compares the top of the stack to zero. It is not a match, so it will execute the `ELSE` branch:

`EqualVerify` checks that `P_b` is equal to Bob's pubkey:

| Stack           |
|:----------------|
| Bob's pubkey    |
| Bob's signature |


The `ENDIF` is a no-op, so the last instruction checks the given signature against Bob's pubkey:

`CheckSig`:

| Stack |
|:------|
| 0     |

Similarly, if it is after block 4000, and Alice tries to spend the UTXO, the sequence is:

| Initial Stack     |
|:------------------|
| Alice's pubkey    |
| Alice's signature |

`Dup` and `PushPubkey(P_b)` as before:

| Stack             |
|:------------------|
| `P_b`             |
| Alice's pubkey    |
| Alice's pubkey    |
| Alice's signature |

`CheckHeight(4000)` calculates `max(0, 4000 - 4001)` and pushes 0 to the stack:

| Stack             |
|:------------------|
| 0                 |
| `P_b`             |
| Alice's pubkey    |
| Alice's pubkey    |
| Alice's signature |

The top of the stack is zero, so `IFTHEN` executes the first branch, `PushPubkey(P_a)`:

| Stack             |
|:------------------|
| `P_a`             |
| `P_b`             |
| Alice's pubkey    |
| Alice's pubkey    |
| Alice's signature |

`Or(2)` compares the 3rd element, Alice's pubkey, with the 2 top items that were popped. There is a match, so the script
continues.

| Stack             |
|:------------------|
| Alice's pubkey    |
| Alice's pubkey    |
| Alice's signature |

`Drop`:

| Stack             |
|:------------------|
| Alice's pubkey    |
| Alice's signature |

`CheckSig`:

| Stack |
|:------|
| 0     |

### Credits

Thanks to [@philipr-za](https://github.com/philipr-za) and [@SWvheerden](https://github.com/SWvheerden) for their input
and contributions to this RFC.

[data commitments]: https://phyro.github.io/grinvestigation/data_commitments.html
[LIP-004]: https://github.com/DavidBurkett/lips/blob/master/lip-0004.mediawiki
[Scriptless script]: https://tlu.tarilabs.com/cryptography/scriptless-scripts/introduction-to-scriptless-scripts.html
[Handshake white paper]: https://handshake.org/files/handshake.txt

