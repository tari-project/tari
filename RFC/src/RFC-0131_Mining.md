# RFC-0131/Mining

## Full-node Mining on Tari Base Layer

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Hansie Odendaal](https://github.com/hansieodendaal), [Philip Robinson](https://github.com/philipr-za)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

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

This document describes the final proof-of-work strategy proposal for Tari main net.

## Related Requests for Comment

* [RFC-0100: Base Layer](RFC-0100_BaseLayer.md)
* [RFC-0110: Base Nodes](RFC-0110_BaseNodes.md)

This RFC replaces and deprecates [RFC-0130: Mining](RFCD-0130_Mining.md)

## Description

The following proposal draws from many of the key points of debate from the Tari community on the topic of Tari’s
main chain proof of work strategy. The early working assumption was that Tari would be 100% merged mined by Monero.

It would be nice to have a single, merge mined Proof of Work (PoW) algorithm, but the risks of hash rate attacks are 
real and meaningful. Double-spends and altering history can happen with >50% hash power, while selfish mining and 
eclipse attacks can happen with >33% hash power for a poorly connected attacker and >25% for a well-connected attacker 
(see [_Merged Mining: Analysis of Effects and Implications_](http://repositum.tuwien.ac.at/obvutwhs/download/pdf/2315652)).
Any non-merge mined PoW algorithm that is currently employed is even more vulnerable, especially if one can simply buy
hash rate on platforms like NiceHash.

Hybrid mining is a strategy that apportions blocks across multiple PoW algorithms. If the hybrid algorithms are
independent, then one can get at most x% of the total hash rate, where x is the fraction of blocks apportioned to that
algorithm. As a result, the threat of a double-spend or selfish mining attack is mitigated, and in some cases
eliminated.

This proposal puts forward Hybrid mining as the Tari PoW algorithm. However, some details still needed to be decided:

* the number of algorithms;
* the choice of algorithms;
* the block distribution;
* the difficulty adjustment strategy.

### The number of algorithms

In hybrid mining, "independence" of algorithms is key. If the same mining hardware can be used on multiple PoW
algorithms in the hybrid mining scheme, you may as well not bother with hybrid mining, because miners can simply
switch between them.

In practice, no set of algorithms are truly independent. The best we can do is try to choose algorithms that work best
on CPUs, GPUs, and ASICs. In truth, the distinction between GPUs and ASICs is only a matter of time. Any "GPU-friendly"
algorithm is ASIC-friendly too; it's just a case of whether the capital outlay for fabricating them is worth it; and
this will eventually become true for any algorithm that supplies PoW for a growing market cap. Employing merged mining
with major players that use independent hardware introduces another degree of freedom, as long as those are independent,
like RandomX with Monero, SHA-256 with Bitcoin and Scrypt with Litecoin.

_**Note:** Merge mining does not add security per se, but it does add plenty of hash rate and continuity of the blockchain._

So really the answer to how many algorithms is: More than one, as independent as possible.

### The choice of algorithms

A good technical choice would be merge mining with Monero, Bitcoin and Litecoin, if enough interest could be
attracted from those mining communities. However, that would rule out any participation from Tari supporters and
enthusiasts, at least in the early stages. So, to be inclusive of Tari supporters and enthusiasts, merge mining RandomX
with Monero and another GPU/ASIC-friendly algorithm, like SHA3 also known as Keccak, is proposed. Using a custom
configuration of such a simple and well understood algorithm means there is a low likelihood of unforeseen optimizations
that will give a single miner a huge advantage. It also means that it stands a good chance of being "commoditized" when 
ASICs are eventually manufactured. This means that SHA3 ASICs will be widely available and not available from only a 
single supplier.

_**Edit:** Handshake, which launched a few months ago, selected a Hashcash PoW algorithm
[(see #Consensus)](https://handshake.org/files/handshake.txt) using SHA3 and Blake2B for many of the same reasons:
SHA3 is currently under-represented in PoW; SHA3 usage in combination with Blake2B in PoW creates a more level playing
field for hardware manufacturers._

### The block distribution

To reduce the chance of hash rate attacks, an even 50/50 distribution is needed, as discussed earlier. However,
sufficient buy-in is needed, especially with regards to merge mining RandomX with Monero. To make it worthwhile for a
Monero pool operator to merge mine Tari, but still guard against hash rate attacks and to be inclusive of independent
Tari supporters and enthusiasts, a 60/40 split is proposed in favour of merge mining RandomX with Monero. The
approaching [Monero tail emission](https://web.getmonero.org/resources/moneropedia/tail-emission.html) at the end of May
2022 should also make this a worthwhile proposal for Monero pool operators.

### The difficulty adjustment strategy

The choice of difficulty adjustment algorithm is important. In typical hybrid mining strategies, each algorithm operates
completely independently with a scaled-up target block time and is the most likely approach that any blockchain will
take. Tari testnet has been running very successfully on Linear Weighted Moving Average (LWMA) from Bitcoin & Zcash
Clones [version 2018-11-27](https://github.com/zawy12/difficulty-algorithms/issues/3#issuecomment-442129791). This LWMA
difficulty adjustment algorithm has also been
[tested in simulations](https://github.com/tari-labs/modelling/tree/master/scenarios/multi_pow_01) and it proved to be a
good choice in the multi-PoW scene as well.

### Final proposal, hybrid mining details

The final proposal is summarized below:

- 2x mining algorithms, with average combined target block time at 120 s, to match Monero's block interval
- LWMA version 2018-11-27 difficulty algorithm adjustment for both with difficulty algo window of 90 blocks
- Algorithm 1: Monero merged mining
  - at ~60% blocks distribution, based on block time setting of 192.0
  - using RandomX, with `seed_hash` as arbitrary data, re-use restricted by age measured in Tari blocks
- Algorithm 2: Independent mining
  - at ~40% blocks distribution, based on block time setting of 288.0 s
  - SHA3-based algorithm, details to be fleshed out