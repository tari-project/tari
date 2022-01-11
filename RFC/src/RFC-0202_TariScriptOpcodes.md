# RFC-0202/TariScriptOpcodes

## TariScript Opcodes

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

* [RFC-0201: TariScript](RFC-0201_TariScript.md)
* [RFC-0200: Base Layer Extensions](BaseLayerExtensions.md)


## Introduction


## TariScript semantics

The proposal for TariScript is straightforward. It is based on Bitcoin script and inherits most of its ideas.

The main properties of [TariScript] are

* The scripting language is stack-based. At redeem time, the UTXO spender must supply an input stack. The script runs by
  operating on the stack contents.
* If an error occurs during execution, the script fails.
* After the script completes, it is successful if and only if it has not aborted, and there is exactly a single element
  on the stack. The script fails if the stack is empty, or contains more than one element, or aborts early.
* It is not Turing complete, so there are no loops or timing functions.
* The opcodes enforce type safety. e.g. A public key cannot be added to an integer scalar. Errors of this kind MUST cause
  the script to fail. The Rust implementation of [TariScript] automatically applies the type safety rules.

### Failure modes

Bitcoin transactions can "fail" in two main ways: Either there is a genuine error in the locking or unlocking script;
or a wallet broadcasts a _[Non-standard transaction]_ to a non-mining node. To be more precise, Bitcoin core nodes
only accept a small subset of valid transaction scripts that are deemed [Standard transactions]. To succesfully produce
a non-standard Bitcoin transaction, one has to submit it directly to a miner that accepts non-standard transactions.

It's interesting to note that only 0.02% of transactions
mined in Bitcoin before block 550,000 were non-standard, and it appears that the vast majority of these were in fact
unintentional, leading to loss of funds [(1)].

[Standard transactions]: https://www.oreilly.com/library/view/mastering-bitcoin/9781491902639/ch05.html
[Non-standard transaction]: https://www.frontiersin.org/articles/10.3389/fbloc.2019.00007/full
[(1)]: https://www.frontiersin.org/articles/10.3389/fbloc.2019.00007/full

The present RFC proposes that TariScript not identify a subset of transactions as "standard". However, some transactions
might be invalid in and of- themselves (e.g. invalid signature, invalid script), while others may be invalid because of
the execution context (e.g. a lock time has not expired).

All Tari nodes MUST reject the former type of invalid transaction; and SHOULD reject the latter. In these instances, it
is the _wallets'_ responsibility to wait until transactions are valid before broadcasting them.

Rejected transactions are simply silently dropped.

This policy discourages spamming on the network and promotes responsible behaviour by wallets.

