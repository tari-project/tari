# RFC-0230/HTLC

## Hashed Time Locked Contract (HTLC)

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [SW van heerden](https://github.com/SWvheerden)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2018 The Tari Development Community

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", 
"NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in 
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as 
shown here.

## Disclaimer

The purpose of this document and its content is for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

This document describes how to implement hashed time locked contracts as an extension to [mimblewimble](MimbleWimble).

## Related RFCs
## Description
### Abstract
Hashed Time Locked Contracts are time locked contracts that only pays out after a certain criteria has been met or refunds the originator if a certain period has expired. 

#### Time Locked contracts
In [mimblewimble](MimbleWimble) timelocked contracts can be accomplished by modifying the kernel of each transaction to include a block height. This allows a limit to how early in the blockchain lifetime the specific transaction can be included in a block. 

This requires users constructing a transaction to:
* MUST include a lockheight in the kernal of their transaction,
* MUST include the lockheight in the signed message to stop tampering with the lockheight after the transaction was constructed.

This add the following requirement to a miner:
* MUST not add any transaction to the mined [block](block) that has not already exceeded its lockheight.

This also adds the following requirement to a [base node](base node):
* MUST reject any [block](block) that contains a kernel with a lockheight not already past the [currenthead](currenthead).

### Mimblewimble N-of-N Multisig UTXO

A normal Mimblewimble UTXO does not have a notion of being a [multisig] UTXO. The value of UTXO is hidden inside the commitment `C(v,r) = r·G + v·H` by virtue of being blinded. However, the blinding factor `r` can be composed of multiple blinding factors where `r = r1 + r2 + ... + rn`. The output commitment can then be constructed as `C(v,r) = r1·G + r2·G + ... + rn·G + v·H = (r1 + r2 + ... + rn)·G + v·H` where each participant keeps their private blinding factor hidden and only provides their public blinding factor. A multi-party aggregated signature scheme like Musig may be employed to sign such a transaction so that all parties' interests can be protected.

The base layer is oblivious as to how the commitment and related signature were constructed. To open such commitments (in order to spend it) only the n-of-n blinding factor `r` is required. The parties that wants to open the commitment needs to collaborate n-of-n blinding factor `r`.

#### Hashed Time Locked Contract
Unlike Bitcoin where this can be accomplished with a single transaction, in [MimbleWimble](MimbleWimble) this is a multi-step process to construct a timelocked contract. 

The steps are as follows:
* The sender MUST pay all the funds into a n-of-n [multisig](multisig) [utxo](UTXO).  
* All parties invlovled MUST construct a refund [transaction](transaction) paying back all funds to the sender, paying from the n-of-n [multisig](multisig) [utxo](UTXO). This [transaction](transaction) has a [transaction lock height](#Time Locked contracts) in the future and cannot be immediately mined. It therefor lives in the [mempool](mempool).
* The sender MUST publish both above [transactions](transaction) at the same time to ensure the receiver cannot hold him hostage. 
* The parties MUST construct a second [transaction](transaction) that pays the receiver the funds. This [transaction](transaction) typically makes use of a preimage to allow spending of the [transaction](transaction) if the user reveals some knowledge, allowing the user to unlock the [utxo](UTXO).

HTLC's in [mimblewimble](MimbleWimble) makes use of double spending the n-of-n [multisig](multisig) [utxo](UTXO) and the first valid published [transaction](transaction) can then be mined and claim the n-of-n [multisig](multisig) [utxo](UTXO). 

An example of a [HTLC](HTLC) in practice can be viewed at Tari University:
[Bitcoin atomic swaps](https://tlu.tarilabs.com/protocols/atomic-swaps/AtomicSwaps.html)
[MimbleWimble atomic swaps](https://tlu.tarilabs.com/protocols/grin-protocol-overview/MainReport.html#atomic-swaps)

[HTLC]: Glossary.md#Hashed-Time-Locked-Contract
[mempool]: Glossary.md#mempool
[mimblewimble]: Glossary.md#mimblewimble
[base node]: Glossary.md#base-node
[block]: Glossary.md#block
[currenthead]: Glossary.md#currenthead
[utxo]: Glossary.md#unspent-transaction-outputs
[multisig]: Glossary.md#multisig
[transaction]: Glossary.md#transaction


