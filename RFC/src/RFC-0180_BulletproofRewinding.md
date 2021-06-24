# RFC-0180: Bulletproof range proof rewinding

## Bulletproof range proof rewinding

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Hansie Odendaal](https://github.com/hansieodendaal)

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

This Request for Comment (RFC) presents a proposal for Bulletproof range proof rewinding in the Tari blockchain to 
enable advanced usages like wallet recovery and one&#8209;sided payments.

## Related Requests for Comment

* [RFC-0150: Wallets](RFC-0150_Wallets.md)

## Introduction

We use `dalek-cryptography/bulletproofs` in the Tari project and have a need to do wallet recovery from seed values and 
also to recover the value in the value commitment from the Unspent Transaction Output (UTXO). Pull requests 
[PR#340](https://github.com/dalek-cryptography/bulletproofs/pull/340) for the `dalek-cryptography/bulletproofs` crate 
and [PR#6](https://github.com/zkcrypto/bulletproofs/pull/6) for the `zkcrypto/bulletproofs` crate were submitted to add 
Bulletproofs rewinding functionality to the Bulletproofs crate as a user option.

The methodology presented here is closely modelled on Grin's solution 
[as discussed here](https://github.com/mimblewimble/grin-wallet/issues/105), but using two private keys instead of one.

## Rewind Scheme

Bulletproofs per say are not be discussed in this RFC, only how the rewinding scheme works. Readers who require 
background information on Bulletproofs can read the excellent documentation created by the Dalek team 
[here](https://doc-internal.dalek.rs/bulletproofs/index.html). Important to note is that Dalek only implemented 
the aggregated Multiparty Computation Protocol (MCP) for range proofs and that proving a single range proof is handled a 
special case.

### Constructing a rewindable Bulletproof range proof

Our scheme is discussed with reference to the 
[Party and Dealer's algorithm](https://doc-internal.dalek.rs/bulletproofs/range_proof/index.html#party-and-dealers-algorithm)
and using notation defined [here](https://doc-internal.dalek.rs/bulletproofs/notes/index.html#notation).

In this scheme three additional parameters are introduced when creating a range proof for a Pedersen commitment 
(termed _value commitment_ by Dalek because it is a commitment to the value of the token):

- Private rewind key:&nbsp;&nbsp; $ r\_{key} $
- Private blinding key:&nbsp;&nbsp; $ b\_{key} $
- Twenty three (23) bytes proof message:&nbsp;&nbsp; $ p\_{msg} $.

The 23 bytes worth of proof message can be any message a user wants to embed within the proof. Internally the two 
private keys, in combination with the value commitment, are converted into two rewind nonces and two blinding nonces:


- Rewind nonce 1:&nbsp;&nbsp; $ r\_{n1} = \text{H}( \ \text{H}(r\_{key} \cdot \widetilde{B}) \ || \ V\_{(j)} \ ) $
- Rewind nonce 2:&nbsp;&nbsp; $ r\_{n2} = \text{H}( \ \text{H}(b\_{key} \cdot \widetilde{B}) \ || \ V\_{(j)} \ ) $
- Blinding nonce 1:&nbsp;&nbsp; $ b\_{n1} = \text{H}( \ \text{H}(r\_{key}) \ || \ V\_{(j)} \ ) $
- Blinding nonce 2:&nbsp;&nbsp; $ b\_{n2} = \text{H}( \ \text{H}(b\_{key}) \ || \ V\_{(j)} \ ) $


These four values are seen as nonces due to the fact that each value commitment is unique, whereas the $ r\_{key} $ 
and $ b\_{key} $ can be used over and over without leaking any information. 

The value $ v\_{(j)} $ is an 8 byte word, and $ p\_{msg} $ is a 23 byte word. The bytes of these two words can be 
concatenated to form a 32 byte word and when XORed with $ r\_{n2} $&nbsp;,&nbsp; it can be used to embed the value and proof message. $ r\_{n2} $ is modified as follows:

$$
\begin{aligned}
r^\backprime\_{n2}  = r\_{n2} \ \mathbin{\oplus} \ (v\_{(j)\_{\ bytes \ 1..8}}  \ || \ p\_{msg\_{\ bytes \ 9..31}} )
\end{aligned}
\tag{1}
$$

Consider the start of the protocol where each party $ j $ computes three commitments: to the value $ v\_{(j)} $, to the 
bits of that value $ \mathbf{a}\_{L, (j)}, \mathbf{a}\_{R, (j)} $, and to the per-bit blinding factors 
$ \mathbf{s}\_{L, (j)}, \mathbf{s}\_{R, (j)} $:

$$
\begin{aligned}
V\_{(j)} &\gets \operatorname{Com}(v\_{(j)}, {\widetilde{v}\_{(j)}})               && = v\_{(j)} \cdot B + {\widetilde{v}\_{(j)}} \cdot {\widetilde{B}} \\\\
A\_{(j)} &\gets \operatorname{Com}({\mathbf{a}}\_{L, (j)}, {\mathbf{a}}\_{R, (j)}) && = {\langle {\mathbf{a}}\_{L, (j)}, {\mathbf{G}\_{(j)}} \rangle} + {\langle {\mathbf{a}}\_{R, (j)}, {\mathbf{H}\_{(j)}} \rangle} + {\widetilde{a}\_{(j)}} {\widetilde{B}} \\\\
S\_{(j)} &\gets \operatorname{Com}({\mathbf{s}}\_{L, (j)}, {\mathbf{s}}\_{R, (j)}) && = {\langle {\mathbf{s}}\_{L, (j)}, {\mathbf{G}\_{(j)}} \rangle} + {\langle {\mathbf{s}}\_{R, (j)}, {\mathbf{H}\_{(j)}} \rangle} + {\widetilde{s}\_{(j)}} {\widetilde{B}} \\\\
\end{aligned}
\tag{2}
$$

where $ \widetilde{v}\_{(j)}, \widetilde{a}\_{(j)}, \widetilde{s}\_{(j)} $ are sampled randomly from $ {\mathbb Z\_p} $. 
(Note that $ \widetilde{v}\_{(j)} $ is the blinding factor of the value commitment.)

In our scheme:
- blinding factor $ {\widetilde{a}\_{(j)}} $ is replaced by $ r\_{n1} $
- blinding factor $ {\widetilde{s}\_{(j)}} $ is replaced by $ r^\backprime\_{n2} $

Consider where the party commits to the terms $ t\_{1, (j)}, t\_{2, (j)} $:

$$
\begin{aligned}
T\_{1, (j)} &\gets \operatorname{Com}(t\_{1, (j)}, {\tilde{t}\_{(j1}})  && = t\_{1, (j)} \cdot B + {\tilde{t}\_{1, (j)}} \cdot {\widetilde{B}} \\\\
T\_{2, (j)} &\gets \operatorname{Com}(t\_{2, (j)}, {\tilde{t}\_{2, (j)}})  && = t\_{2, (j)} \cdot B + {\tilde{t}\_{2, (j)}} \cdot {\widetilde{B}}
\end{aligned}
\tag{3}
$$

where $ \tilde{t}\_{1, (j)}, \tilde{t}\_{2, (j)} $ are sampled randomly from $ {\mathbb Z\_p} $.

In our scheme:
1. blinding factor $ \tilde{t}\_{1, (j)} $ is replaced by $ b\_{n1} $
1. blinding factor $ \tilde{t}\_{2, (j)} $ is replaced by $ b\_{n2} $

The synthetic blinding factors calculation below is key, as it will be used to extract the data when playing the 
Bulletproof in reverse:

$$
\begin{aligned}
  {\tilde{t}}\_{(j)}(x) &\gets z^{2} {\tilde{v}}\_{(j)} + x {\tilde{t}}\_{1, (j)} + x^{2} {\tilde{t}}\_{2, (j)} \\\\
\end{aligned}
\tag{4}
$$

$$
\begin{aligned}
  \tilde{e}\_{(j)}     &\gets {\widetilde{a}}\_{(j)}   + x {\widetilde{s}}\_{(j)}
\end{aligned}
\tag{5}
$$

In the end, the complete range proof consists of these elements:

$$
\begin{aligned}
   \lbrace A, S, T_1, T_2, t(x), {\tilde{t}}(x), \tilde{e}, L_k, R_k, \\dots, L_1, R_1, a, b \rbrace 
\end{aligned}
\tag{6}
$$

**Note:** This scheme has been improved in what has been presented in by 
[Grin](https://github.com/mimblewimble/grin-wallet/issues/105) after being commented on by Dalek, by not using the same 
rewind nonce for $ {\widetilde{a}\_{(j)}} $ and $ {\widetilde{s}\_{(j)}} $ nor the same blinding nonce for 
$ \tilde{t}\_{1, (j)} $ and $ \tilde{t}\_{2, (j)} $.

### Extracting data

Note the presence of $ {\tilde{t}}\_{(j)} $ and $ \tilde{e} $ in (6). The Dalek Bulletproofs are constructed using 
[Merlin Transcripts](https://doc-internal.dalek.rs/merlin/index.html) to automate the Fiat-Shamir transform, so that 
non-interactive protocols can be implemented as if they were interactive. The prover adds each step of the Bulletproof 
range proof creation to the protocol transcript, so the verifier has to do the same.

The extraction procress starts by adding the values $ A $ and $ S $ are to the protocol transcript to obtain challenge 
scalars $ z $ and $ x $ from the transcript.

There after, $ {\widetilde{s}\_{(j)}} $ is extracted from (5) by replacing $ {\widetilde{a}\_{(j)}} $ with $ r\_{n1} $ &nbsp;:

$$
\begin{aligned}
  {\widetilde{s}}\_{(j)} = ( \tilde{e}\_{(j)} - r\_{n1} ) \cdot \frac{1}x 
\end{aligned}
\tag{7}
$$

Next, the value and proof message are extracted from $ {\widetilde{s}\_{(j)}} $ when XORed with $ r\_{n2} $&nbsp;:

$$
\begin{aligned}
  v\_{(j)} &= ( r\_{n2} \ \mathbin{\oplus} \ {\widetilde{s}}\_{(j)} ) | \_{\ bytes \ 1..8} \\\\
  p\_{msg} &= ( r\_{n2} \ \mathbin{\oplus} \ {\widetilde{s}}\_{(j)} ) | \_{\ bytes \ 9..31} 
\end{aligned}
\tag{8}
$$

Finally, the blinding factor is extracted from (4):

$$
\begin{aligned}
  \widetilde{v} = \frac{1}{z^2} \cdot ( {\tilde{t}}(x) - x \cdot \tilde{t}\_{1, (j)} - x^2 \cdot \tilde{t}\_{2, (j)} )
\end{aligned}
\tag{9}
$$


## Some notes on usage and use cases

Rewinding a Bulletproof can take place according to one or both of these steps:
- **Peak value only:** Using this step returns the value and proof message only, but _returning garbage data if 
  the wrong rewind nonces are provided_, or,
- **Rewind fully:** Using this step returns the value, blinding factor and proof message, _returning an error if 
  the wrong rewind and blinding nonces are provided_. Note that this step is independent from peaking the value only, 
  thus do not have ot be preceded by it. If many range proofs need to be scanned to uncover those that belong to a 
  particuler wallet, peaking the value only before fully rewinfing it will provide a performance benefit.

The main use case has to do with wallet recovery. A user would normally have a backup of their unique wallet seed 
words somewhere, but could more easily lose their entire wallet without having made any backups or only having old 
backups. If a wallet can derive one or more sets of private keys from the seed words and use them in every UTXO 
construction as proposed, it can enable wallet recovery using Bulletproof rewinding.

A secondary use case would be for trusted 3rd parties to identify spending, by only having access to the public rewind 
key and the embedded proof message. The public rewind keys can be shared with a 3rd party out of band. The owner and/or 
delegated 3rd party can then use these keys in conjunction with a specific value commitment to calculate candidate 
rewind nonces for its proof. The returned proof message from the _peak value only_ rewind step can be used to narrow 
down the probability that the particular proof belongs to a specific collection. In this mode the owner alone will be 
able to use both sets of pub-pvt key pairs in conjunction with a specific value commitment to calculate candidate rewind 
and blinding nonces for its proof. The _rewind fully_ step will reveal the details of the value commitment and proof 
message if successful.

The use for this protocol, as opposed to simply revealing the original value along with the blinding factor to whoever 
wants the plain value, is to protect the UTXO. In Mimblewimble, if the value commitment can be opened, it can be spent 
without the owners knowledge.

The proof message is private or can be shared with a trusted 3rd party in the same way one would share the public 
rewind keys, but not common public knowledge. It is totally arbitrary, but known data, to enable identifying beyond a 
doubt if the returned value $ v\_{(j)} $ is from a specific collection of value commitments $ V\_{(j)} $.

## Implementation

Using the Application Programmers Interface (API) it is possible to:
- create a rewindable Zero-knowledge (ZK) proof with up to 23 bytes of additional embedded proof message  $ p\_{msg} $ 
  &nbsp;;
- extract the value $ v\_{(j)} $ and 23 bytes proof messsage $ p\_{msg} $ only;
- extract the value $ v\_{(j)} $ &nbsp;, &nbsp; blinding factor $ \widetilde{v} $ and 23 bytes proof messsage $ p\_{msg} $ 
  &nbsp;.

## Credits

- [@jaspervdm](https://github.com/jaspervdm) for his improved bulletproof rewind scheme, used as precurser.
- [@cathieyun](https://github.com/cathieyun) for provifing valuable feedback to improve this scheme.