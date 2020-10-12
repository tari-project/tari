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
It should also be fairly clear that basic Mimblewimble does not have the feature set to provide functionality such as:

* One-sided payments
* Atomic swaps (possible with scriptless scripts, but not easy)
* Hash time-locked contracts (possible with scriptless scripts, but not easy)
* Multiparty side-chain peg outs and peg-ins
* Generalised smart contracts

Extensions to Mimblewimble have been proposed for most of these features, for example, David Burkett's one-sided payment
proposal for LiteCoin ([LIP-004]), this project's [HTLC RFC](RFC-0230_HTLC.md), the pegging proposals for the
Clacks side-chain, and [Scriptless script]s.

This RFC makes the case that if Tari were to implement a scripting language similar to Bitcoin script, then all of these
use cases collapse and can be achieved under a single set of (relatively minor) modifications and additions to the
current Tari and Mimblewimble protocol.

## Scripting on Mimblewimble

To the author's knowledge, none of existing Mimblewimble projects have employed a scripting language. The reasons for
this are unclear, but there is at least one narrative in the space that it is
[not possible](https://forum.grin.mw/t/will-grin-allow-scripting-smart-contracts-in-the-future/7391/2) with
Mimblewimble. Given that [Grin](https://github.com/mimblewimble/grin) styles itself as a "Minimal implementation of the
Mimblewimble protocol", this status is unlikely to change soon.

As of this writing, the Beam project also considers Scriptless Script to be the
[extent of their scripting capabilities](https://docs.beam.mw/Beam_lightning_network_position_paper.pdf).

[Mimblewimble coin](https://github.com/mwcproject/mwc-node/blob/master/doc/roadmap.md) is a fork of Grin and "considers
the protocol ossified".

Litecoin is in the process of adding Mimblewimble as a
[side-chain](https://github.com/litecoin-project/lips/blob/master/lip-0003.mediawiki). As of this writing, there appear
to be no plans to include general scripting into the protocol.

### Scriptless scripts

[Scriptless script] is a wonderfully elegant technology and inclusion of Tari script does not preclude the use of
Scriptless script in Tari. However, scriptless scripts are difficult to reason about and development of them are best
left to experts in cryptographic proofs, leaving the development of Mimblewimble smart contracts in the hands of a very
select group of people.

It is the opinion of the author that there is no reason why Mimblewimble cannot be extended to include scripting.

## Tari script - a basic motivation

The essential idea of Tari script is as follows:

Given a standard Tari UTXO, we add _additional restrictions_ on whether that UTXO can be included as a valid input in a
transaction.

As long as those conditions are suitably committed to, and are not malleable throughout the existence of the UTXO, then
in general, these conditions are not that different to the requirement of having range proofs attached to UTXOs, which require
that the value of Tari commitments is non-negative.

Note that range proofs can be discarded after a UTXO is spent, since the global security guarantees of Mimblewimble are
not concerned that every transaction in history was valid from an inflation perspective, but that the net effect of all
transactions lead to zero inflation. This sounds worse than it is, since locally, every individual transaction is
checked for validity at the time of inclusion in the blockchain.

This argument is independent of the nature of the additional restrictions. Specifically, if these restrictions are
manifested as a script that provides additional constraints over whether a UTXO may be spent, the same arguments apply.

This means that from a philosophical viewpoint, there ought to be no reason that Tari Script is not workable, and
further, that pruning spent outputs (and possibly the scripts associated with them) is not that different from pruning
range proofs.

There is one key difference though that we need to address.

If it somehow happened that two illegal transactions made it into the blockchain (perhaps due to a bug), and the two
cancelled each other out, such that the global coin supply was still correct, one would never know this when doing a
chain synchronisation in pruned mode.

But if there was a steady inflation bug due to invalid range proofs making it into the blockchain, a pruned mode sync
would still detect that _something_ was awry, because the global coin supply balance acts as another check.

With Tari script, once the script has been pruned away, and then there is a re-org to an earlier point on the chain,
then there's no way to ensure that the script was honoured.
This is broadly in keeping with the Mimblewimble security guarantees that individual transactions are not necessarily verified during synchronisation.
However, the guarantee that no additional coins are created or destroyed remains intact.

Put another way, the blockchain relies on the network _at the time_ to enforce the Tari script spending rules. This means that the scheme may be susceptible to certain _horizon attacks_.

Incidentally, a single honest archival node would be able to detect any fraud on the same chain and provide a simple proof
that a transaction did not honour the redeem script.

### Additional requirements

The assumptions that broadly equate scripting with range proofs in the above argument are:

* The script (hash) must be committed to the blockchain.
* The script must not be malleable in any way without invalidating the transaction.
* The creator of the UTXO must commit to and sign the script (hash).

The script commitment, which can be adequately represented by the hash of the canonical serialisation of the script in
binary format, could be placed in the transaction kernel, or in a dedicated merkle mountain range for scripts.

Range proofs are not malleable because one must have knowledge of the UTXO blinding factor in order to generate a valid
range proof. However, it's trivial to replace scripts with other valid scripts, potentially to the point that miners or
malicious actors could take the UTXO for themselves.

Therefore, it's imperative that the UTXO creator sign the script.

Further, this signature must be present in the _kernel_ in some form, otherwise miners will be able to remove the script
via cut-through, whereas kernels are never pruned.

One approach to commit to the script hashes is to modify the output commitments using a variation of the [data commitments] approach
first suggested by [Phyro](https://github.com/phyro). In this approach, when creating a new UTXO, the owner also calculates
the hash of the locking script, _s_, such that `s = H(script)`. The script hash gets stored in the UTXO itself.

## Protocol modifications

There are several changes to the protocol data structures that must be made to allow this scheme to work. The first is a relatively minor adjustment to the output commitment.
The second is an addition of several pieces of data to the transaction kernel.
The third is the addition of a new section in the transaction and block definition to hold the script data.

Finally, the balance ans signature consensus rules must be updated to account for these changes.

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

Under Tari script, this would change to

```rust,ignore
pub struct TransactionOutput {
    features: OutputFeatures,
    commitment: Commitment,
    proof: RangeProof,
    /// New: The hash of the locking script on this UTXO.
    script_hash: HashOutput, 
}
```

Now when calculating the transaction or block balance, we calculate a different set of commitments. The current commitment,

$$ C_a = v_a.H + k_a.G $$

is modified with a commitment to the script hash, as follows:

$$
\begin{align}
\sigma_a &= \mathrm{H}(s_a) \\\\
L_a &= k_{a'}. G \\\\
\hat{C_a} &= C_a + L_a\sigma_a \\\\
&= k_a.G + k_{a'}\sigma_a . G + v_a . H \\\\
&= P_a + L_a\sigma_a + v_a . H
\end{align}
$$

and parties will sign the kernel with $$ k_a  + k_{a'}\sigma_a $$ rather than just \\( k \\).

The overall and block balance checks must also be modified to use \\( \hat{C} \\) rather than \\( C \\).

### Kernel changes

The key components of the current kernel definition are:

* The transaction metadata, including the fee, lock height, and other kernel features,
* The public excess for the transaction,
* A signature from all parties in the transaction signing the excess and committing to the metadata.

For Tari script, we require tha addition of two additional fields:

* Input script hashes. An array of tuples of \\( (L_i, \sigma_i) \\), the script key and script hash, for every input in the transaction,
* Output script hashes. An array of tuples of \\( (L_i, \sigma_i) \\), the script key and script hash, for every output in the transaction.

The script key, \\( L_i \\) is the public counterpart of a key chosen by the creator of the UTXO.
The secret portion of the script key, \\( k_{i'} \\) effectively locks the script hash and prevents third parties from modifying the script portion of the output commitment without detection.
This was a vulnerability in the original [data commitments] scheme which is resolved with this modification.

### Script section

A new section in the Mimblewimble aggregate body is a set of scripts, and their associated data, for every input in the transaction.

Specifically, the script data comprises:
* script: The serialised binary representation of the script. The hash of this data MUST equal one of the script hashes supplied in the kernel input script hash array, unless the script hash is the null script.
* script data: The serialised representation of the data to push onto the stack prior to executing of the script.

The exception is for the default script, `PUSH(0)`. This script is trivially satisfied.
And since it has an invariant, and public hash, there is no need to replicate the script in this section for any default script hash values in the kernel.
### Transaction balance

The Mimblewimble transaction balance must be modified to take the script hash commitments into account. This is fairly straightforward.

Given a set of inputs, outputs, the transaction fee, \\( f \\), and the transaction offset, \\( \delta \\):

$$
\begin{align}
  & \sum(\mathrm{Inputs}) - \sum(\mathrm{Outputs}) - \sum(f_i.G)   \\\\
 =& \sum \pm\hat{C_i} - \sum(f_i.G)   \\\\
 =& \sum \pm C_i \pm L_i\sigma_i - \sum(f_i.G)
\end{align}
$$

Here the \\( \pm \\) operator is used to simply the notation and indicates the addition operation when the following term is associated with an input and subtraction when the term related to an output.

If the accounting is correct, all values will cancel out:

$$
\begin{align}
  &= \sum(\pm k_i.G \pm L_i.\sigma_i) \\\\
  &= \sum \pm P_i \pm \sum L_i.\sigma_i \\\\
  &= X_s \pm \sum L_i\sigma_i
\end{align}
$$

Where \\( X_s \\), the sum of all the public keys, or blinding factors (times G), is the definition of the standard Mimblewimble excess.

### Kernel signature

The kernel signature must also be adapted to account for the fact that the residual of the block balance is no longer simply the public excess, but also includes the script hashes and script keys.

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

The exercise above demonstrates that $$x_s + \sum \pm L_i\sigma_i$$ signs the kernel correctly. The kernel offset
is not included in this treatment, but it does not affect the result. One of the input commitments will be offset by a
value selected by the sender and provided with the transaction data as usual. The signatures will still validate as
usual, and the kernel offset will correct the overall excess balance.

The same treatment extends to be block validation check. Note that since the individual kernel excesses can still be
summed to obtain the overall block balance.
Furthermore, at the block aggregation level, it is non-trivial to assign a script hash to a given UTXO, and so the
dis-association of kernels and their outputs is maintained.

### Additional consensus rules

In addition to the changes given above, there are two additional consensus rule changes for transaction and block validation.

For every valid block or transaction,

1. Every script hash in the kernel MUST be a default script hash or match a script in the script section. If there are duplicate script hashes in the kernel, then only one script MUST be presented in the script section.
2. The script, with it's input data, if any, MUST execute successfully, i.e. without any errors. After execution, the stack MUST have exactly one element and its value MUST be exactly zero.

### Checking the requirements

Let's check whether the scripts are suitably committed on the blockchain, whether there's any malleability from miners or transaction participants and whether a new syncing node can verify the correctness of the chain.

### Malleability

Let's say a malicious party, Bob, can modify a transaction before it enters the blockchain.

First, Bob simply tries to modify the kernel, replacing one of the kernel script hashes from \\( (L_a, \sigma_a) \\) to
\\( (L_a, \sigma_b) \\). Since \\( k_{a'}\sigma_a \\) is part of the UTXO commitment, the overall balance would fail.

Bob then tries to apply the attack described in [data commitments], and tries to modify the UTXO commitment as follows:

$$  
\begin{align}
  \hat{C} &= k_i.G + k_{a'}\sigma_a.G + v.H  \\\\
  \Rightarrow \hat{C_b} &= k_i.G + k_{a'}\sigma_a.G + v.H - k_{a'}\sigma_a.G + k_{b'}\sigma_a.G \\\\
            &= k_i.G + v.H + k_{b'}.G \\\\
            &= C_a + L_b\sigma_b
\end{align}  
$$

So far, so bad. But Bob is still required to produce a signature. Bob needs to replace the signature with one that signs
with his script:

$$
  s \Rightarrow s^* = s - k_{a'}\sigma_a.e + k_{b'}\sigma_b.e
$$

To do this, Bob must know the value of \\( k_{a'} \\) which requires him to find the discrete log of \\( L_a \\), an infeasable axercise.

### Cut-through

A major issue with many Mimblewimble extension schemes is that miners are able to cut-through UTXOs if an output is spent
in the same block it was created.

In this proposal, the script hashes are committed in the kernels as well as the UTXOs and the scripts themselves are
provided in a separate part of the block. Therefore, even if the UTXO related to the script has been cut through, the
consensus rules would _still_ require the script to be validated.

Recall that by the time the block is assembled, the link between output and script has been severed. Nodes only have a  
list of scripts and their hashes and must validate that they all produce a zero result. (TODO - currently, the script input is malleable. How to fix this?)

By extension, UTXOs can still be pruned because the \\( \L_i\sigma_i \\) values change sign when used as inputs and will
cancel out in the overall balance in the same way that the pruned out excesses are.

### Script lock key generation

At face value, it looks like the burden for wallets has doubled, since each UTXO owner has to remember two private keys,  
the spend key, \(( k_i \\) and the script key \\( k_{i'} \\). In practice, the script lock key can be
deterministically derived from the spend key. For example, the script key can be equal to the hash of the spend key.




## Disadvantages

Size implications

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
* The Rust type system ensures that only compatible data types can be operated on, e.g. A public key cannot be added to
  an integer scalar. Errors of this kind cause the script to fail. Non-reference implementations MUST replicate this behaviour.

### Opcodes

Tari Script opcodes are enumerated from 0 to 255 and are represented as a single unsigned byte. The opcode set is
initially limited to allow for the applications specified in this RFC, but can be expanded in future.

```rust,ignore
pub enum Opcode {
    /// Push the current chain height onto the stack
    PushHeight,
    /// Push the associated 32-byte value onto the stack
    PushHash(Box<HashValue>),
    /// Hash the top stack element with the Blake256 hash function and push the result to the stack
    HashBlake256,
    /// Fail the script immediately. (Must be executed.)
    Return,
    /// Drops the top stack item
    Drop,
    /// Duplicates the top stack item
    Dup,
    /// Reverse rotation. The top stack item moves into 3rd place, abc => bca
    RevRot,
    /// Pop two items and push their sum
    Add,
    /// Pop two items and push the second minus the top
    Sub,
    /// Pop the public key and then the signature. If the signature signs the script, push 0 to the stack, otherwise
    /// push 1
    CheckSig,
    /// As for CheckSig, but aborts immediately if the signature is invalid. As opposed to Bitcoin, it pushes a zero
    /// to the stack if successful
    CheckSigVerify,
    /// Pushes 0, if the inputs are exactly equal, 1 otherwise
    Equal,
    /// Pushes 0, if the inputs are exactly equal, aborts otherwise
    EqualVerify,
}
```

### Serialisation

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

Input parameters are serialised in an analogous manner.

The types of input parameters that are accepted are:

```rust,ignore
pub enum StackItem {
    Number(i64),
    Hash(HashValue),
    Commitment(PedersenCommitment),
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

The lock time could also be added to the script commitment hash to solve this problem.
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

where \\( P_b \\) is Bob's Tari node address, or any other public key that Bob has shared with Alice. Alice can generate
an ephemeral public-private keypair, \\( P_a = k_a. G \\) for this transaction.

Alice then locks the output with the following script:

```text
Dup PushPubkey(P_B) EqualVerify CheckSig Add
```

where `P_B` is Bob's public key. As one can see, this Tari script is very similar to Bitcoin script.
The interpretation of this script is, "Given a Public key, and a signature of this
script, the public key must be equal to the one in the locking script, and the signature must be valid using the same
public key".

This is in effect the same as Bitcoin's P2PK script. To increase privacy, Alice could also lock the UTXO with a P2PKH
script:

```text
Dup HashBlake256 PushHash(HB) EqualVerify CheckSig Add
```

where `HB` is the hash of Bob's public key.

In either case, only someone with the knowledge of Bob's private key can generate a valid signature, so Alice will not
be able to unlock the UTXO to spend it.

Since the script is committed to and cannot be cut-through, only Bob will be able to spend this UTXO unless someone is
able to discover the private key from the public key information (the discrete log assumption), or if the majority of
miners collude to not honour the consensus rules governing the successful evaluation of the script (the 51% assumption).


### Credits

Thanks to [@philipr-za](https://github.com/philipr-za) and [@SWvheerden](https://github.com/SWvheerden) for their input
and contributions to this RFC.

[data commitments]: https://phyro.github.io/grinvestigation/data_commitments.html
[LIP-004]: https://github.com/DavidBurkett/lips/blob/master/lip-0004.mediawiki
[Scriptless script]: https://tlu.tarilabs.com/cryptography/scriptless-scripts/introduction-to-scriptless-scripts.html
[Handshake white paper]: https://handshake.org/files/handshake.txt

