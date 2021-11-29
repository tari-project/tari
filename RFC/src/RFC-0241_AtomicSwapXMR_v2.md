# RFC-0241/XMR Atomic Swap

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

This Request for Comment (RFC) aims to describe how an Atomic swap between Tari and Monero will be created.

## Related Requests for Comment

* [RFC-0201: TariScript](RFC-0201_TariScript.md)
* [RFC-0202: TariScript Opcodes](RFC-0202_TariScriptOpcodes.md)

$$
\newcommand{\script}{\alpha} % utxo script
\newcommand{\input}{ \theta }
\newcommand{\cat}{\Vert}
\newcommand{\so}{\gamma} % script offset
\newcommand{\hash}[1]{\mathrm{H}\bigl({#1}\bigr)}
$$

## Description

Doing atomic swaps with Monero is more complicated and requires a crypto dance to complete as Monero does not
implement any form of HTLC's or the like. This means that when doing an atomic swap with Monero, most of the logic will
have to be implemented on the Tari side. Atomic swaps between Monero and bitcoin have been implemented by the [Farcaster  project](https://github.com/farcaster-project/RFCs)
and the [commit team](https://github.com/comit-network/xmr-btc-swap).

### Method

The primary, happy outline of a Tari - Monero atomic swap is described here, and more detail will follow. We will assume here that Alice wants to trade her XTR for Bob's XMR.

* Negotiation - Here, both parties negotiate about the values and how the Monero and Tari Utxo's will look
* Commitment - Here, both parties commit to their use of keys, and Bob commits to the refund transaction
* XTR payment - Here, the XTR payment is made to a multi-party UTXO containing a script
* XMR Payment - The Monero payment is made to a multiparty [scriptless script](https://tlu.tarilabs.com/cryptography/introduction-to-scriptless-scripts) UTXO.
* Claim XTR - Here, the XTR is claimed, and in claiming it, the XMR private key is revealed
* Claim XMR - Here, the XMR is claimed with the revealed key.

Please take note of the notation used in [TariScript] and specifically notation used on the signatures on the [transaction inputs](RFC-0201_TariScript.md#transaction-input-changes) and on the signatures on the [transaction outputs](RFC-0201_TariScript.md#transaction-output-changes), other notation will be noted in the [Notation](#notation) section.


### TL;DR

This scheme revolves around Alice, who wants to exchange some of her Tari for Bob's Monero. Because they don't 
trust each other, they have to commit some values to do the exchange. And if something goes wrong here, we want to ensure
that we can refund both parties either in Monero or Tari.

How this works is that Alice and Bob create a shared output on both chains. The Monero output is a simple aggregate key
to unlock the UTXO, while the Tari UTXO is unlocked by either one of two keys. The block height determines the unlock key on Tari. 

The TariScript will decide the unlock key to claim the Tari amount. The script will require the user to input their part
of the Monero aggregate key used to lock the Monero UTXO. The script can end one of three ways, one for the happy path if
Alice gives Bob the pre_image after she checked and verified the Monero UTXO, one for her to reclaim her Tari amount if Bob
disappears or tries to break the contract. And lastly, one for Bob to claim the Tari if Alice disappears after he publishes the
Monero UTXO.

### TariScript

The Script used for the Tari UTXO is as follows:
``` TariScript,ignore
   CheckHeight(height_1)
   LtZero
   IFTHEN
      HashSha256 
      PushHash(HASH256{pre_image})
      EqualVerify
      Ristretto
      PushPubkey(X_b)
      EqualVerify
      PushPubkey(K_{Sb})
   Else
      CheckHeight(height_2)
      LtZero
      IFTHEN
         Ristretto
         PushPubkey(X_a)
         EqualVerify
         PushPubkey(K_{Sa})
      Else
         Ristretto
         PushPubkey(X_b)
         EqualVerify
         PushPubkey(K_{Sb})
      ENDIF
   ENDIF
```

Here `height_1` is the lock height till Alice can claim the transaction. If Alice fails to publish the refund transaction
after `height_2,` Bob can claim the lapse transaction.

### Negotiation

Alice and Bob have to negotiate the exchange rate and the amount to be exchanged in the atomic swap. 
They also need to decide how the two UTXO's will look on the blockchain. To accomplish this, the following needs to be finalized:

* Amount of Tari to swap for the amount of Monero
* Monero public key parts \\(X_a\\), \\(X_b\\) and its aggregate form \\(X\\)
* Tari [script key] parts \\(K_{Sa}\\), \\(K_{Sb}\\) 
* The [TariScript] to be used in the Tari UTXO
* The blinding factor \\(k_i\\) for the Tari UTXO, this can be a Diffie-Hellman between their addresses.


### Key construction

We need to use multi-signatures with Schnorr signatures to ensure that the keys are constructed so that key
cancellation attacks are not possible. To do this, we follow the Musig way of creating keys. 
Musig keys are constructed in the following way if there are two parties.

$$
\begin{aligned}
K_a &=  \hash{\hash{K_a' \cat K_b'} \cat K_a' } * K_a' \\\\
k_a &=  \hash{\hash{K_a' \cat K_b'} \cat K_a' } * k_a' \\\\
K_b &=  \hash{\hash{K_a' \cat K_b'} \cat K_b' } * K_b' \\\\
k_b &=  \hash{\hash{K_a' \cat K_b'} \cat K_b' } * k_b' \\\\
\end{aligned}
\tag{1}
$$

The Monero key parts for Alice and Bob is constructed as follows:


$$
\begin{aligned}
x_a &=  \hash{\hash{X_a' \cat X_b'} \cat X_a' } * x_a' \\\\
x_b &=  \hash{\hash{X_a' \cat X_b'} \cat X_b' } * x_b' \\\\
x &= x_a + x_b \\\\
\end{aligned}
\tag{2}
$$


### Commitment phase

This phase allows Alice and Bob to commit to the use of their keys. This phase requires more than one round to complete
as some of the information that needs to be committed to is dependent on previous knowledge. 

Alice needs to provide Bob the following:

* Script key  \\( \\k_{Sa}\\)
* Monero public key:  \\( X_a'\\)

Bob needs to provide Alice the following:

* Script key  \\( \\k_{Sb}\\)
* Monero public key:  \\( X_b'\\)


### XTR payment

Alice will construct the Tari UTXO and publish this to the blockchain, knowing that she can reclaim her Tari if Bob vanishes
or tried to break the agreement.


### XMR Payment

If Bob cab see that Alice has published the Tari UTXO with the correct script, Bob can go ahead and publish the Monero UTXO
with the aggregate key \\(X = X_a + X_b \\).

### Claim XTR 

If Alice can see that Bob published the Monero UTXO to the correct aggregate key \\(X\\). She does not yet have the required
key \\(x_b \\) to claim the Monero. 
But she can now provide Bob with the correct pre_image to spend the Tari UTXO.

Bob can now supply the pre_image, and he has to give his Monero private key to the transaction to unlock the script.

### Claim XMR

Alice can now see that Bob spent the Tari UTXO, and by looking at the input_data required to spend the script, she can learn
Bob's secret Monero key. Although this key is public, her part of the Monero spend key is still private, and thus only she
knows the complete Monero spend key. She can use this knowledge to claim the Monero UTXO.

### The refund

If something goes wrong and Bob never publishes the Monero, or he disappears. Alice needs to wait for the lock height
`height_1` to pass. This will allow her to reclaim her Tari. But in doing so, she needs to publish her Monero
secret key as input to the TariScript to unlock the Tari. In doing so, when Bob comes back online, he can use
this knowledge to reclaim his Monero as only he now knows both parts of the Monero UTXO spend key.


### The lapse transaction

If something goes wrong and Alice never gives Bob the preimage, or she disappears. Bob needs to wait for the lock height
`height_2` to pass. This will allow him to create claim the Tari he wanted all along. But in doing so, he needs to publish
his Monero secret key as input to the TariScript to unlock the Tari. In doing so, when Alice comes back online,
he can use this knowledge to claim the Monero she wanted all along as only she now knows both parts of the Monero UTXO spend key.

## Notation

Where possible, the "usual" notation is used to denote terms commonly found in cryptocurrency literature. Lower case 
characters are used as private keys, while uppercase characters are used as public keys. New terms introduced here are 
assigned greek lowercase letters in most cases. Some terms used here are noted down in [TariScript]. 

| Name               | Symbol             | Definition |
|:-------------------|--------------------| -----------|
| Monero key         | \\( X \\)        | Alice's partial  Monero public key  |
| Alice's Monero key | \\( X_a \\)      | Alice's partial  Monero public key  |
| Bob's Monero key   | \\( X_b \\)      | Bob's partial  Monero public key  |
| Script key         | \\( K_s \\)      | The [script key] of the utxo |

[HTLC]: Glossary.md#hashed-time-locked-contract
[Mempool]: Glossary.md#mempool
[Mimblewimble]: Glossary.md#mimblewimble
[TariScript]: Glossary.md#tariscript
[TariScript]: Glossary.md#tariscript
[script key]: Glossary.md#script-keypair
[sender offset key]: Glossary.md#sender-offset-keypair
[script offset]: Glossary.md#script-offset
