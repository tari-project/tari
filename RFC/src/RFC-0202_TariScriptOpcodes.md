# RFC-0201/TariScriptOpcodes

## Tari Script Opcodes

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

This Request for Comment (RFC) defines the opcodes that make up the TariScript scripting language and provides some 
examples and applicaitons.

## Related Requests for Comment

* [RFC-0201: TariScript](RFC-201_TariScript.md)
* [RFC-0200: Base Layer Extensions](BaseLayerExtensions.md)


## Introduction


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