The full list of [Error codes](#error-codes) is given below.

### Constraints

* The maximum length of a script when serialised is 1,024 bytes.
* The maximum length of a script's input is 1,024 bytes.
* The maximum stack height is 255.

## Opcodes

[TariScript] opcodes range from 0 to 255 and are represented as a single unsigned byte. The opcode set is
limited to allow for the applications specified in this RFC, but can be expanded in the future.

### Block height checks

All these opcodes test the current block height (or, if running the script as part of a transaction
validation, the next earliest block height) against a given value.

##### CheckHeightVerify(height)

Compare the current block height to `height`.

Fails with `VERIFY_FAILED` if the block height < `height`.

##### CheckHeight(height)

Pushes the value of (the current height - `height`) to the stack. In other words,
the top of the stack will hold the height difference between `height` and the current height. If the chain has
progressed beyond height, the value is positive; and negative if the chain has yet to reach `height`.

Fails with `STACK_OVERFLOW` if the stack would exceed the max stack height.

##### CompareHeightVerify

The same as [`CheckHeightVerify`](#checkheightverifyheight), except that the height to compare against the block height
  is popped off the stack instead of being hardcoded in the script. Pops the top of the stack as `height` and compares
  it to the current block height.

* Fails with `INVALID_INPUT` if there is not a valid integer value on top of the stack.
* Fails with `EMPTY_STACK` if the stack is empty.
* Fails with `VERIFY_FAILED` if the block height < `height`.


##### CompareHeight

The same as [`CheckHeight`](#checkheightheight) except that the height to compare against the block height
  is popped off the stack instead of being hardcoded in the script.

Pops the top of the stack as `height`, then pushes the value of (`height` - the current height) to the stack.
In other words, this opcode replaces the top of the stack with the difference between that value and the current height.

* Fails with `INVALID_INPUT` if there is not a valid integer value on top of the stack.
* Fails with `EMPTY_STACK` if the stack is empty.

### Stack manipulation

##### NoOp

Does nothing. Never fails.

##### PushZero

Pushes a zero onto the stack. This is a very common opcode and has the same effect as `PushInt(0)` but is more compact.
`PushZero` can also be interpreted as `PushFalse` (although no such opcode exists).

* Fails with `STACK_OVERFLOW` if the stack would exceed the max stack height.

##### PushOne

Pushes a one onto the stack. This is a very common opcode and has the same effect as `PushInt(1)` but is more compact.
`PushOne` can also be interpreted as `PushTrue`, although no such opcode exists.

* Fails with `STACK_OVERFLOW` if the stack would exceed the max stack height.

##### PushHash(HashValue)

Push the associated 32-byte value onto the stack.

* Fails with `INVALID_SCRIPT_DATA` if HashValue is not a valid 32 byte sequence
* Fails with `STACK_OVERFLOW` if the stack would exceed the max stack height.

##### PushInt(i64)

Push the associated 64-bit signed integer onto the stack

* Fails with `INVALID_SCRIPT_DATA` if `i64` is not a valid integer.
* Fails with `STACK_OVERFLOW` if the stack would exceed the max stack height.

##### PushPubKey(PublicKey)

Push the associated 32-byte value onto the stack. It will be interpreted as a public key or a commitment.

* Fails with `INVALID_SCRIPT_DATA` if HashValue is not a valid 32 byte sequence
* Fails with `STACK_OVERFLOW` if the stack would exceed the max stack height.

##### Drop

Drops the top stack item.

* Fails with `EMPTY_STACK` if the stack is empty.

##### Dup

Duplicates the top stack item.

* Fails with `EMPTY_STACK` if the stack is empty.
* Fails with `STACK_OVERFLOW` if the stack would exceed the max stack height.

##### RevRot

Reverse rotation. The top stack item moves into 3rd place, e.g. `abc => bca`.

* Fails with `EMPTY_STACK` if the stack has fewer than three items.

### Math operations

#### GeZero

Pops the top stack element as `val`. If `val` is greater than or equal to zero, push a 1 to the stack, otherwise push 0.

* Fails with `EMPTY_STACK` if the stack is empty.
* Fails with `INVALID_INPUT` if `val` is not an integer.

#### GtZero

Pops the top stack element as `val`. If `val` is strictly greater than zero, push a 1 to the stack, otherwise push 0.

* Fails with `EMPTY_STACK` if the stack is empty.
* Fails with `INVALID_INPUT` if the item is not an integer.

#### LeZero

Pops the top stack element as `val`. If `val` is less than or equal to zero, push a 1 to the stack, otherwise push 0.

* Fails with `EMPTY_STACK` if the stack is empty.
* Fails with `INVALID_INPUT` if the item is not an integer.

#### LtZero

Pops the top stack element as `val`. If `val` is strictly less than zero, push a 1 to the stack, otherwise push 0.

* Fails with `EMPTY_STACK` if the stack is empty.
* Fails with `INVALID_INPUT` if the items is not an integer.

##### Add

Pop two items and push their sum

* Fails with `EMPTY_STACK` if the stack has fewer than two items.
* Fails with `INVALID_INPUT` if the items cannot be added to each other (e.g. an integer and public key).

##### Sub

Pop two items and push the second minus the top

* Fails with `EMPTY_STACK` if the stack has fewer than two items.
* Fails with `INVALID_INPUT` if the items cannot be subtracted from each other (e.g. an integer and public key).

##### Equal

Pops the top two items, and pushes 1 to the stack if the inputs are exactly equal, 0 otherwise. 0 is also pushed if the
values cannot be compared (e.g. integer and pubkey).

* Fails with `EMPTY_STACK` if the stack has fewer than two items.

##### EqualVerify

Pops the top two items, and compares their values.

* Fails with `EMPTY_STACK` if the stack has fewer than two items.
* Fails with `VERIFY_FAILED` if the top two stack elements are not equal.

### Boolean logic

#### Or(n)

`n` + 1 items are popped from the stack. If the last item popped matches at least one of the first `n` items popped,
push 1 onto the stack. Push 0 otherwise.

* Fails with `EMPTY_STACK` if the stack has fewer than `n` + 1 items.

#### OrVerify(n)

`n` + 1 items are popped from the stack. If the last item popped matches at least one of the first `n` items popped,
continue. Fail with `VERIFY_FAILED` otherwise.

* Fails with `EMPTY_STACK` if the stack has fewer than `n` + 1 items.

### Cryptographic operations

##### HashBlake256

Pop the top element, hash it with the Blake256 hash function and push the result to the stack.

* Fails with `EMPTY_STACK` if the stack is empty.

##### HashSha256

Pop the top element, hash it with the SHA256 hash function and push the result to the stack.

* Fails with `EMPTY_STACK` if the stack is empty.

##### HashSha3

Pop the top element, hash it with the SHA-3 hash function and push the result to the stack.

* Fails with `EMPTY_STACK` if the stack is empty.

##### CheckSig(Msg)

Pop the public key and then the signature. If the signature signs the 32-byte message, push 1 to the stack, otherwise
push 0.

* Fails with `INVALID_SCRIPT_DATA` if the `Msg` is not a valid 32-byte value.
* Fails with `EMPTY_STACK` if the stack has fewer than 2 items.
* Fails with `INVALID_INPUT` if the top stack element is not a PublicKey or Commitment
* Fails with `INVALID_INPUT` if the second stack element is not a Signature

##### CheckSigVerify(Msg),

Identical to [`CheckSig`](#checksigmsg), except that nothing is pushed to the stack if the signature is valid, and the
operation fails with `VERIFY_FAILED` if the signature is invalid.

##### ToRistrettoPoint,

Pops the top element which must be a valid Ristretto scalar, calculates the corresponding Ristretto point, and pushes this to the stack.

* Fails with `EMPTY_STACK` if the stack is empty.
* Fails with `INVALID_INPUT` if the top stack element is not a scalar.

### Miscellaneous

##### Return

Always fails with `VERIFY_FAILED`.

##### If-then-else

The if-then-else clause is marked with the `IFTHEN` opcode.
When the `IFTHEN` opcode is reached, the top element of the stack is popped into `pred`.
If `pred` is 1, the instructions between `IFTHEN` and `ELSE` are executed. The instructions from `ELSE` to `ENDIF` are
then popped without being executed.

If `pred` is 0, instructions are popped until `ELSE` or `ENDIF` is encountered.
If `ELSE` is encountered, instructions are executed until `ENDIF` is reached.
`ENDIF` is a marker opcode and a no-op.

* Fails with `EMPTY_STACK` if the stack is empty.
* If `pred` is anything other than 0 or 1, the script fails with `INVALID_INPUT`.
* If any instruction during execution of the clause causes a failure, the script fails with that failure code.

## Serialisation

TariScript and the execution stack are serialised into byte strings using a simple linear parser. Since all opcodes are
a single byte, it's very easy to read and write script byte strings. If an opcode has a parameter associated with it,
e.g. `PushHash` then it is equally known how many bytes following the opcode will contain the parameter.

The script input data is serialised in an analogous manner. The first byte in a stream indicates the type of data in the
bytes that follow. The length of each type is fixed and known _a priori_. The next _n_ bytes read represent the data type.

As input data elements are read in, they are pushed onto the stack. This means that the _last_ input element will typically
be operated on _first_!

The types of input parameters that are accepted are:

| Type            | Range / Value                                                          |
|:----------------|:-----------------------------------------------------------------------|
| Integer         | 64-bit signed integer                                                  |
| Hash            | 32-byte hash value                                                     |
| PublicKey       | 32-byte Ristretto public key                                           |
| Signature       | 32-byte nonce + 32-byte signature                                     |
| Data            | single byte, n, indicating length of data, followed by n bytes of data |
| RistrettoScalar | 32-byte Ristretto secret key                                           |

## Example scripts

### Anyone can spend

The simplest script is an empty script, or a script with a single `NoOp` opcode. When faced with this script, the spender
can supply any pubkey in her script input for which she knows the private key. The script will execute, leaving that
public key as the result, and the transaction script validation will pass.

### One-sided transactions

One-sided transactions lock the input to a predetermined public key provided by the recipient; essentially the same
method that Bitcoin uses. The simplest form of this is to simply post the new owner's public key as the script:

```text
PushPubkey(P_B)
```

To spend this output, Bob provides an empty input stack. After execution, the stack contains his public key.

An equivalent script to Bitcoin's P2PKH would be:

```text
Dup HashBlake256 PushHash(PKH) EqualVerify
```

To spend this, Bob provides his public key as script input.
To illustrate the execution process, we show the script running on the left, and resulting stack on the right:

| Initial script  | Initial Stack |
|:----------------|:--------------|
| `Dup`           | Bob's Pubkey  |
| `HashBlake256`  |               |
| `PushHash(PKH)` |               |
| `EqualVerify`   |               |

Copy Bob's pubkey:

| `Dup`           |              |
|:----------------|:-------------|
| `HashBlake256`  | Bob's Pubkey |
| `PushHash(PKH)` | Bob's Pubkey |
| `EqualVerify`   |              |

Hash the public key:

| `HashBlake256`  |                 |
|:----------------|:----------------|
| `PushHash(PKH)` | H(Bob's Pubkey) |
| `EqualVerify`   | Bob's Pubkey    |

Push the expected hash to the stack:

| `PushHash(PKH)` |                 |
|:----------------|:----------------|
| `EqualVerify`   | PKH             |
|                 | H(Bob's Pubkey) |
|                 | Bob's Pubkey    |

Is `PKH` equal to the hash of Bob's public key?

| `EqualVerify` |              |
|:--------------|:-------------|
|               | Bob's Pubkey |

The script has completed without errors, and Bob's public key remains on the stack.

###  Time-locked contract

Alice sends some Tari to Bob. If he doesn't spend it within a certain timeframe (up till block 4000), then she is also
able to spend it back to herself.

The spender provides their public key as input to the script.

```text
Dup PushPubkey(P_b) CheckHeight(4000) GeZero IFTHEN PushPubkey(P_a) OrVerify(2) ELSE EqualVerify ENDIF
```

Let's run through this script assuming it's block 3990 and Bob is spending the UTXO. We'll only print the stack this
time:

| Initial Stack   |
|:----------------|
| Bob's pubkey    |

`Dup`:

| Stack           |
|:----------------|
| Bob's pubkey    |
| Bob's pubkey    |

`PushPubkey(P_b)`:

| Stack           |
|:----------------|
| `P_b`           |
| Bob's pubkey    |
| Bob's pubkey    |

`CheckHeight(4000)`. The block height is 3990, so `3990 - 4000` is pushed to the stack:

| Stack           |
|:----------------|
| -10             |
| `P_b`           |
| Bob's pubkey    |
| Bob's pubkey    |

`GeZero` pushes a 1 if the top stack element is positive or zero:

| Stack        |
|:-------------|
| 0            |
| `P_b`        |
| Bob's pubkey |
| Bob's pubkey |

`IFTHEN` compares the top of the stack to 1. It is not a match, so it will execute the `ELSE` branch:

| Stack        |
|:-------------|
| `P_b`        |
| Bob's pubkey |
| Bob's pubkey |

`EqualVerify` checks that `P_b` is equal to Bob's pubkey:

| Stack           |
|:----------------|
| Bob's pubkey    |

The `ENDIF` is a no-op, so the stack contains Bob's public key, meaning Bob must sign to spend this transaction.

Similarly, if it is after block 4000, say block 4005, and Alice or Bob tries to spend the UTXO, the sequence is:

| Initial Stack         |
|:----------------------|
| Alice or Bob's pubkey |

`Dup` and `PushPubkey(P_b)` as before:

| Stack                    |
|:-------------------------|
| `P_b`                    |
| Alice or Bob's pubkey    |
| Alice or Bob's pubkey    |

`CheckHeight(4000)` calculates `4005 - 4000)` and pushes 5 to the stack:

| Stack                 |
|:----------------------|
| 5                     |
| `P_b`                 |
| Alice or Bob's pubkey |
| Alice or Bob's pubkey |


`GeZero` pops the 5 and pushes a 1 to the stack:

| Stack                 |
|:----------------------|
| 1                     |
| `P_b`                 |
| Alice or Bob's pubkey |
| Alice or Bob's pubkey |

The top of the stack is 1, so `IFTHEN` executes the first branch, `PushPubkey(P_a)`:

| Stack                 |
|:----------------------|
| `P_a`                 |
| `P_b`                 |
| Alice or Bob's pubkey |
| Alice or Bob's pubkey |

`OrVerify(2)` compares the 3rd element, Alice's pubkey, with the 2 top items that were popped. There is a match, so the script
continues.

| Stack                 |
|:----------------------|
| Alice or Bob's pubkey |

If the script executes successfully, then either Alice's or Bob's public key is left on the stack, meaning only Alice
or Bob can spend the output.

### Error codes

| Code                        | Description                                                                      |
|:----------------------------|:---------------------------------------------------------------------------------|
| `SCRIPT_TOO_LONG`           | The serialised script exceeds 1024 bytes.                                        |
| `SCRIPT_INPUT_TOO_LONG`     | The serialised script input exceeds 1024 bytes.                                  |
| `STACK_OVERFLOW`            | The stack exceeded 255 elements during script execution                          |
| `EMPTY_STACK`               | There was an attempt to pop an item off an empty stack                           |
| `INVALID_OPCODE`            | The script cannot be deserialised due to an invalid opcode                       |
| `INVALID_SCRIPT_DATA`       | An opcode parameter is invalid or of the wrong type                              |
| `INVALID_INPUT`             | Invalid or incompatible data was popped off the stack as input into an operation |
| `VERIFY_FAILED` | A script condition (typically a `nnnVerify` opcode) failed                       |

### Credits

Thanks to [@philipr-za](https://github.com/philipr-za) and [@SWvheerden](https://github.com/SWvheerden) for their input
and contributions to this RFC.

[TariScript]: Glossary.md#tariscript
