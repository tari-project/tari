# RFC-0240/Atomic Swap

## Time-related Transactions

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

The aim of this Request for Comment (RFC) is to describe how an Atomic swap between Tari and Monero will be created.

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

Doing atomic swaps with Monero is a bit more complicated and requires a crypto dance to complete as Monero does not
implement any form of HTLC's or the like. This means that when doing an atomic swap with Monero most of the logic will
have to implemented in the Tari side. Atomic swaps between Monero and bitcoin have been implemented by the [Farcaster  project](https://github.com/farcaster-project/RFCs)
and the [commit team](https://github.com/comit-network/xmr-btc-swap). Due to the way that TariScript works we have a few advantages over bitcoin script when it comes to [adaptor signatures](https://tlu.tarilabs.com/cryptography/introduction-to-scriptless-scripts#adaptor-signatures) as the [script key] was spesifically designed with [scriptless scripts](https://tlu.tarilabs.com/cryptography/introduction-to-scriptless-scripts).

### Method

The basic happy outline of a Tari - Monero atomic swap is described here and more detail will follow. We will assume here that Alice wants to trade her XTR for Bob's XMR.

* Negotiation - Here both parties negotiate about the values, and how the Monero and Tari Utxo's will look
* Commitment - Here both parties commit to their use of keys and Bob commits to the refund transaction
* XTR payment - Here the XTR payment is made to a multi party UTXO containing a script
* XMR Payment - Here the Monero payment is made to a multiparty [scriptless script](https://tlu.tarilabs.com/cryptography/introduction-to-scriptless-scripts) utxo.
* Claim XTR - Here the XTR is claim and in claiming it, with the help of an , the XMR private key is revealed
* Claim XMR - Here the XMR is claimed with the revealed key.

Please take note of the notation used in [TariScript] and specifically notation used on the signatures on the [transaction inputs](RFC-0201_TariScript.md##transaction-input-changes) , other notation will be noted in the [Notation](#notation) section.


### TL;DR

This whole scheme resolves around Alice who wants to exchange some of her Tari for some of Bob's Monero. They both commit 
to do the exchange and while also not trusting each other. And if something goes wrong here we want to refund Alice's Tari
while also refund Bob's Monero. 

How this works is that Alice and Bob create a shared output on both chains. The monero output is a simple aggregate key
to unlock the UTXO, while the Tari UTXO is unlocked by either one of two aggregate keys. The unlock key on Tari is
determined by the block height. 

The Tari aggregate keys are constructed in such a way that the swap transaction's signature will reveal Bob's Monero key
so that Alice has both keys and this allows her to claim the Monero. While the refund transaction's signature will reveal
Alice's Monero key so that Bob has both keys and he claim the Monero.

In order to ensure that we can always claim the refund in case Bob disappears after Alice posts the Tari UTXO, we need to
ensure that this refund transaction is completed and signed by both Alice and Bob before Alice published the Tari UTXO.
This ensures that in the case that Bob disappears Alice can reclaim her Tari. And if Bob reappears he can reclaim his Monero.

### TariScript

The Script used for the Tari UTXO is as follows:
``` TariScript,ignore
   CheckHeight(height)
   LtZero
   IFTHEN
      PushPubkey(K_{Ss})
   Else
      PushPubkey(K_{Sr})
   ENDIF
```

### Negotiation

Alice and Bob have to negotiate about the rate of exchange, and the amount to be exchanged in the atomic swap. 
They also need to decide how the two UTXO's will look on the block chain. In order to do so the following needs to be finalized:

* Amount of Tari to swap for the amount of Monero
* Monero public key parts \\(X_a\\), \\(X_b\\) and its aggregate form \\(X\\)
* Tari [script key] parts \\(K_Ssa\\), \\(K_Ssb\\) and its aggregate form \\(K_Ss\\)
* Tari [script key] parts \\(K_Sra\\), \\(K_Srb\\) and its aggregate form \\(K_Sr\\)
* Tari [script offset key] parts  \\(K_Osa\\), \\(K_Osb\\) and its aggregate form \\(K_Os\\)
* Tari [script offset key] parts  \\(K_Ora\\), \\(K_Orb\\) and its aggregate form \\(K_Or\\)
* All of the nonces used in the script signature creation and Metadata signature for both the swap and refund transactions
* The [script offset] used in both the swap and refund transactions
* The [TariScript] to be used in the Tari UTXO
* The blinding factor \\(k_i\\) for the Tari UTXO, this can be a DiffieHellman between their addresses.


### Commitment phase

This phase allows Alice and Bob to commit to the use of their keys. 
Alice needs to provide bob the following:

* Adaptor signature \\(b'_{Sra}\\) for \\(b_{Sra}\\)
* Signature \\(a_{Sra}\\)
* Partial [script key] for both transactions \\(K_Ssa\\) and \\(K_Sra\\)
* Partial [script offset] for both transactions \\(K_Osa\\), \\(K_Ora\\)
* Public Nonce \\(R_{Ssa}\\) and \\(R_{Sra}\\) for her script_signature
* Partial Monero public key \\(X_a\\)
* All of the rest of the challenge information in e_s for the script signature s_{Ssa}
* All of the rest of the challenge information in e_r for the script signature s_{Sra}

Alice constructs for the happy path \\(a_{Ssa}\\) and \\(b_{Ssa}\\) with
$$
\begin{aligned}
a_{Ssa} &= r_{Ssa_a} \\\\
b_{Ssa} &= r_{Ssa_b} +  e_{s}(k_{Ssa}) \\\\
e_s &= \hash{ (R_{Ss} + X_b) \cat \alpha_i \cat \input_i \cat (K_{Ssa} + K_{Ssb}) \cat C_i} \\\\
R_{Ss} &= r_{Ssa_a} \cdot H + r_{Ssa_b} \cdot G + r_{Ssb_a} \cdot H + r_{Ssb_b} \cdot G \\\\
\end{aligned}
\tag{1}
$$
But Alice keeps \\(a_{Ssa}\\) and \\(b_{Ssa}\\) to herself for now.

Alice also constructs for the refund path  \\(a_{Sra}\\) and \\(b'_{Sra}\\) with
$$
\begin{aligned}
a_{Sra} &= r_{Sra_a} +  e(v_{i}) \\\\
b'_{Sra} &= r_{Sra_b} +  e(k_{Sra}+k_i) \\\\
e_r &= \hash{ (R_{Sr} + (X_a)) \cat \alpha_i \cat \input_i \cat (K_{Sra} + K_{Srb}) \cat C_i} \\\\
R_{Sr} &= r_{Sra_a} \cdot H + r_{Sra_b} \cdot G + r_{Srb_a} \cdot H + r_{Srb_b} \cdot G \\\\
X_a = x_a \cdot G \\\\
\end{aligned}
\tag{2}
$$

Bob can now verify Alice's adaptor signature with
$$
\begin{aligned}
a_{Sra} \cdot H + b_{Sra} \cdot G = R_{Sra} + (C_i+K_{Sra})*e_r \\\\
\end{aligned}
\tag{2}
$$


Bob also constructs for the Happy path  \\(a_{Ssb}\\) and \\(b'_{Ssb}\\) with
$$
\begin{aligned}
a_{Ssb} &= r_{Ssb_a} +  e(v_{i}) \\\\
b'_{Ssb} &= r_{Ssb_b} +  e(k_{Ssb}+k_i) \\\\
e_s &= \hash{ (R_{Sr} + (X_b)) \cat \alpha_i \cat \input_i \cat (K_{Ssa} + K_{Ssb}) \cat C_i} \\\\
R_{Ss} &= r_{Ssa_a} \cdot H + r_{Ssa_b} \cdot G + r_{Ssb_a} \cdot H + r_{Ssb_b} \cdot G \\\\
X_b = x_b \cdot G \\\\
\end{aligned}
\tag{2}
$$

Bob constructs for the refund path \\(a_{Srb}\\) and \\(b_{Srb}\\) with
$$
\begin{aligned}
a_{Srb} &= r_{Srb_a} \\\\
b_{Srb} &= r_{Srb_b} +  e_{s}(k_{Srb}) \\\\
e_r &= \hash{ (R_{Ss} + X_a) \cat \alpha_i \cat \input_i \cat (K_{Sra} + K_{Srb}) \cat C_i} \\\\
R_{Ss} &= r_{Sra_a} \cdot H + r_{Sra_b} \cdot G + r_{Srb_a} \cdot H + r_{Srb_b} \cdot G \\\\
\end{aligned}
\tag{1}
$$

Alice can now verify Bob's adaptor signature with
$$
\begin{aligned}
a_{Ssb} \cdot H + b_{Ssb} \cdot G = R_{Ssb} + (C_i+K_{Ssb})*e_s \\\\
\end{aligned}
\tag{2}
$$

### XTR payment

If Alice and Bob is happy with all the committed values up to know. Alice will create a Tari UTXO with the script mentioned above. 
And because Bob already gave her the required signatures \\(a_{Srb}\\), \\(b_{Srb}\\) and \\(R_{Srb}\\) of the his part 
of the aggregated signatures, she knows all the required information to spend this after the lock expired. 

### XMR Payment

If Bob cab see that Alice has published the Tari UTXO with the correct script, Bob can go ahead and publish the Monero UTXO
with the aggregate key \\(X = X_a + X_b \\).

### Claim XTR 

If Alice can see that Bob published the Monero UTXO to the correct aggregate key \\(X\\). She does not yet have the required
key \\(x_b \\) to claim the Monero. But she can now provide Bob with: \\(a_{Ssa}\\), \\(b_{Ssa}\\) and \\(R_{Ssa}\\).
This will allow Bob to claim the Tari. But when Bob spends the Tari, he signs with \\(a_{Ssb}\\), \\(b_{Ssb}\\) and \\(R_{Ssb}\\)
as his part of the aggregate key. Thus revealing \\(x_b \\).



### Claim XMR

Because Alice now has \\(x_b \\), she is the only person who knows \\(x = x_a + x_b \\) the required aggregate private key
to claim the Monero. 

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
| Alice's Script key | \\( K_sa \\)     | Alice's partial [script key]  |
| Bob's Script key   | \\( K_sb \\)     | Bob's partial [script key]  |
| Alice's adaptor signature   | \\( b'_{Sa} \\)     | Alice's adaptor signature for the signature \\( b_{Sa} \\) of the script_signature of the utxo |
| Bob's adaptor signature   | \\( b'_{Sb} \\)     | Bob's adaptor signature for the \\( b_{Sb} \\) of the script_signature of the utxo |



[HTLC]: Glossary.md#hashed-time-locked-contract
[Mempool]: Glossary.md#mempool
[Mimblewimble]: Glossary.md#mimblewimble
[TariScript]: Glossary.md#tariscript
[TariScript]: Glossary.md#tariscript
[script key]: Glossary.md#script-keypair
[sender offset key]: Glossary.md#sender-offset-keypair
[script offset]: Glossary.md#script-offset
