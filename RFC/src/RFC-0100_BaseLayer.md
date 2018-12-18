# RFC-0100/BaseLayer

## The Tari Base Layer

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

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

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in [RFC 2119](http://tools.ietf.org/html/rfc2119).

## Disclaimer

The purpose of this document and its content is for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

This document describes the major software components of the Tari Base Layer network.

## Related RFCs

* [RFC-0001: Overview](RFC-0001_overview.md)
* [RFC-0110: Base Nodes](./RFC-0110_BaseNodes.md)
* [RFC-0130: Mining](./RFC-0130_Mining.md)
* [RFC-0150: Wallets](./RFC-0150_Wallets.md)


## Description

The Tari Base Layer network comprises the following major pieces of software:

* Base Layer full node implementation - The base layer full nodes are the consensus-critical pieces of software for the
  Tari base layer and cryptocurrency. The base nodes validate and transmit transactions and blocks and maintain
  consensus about the longest valid proof-of-work blockchain.
* Mining software - Mining nodes perform proof-of-work to secure the base layer and compete to submit the
  next valid block into the Tari blockchain. Tari is merge-mined with Monero. The Tari source provides two alternatives
  for Tari miners:
  * A standalone miner
  * A stratum-compatible pool miner.
* Wallet software - Client software and APIs offering means to construct transactions, query nodes for information and
  maintain personal private keys

These three main pieces of software make use of common functionality provided by the following libraries within the Tari
project source code:
* Local data storage
* Cryptography services
* Peer-to-peer networking and messaging services

[RFC-0110](./RFC-0110_BaseNodes.md) provides more detail on how the source code is structured within the Tari codebase.

