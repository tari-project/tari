# RFC-8001/MultiPartyTransactions

## Time related transactions

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [SW van heerden](https://github.com/SWvheerden)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019 The Tari Development Community

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

This document describes a few extension to [MimbleWimble](MimbleWimble) to allow multi-party [UTXOs](utxo).

## Related RFCs

## Description

#### Multi Party UTXO

Normal [MimbleWimble] does not have the concept of a [multisig] UTXO. The UTXO is a commitment `C(v,r) = r·G + v·H` with
the value blinded. However, the blinding factor `r` can be composed of multiple blinding factors where `r = r1 + r2 + ... + rn`, as Pedersen commitments are linear.

The output commitment can then be constructed as `C(v,r) = r1·G + r2·G + ... + rn·G + v·H = (r1 + r2 + ... + rn)·G + v·H`.
This can be exploited for multiple users where each participant has their own `ri` and keeps their private blinding factor
hidden and only provides their public blinding factor.

The base layer is oblivious as to how the commitment and related signature were constructed.
To open such commitments (in order to spend it) only the n-of-n blinding factor `r` is required, and not the original
aggregated signature that was used to sign the transaction. The parties that wants to open the commitment needs to
collaborate to produce the n-of-n blinding factor `r`.

[mimblewimble]: Glossary.md#mimblewimble
[UTXO]: Glossary.md#unspent-transaction-outputs
[multisig]: Glossary.md#multisig

