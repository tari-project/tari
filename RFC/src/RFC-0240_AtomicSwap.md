# RFC-0240/Atomic Swap

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [S W van Heerden](https://github.com/SWvheerden)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2021 The Tari Development Community

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
SPECIAL, EXEMPLARY OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",
"NOT RECOMMENDED", "MAY" and "OPTIONAL" in this document are to be interpreted as described in
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as
shown here.

## Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community of the
technological merits of the potential system outlined herein.

## Goals

This Request for Comment (RFC) aims to describe how Atomic swaps will be created between two parties on different blockchains.

## Related Requests for Comment

* [RFC-0201: TariScript](RFC-0201_TariScript.md)
* [RFC-0202: TariScript Opcodes](RFC-0202_TariScriptOpcodes.md)

$$
\newcommand{\preimage}{\\phi} % pre image
\newcommand{\hash}[1]{\mathrm{H}\bigl({#1}\bigr)}
$$

## Description

Atomic swaps are atomic transactions that allow users to exchange different crypto assets and or coins without using a
central exchange and or trusting each other. Trading coins or assets this way makes it much more private and secure to
do and swap because no third party is required to be secure. Atomic swaps work on the principle of using 
[Hashed Time Lock Contracts](https://en.bitcoin.it/wiki/Hash_Time_Locked_Contracts)(HTLC). 
In short, it requires some hash pre-image to unlock the contract, or the time-lock can be used to reclaim the funds. 

In a cross-chain Atomic swap, both users lock up the funds to be exchanged on their respective chains in an HTLC-type contract. 
However, the two contracts’ pre-image, or spending secret, is the same, but only one party knows the correct pre-image. 
When the first HTLC contract is spent, this publicly reveals the pre-image for the other party to spend the second HTLC.
If the first HTLC is never spent, the second transaction's time-lock will allow the user to respend the funds back to
themselves after the time lock has passed.

### BTC - XTR AtomicSwap

#### Overview

BTC uses a scripting language for smart contracts on transactions which enables [atomic swaps](https://tlu.tarilabs.com/protocols/atomic-swaps/AtomicSwaps.html)
on the BTC chain. Traditionally [Mimblewimble] coins do not implement scripts, which makes Atomic swaps harder to
implement but not impossible. Grin has implemented atomic swaps using a version of a 2-of-2 multi-signature transaction
[mimblewimble atomic swaps](https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#atomic-swaps).
Fortunately, Tari does have scripting with [TariScript], which works a lot like BTC scripts, making the implementation simpler.
Because of the scripting similarities, the scripts to both HTLCs will look very similar, and we only need to ensure that
we use the same hash function in both. 

To do an Atomic swap from BTC to XTR, we need four wallets, two BTC wallets, and two XTR wallets, one wallet per person,
per coin.

As an example, Alice wants to trade some of her XTR for Bob's BTC. Alice and Bob need to agree on an amount of XTR and
BTC to swap. Once an agreement is reached, the swap is executed in the following steps:

* Alice chooses a set of random bytes, \\( \preimage \\), as the pre-image and hashes it with SHA256. She then sends
* the hash of the pre-image, \\( \hash{\preimage} \\), to Bob along with her BTC address.

* Bob sends her a public version of his [script key], \\( K_{Sb} \\), for use in the XTR transaction, which we can refer
to as Bob's script address.

* Alice creates a one-sided XTR transaction with an HTLC contract requiring \\( \preimage \\) as the input, which will
either payout to Bob's script address or her script address, \\( K_{Sa} \\), after a particular "time" has elapsed
(block height has been reached). 

* Bob waits for this transaction to be mined. When it is mined, he verifies that the UTXO spending script expects a
comparison of \\( \hash{\preimage} \\) as the first instruction, and that his public [script key], \\( K_{Sb} \\), will
be the final value remaining after executing the script. He has the private [script key], \\( k_{Sb} \\), to enable him
to produce a signature to claim the funds if he can get hold of the expected pre-image input value, \\( \preimage \\).
He also verifies that the UTXO has a sufficiently long time-lock to give him time to claim the transaction.

* Upon verification, Bob creates a Segwit HTLC BTC transaction with the same \\( \hash{\preimage} \\), which will spend
* to Alice's BTC address she gave him. It is essential to note that the time lock for this HTLC has to expire before
* the time lock of the XTR HTLC that Alice created.

* Alice checks the Bitcoin blockchain, and upon seeing that the transaction is mined, she claims the transaction, but,
* for her to do so, she has to make public what \\( \preimage \\) is as she has to use it as the witness of the 
claiming transaction.

* Bob sees that his BTC is spent, and looks at the witness to get \\( \preimage \\). Bob can then use \\( \preimage \\)
to claim the XTR transaction.

#### BTC - HTLC script 

Here is the required BTC script that Bob publishes:

``` btc_script,ignore
	OP_IF
	   OP_SHA256 <HASH256{pre_image}> OP_EQUALVERIFY
		<Alice BTC address> OP_CHECKSIG
	OP_ELSE
      <relative locktime>
      OP_CHECKSEQUENCEVERIFY
      OP_DROP
      <Bob BTC address> OP_CHECKSIG
   OP_ENDIF
```
_relative locktime_ is a time sequence in which Alice chooses to lock up the funds to give Bob time to claim this. 

#### XTR - HTLC script 

Here is the required XTR script that Alice publishes:

``` TariScript,ignore
   HashSha256 PushHash(HASH256{pre_image}) Equal
   IFTHEN
      PushPubkey(K_{Sb})
	ELSE
      CheckHeightVerify(height)
      PushPubkey(K_{Sa})
   ENDIF
```
(\\( K_{Sb} \\)) is the public key of the [script key] pair that Bob chooses to claim this transaction if Alice backs out. 
_height_ is an absolute block height that Bob chooses to lock up the funds to give Alice time to claim the funds. 

## XTR - XMR swap 

The Tari - Monero atomic swap involved a bit more detail than just a simple script and is explained in
[RFC-0241: XTR - XMR swap](RFC-0241_AtomicSwapXMR.md)


## Notation

Where possible, the "usual" notation is used to denote terms commonly found in cryptocurrency literature. Lower case
characters are used as private keys, while uppercase characters are used as public keys. New terms introduced here are
assigned greek lowercase letters in most cases. Some terms used here are noted down in [TariScript].

| Name        | Symbol              | Definition |
|:------------|---------------------| -----------|
| Pre-image   | \\( \preimage \\) | The random byte data used for the pre-image of the hash |
| 


[HTLC]: Glossary.md#hashed-time-locked-contract
[Mempool]: Glossary.md#mempool
[Mimblewimble]: Glossary.md#mimblewimble
[TariScript]: Glossary.md#tariscript
[script key]: Glossary.md#script-keypair
