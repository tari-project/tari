# RFC-0203/Stealth addresses

## Stealth addresses

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Philip Robinson](https://github.com/philipr-za)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2022 The Tari Development Community

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

This Request for Comment (RFC) presents the implementation of Dual-Key Stealth Addresses in One-Sided payments to improve
privacy for receivers of these payments on the Tari base layer.

## Related Requests for Comment

- [RFC-0201: TariScript](RFC-0201_TariScript.md)

## Introduction

The Tari protocol extends the [Mimblewimble] protocol to include scripting in the form of [TariScript]. One of the 
first features implemented using [TariScript] was [one-sided payments]. These are payments to a recipient that do not 
require an interactive negotiation in the same way a standard [Mimblewimble] transaction does. One of the main downsides
of the current implementation of [one-sided payments] is that the script key used is the Public Key of the recipient's 
wallet. This public key is embedded in the [TariScript] of the [UTXO] created by the sender. The issue is that it becomes
very easy for a third party to scan the blockchain to look for one-sided transaction outputs being sent to a given wallet. 
In order to alleviate this privacy leak, this RFC proposes the use of Dual-Key Stealth Addresses to be used as the script
key when sending a one-sided payment.

## Brief background on the development of Stealth Addresses

Stealth addresses were first proposed on the Bitcoin Talk forum by user [Bytecoin]. The concept was further refined in 
the [Cryptonote] whitepaper and by [Peter Todd] which went on to be used in Monero. These formulations were very similar 
to the [BIP-32] style of address generation. Later in 2014 a developer called rynomster/sdcoin proposed a further 
improvement to the scheme that he called the Dual-Key Stealth Address Protocol (DKSAP) that allowed for a separate 
scanning key and spending key. Since then there have been many variations of DKSAP proposed, but generally they only 
offer performance optimizations for certain scenarios or add an application-specific feature. For our application, DKSAP 
will do the job.

## Dual-key Stealth Addresses

The Dual-key Stealth Address Protocol (DKSAP) uses two key-pairs for the recipient of a [one-sided payment], 
\\( A = a \cdot G \\) and \\( B = b \cdot G \\). Where \\( a \\) is called the scan key and \\( b \\) is the spend key.
A recipient will distribute the public keys out of band to receive [one-sided payments].

The protocol that a sender will use to make a payment to the recipient is as follows:
1. Sender generates a random nonce key-pair \\( R = r \cdot G \\).
2. Sender calculates a ECDH shared secret \\(c = H( r \cdot a \cdot G ) = H( a \cdot R) = H( r \cdot A) \\), where
\\( H( \cdot ) \\) is a cryptographic hash function.
3. The sender will then use \\( K_S = c \cdot G + B \\) as the last public key in the [one-sided payment] script. 
4. The sender includes  \\( R \\) for the receiver but dropping it as it is not required during script execution. 
This changes the script for a [one-sided payment] from `PushPubkey(K_S)` to `PushPubkey(R) Drop PushPubkey(K_S)`.

The recipient will need to scan the blockchain for outputs that contain scripts of the [one-sided payment] form, and when
one is found they will need to do the following:
1. Extract the nonce \\( R \\) from the script.
2. Use the public nonce to calculate the shared secret \\(c = H( a \cdot R) \\)
3. Calculate \\( K_S \\) and check if it exists in the script.
4. If it exists, the recipient can produce the script signature required using the private key calculated by \\( c + b \\). 
This private key can only be computed by the recipient using \\( b \\).

One of the benefits of the DKSAP is that the key used for scanning the blockchain, \\( a \\), does not enable one to
calculate the private key required for spending the output. This means that a recipient can potentially outsource the
scanning of the blockchain to a less trusted third-party by giving them just the scanning key \\( a \\) but retaining the
secrecy of the spend key \\( b \\).

[tariscript]: ./Glossary.md#tariscript
[mimblewimble]: ./Glossary.md#mimblewimble
[one-sided payments]: ./RFC-0201_TariScript.md#one-sided-payment
[one-sided payment]: ./RFC-0201_TariScript.md#one-sided-payment
[utxo]: ./Glossary.md#unspent-transaction-outputs
[bytecoin]: https://bitcointalk.org/index.php?topic=5965.0
[Cryptonote]: https://cryptonote.org/whitepaper.pdf
[Peter Todd]: https://www.mail-archive.com/bitcoin-development@lists.sourceforge.net/msg03613.html
[BIP-32]: https://en.bitcoin.it/wiki/BIP_0032