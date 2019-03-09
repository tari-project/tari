# RFC-0150/Wallets

## Base layer wallet module

![status: draft](https://github.com/tari-project/tari/raw/master/RFC/src/theme/images/status-draft.svg)

**Maintainer(s)**: [Yuko Roodt](https://github.com/neonknight64), [Cayle Sharrock](https://github.com/CjS77)

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

This document will propose the functionality and techniques required by the [Base Layer] Tari [wallet] module. The module
exposes the core wallet functionality that user-facing wallet applications may be built on.

## Related RFCs

* [RFC-0100: Base layer](https://github.com/tari-project/tari/blob/master/RFC/src/RFC-0100_BaseLayer.md)

This RFC is derived from a proposal first made in [this issue](https://github.com/tari-project/tari/issues/17).

## Description

### Key responsibilities

The wallet software is responsible for constructing and negotiating [transaction]s for transferring and receiving [Tari token]s on the Base Layer. It should also provide functionality to generate, store and recover a master seed key and derived cryptographic key pairs that can be used for Base Layer addresses and signing of transactions.

### Details of functionality

A detailed description of the required functionality of the Tari software wallet is provided in three parts:
* Basic transaction functionality,
* Key management features and
* the different wallet recovery methods.

#### Basic transaction functionality

- It MUST be able to send and receive Tari coins using [Mimblewimble] transactions.
- It SHOULD be able to establish a connection between different user wallets to negotiate:
  - the construction of a transaction and
  - the signing of multi signature transactions.
- The Tari software wallet SHOULD be implemented as a library or API so that GUI or CLI applications can be developed on top of it.
- It MUST be able to establish a connection to a [Base Node] to submit transactions and monitor the Tari [blockchain].
- It SHOULD maintain an internal ledger to keep track of the Tari token balance of the wallet.
- It MAY offer transaction fee estimation taking into account:
  - transaction byte size,
  - network congestion and
  - the desired transaction priority.
- It SHOULD be able to monitor and return the states (Spent, Unspent or Unconfirmed) of previously submitted transactions by querying information from the connected base node.
- The Wallet SHOULD present the total Spent, Unspent or Unconfirmed transactions in summarised form.
- The wallet software SHOULD be able to update its software to patch potential security vulnerabilities. Automatic updating SHOULD be selected by default, but users can decide to opt out.
- Wallet features requiring querying a base node for information, SHOULD have caching capabilities to reduce bandwidth consumption.

#### Key management features

- It MUST be able to generate a master seed key for the wallet by using one of:
  - input from a user (for example, when restoring a wallet, or in testing),
  - a user defined set of mnemonic word sequences using known word lists,
  - a cryptographically secure random number generator.
- It SHOULD be able to generate derived transactional cryptographic key pairs from the master seed key using deterministic key pair generation.
- It SHOULD store the wallet state using a password or passphrase encrypted persistent key-value database.
- It SHOULD provide the ability to backup the wallet state to a single encrypted file to simplify wallet recovery and reconstruction at a later stage.
- It MAY provide the ability to export the master seed key or wallet state as a printable paper wallet using coded markers.

#### Different methods for recovering the wallet state of the Tari software wallet:
- It MUST be able to reconstruction the wallet state from a manually entered master seed key.
- It MUST have a mechanism to systematically search through the Tari blockchain and mempool for unspent and unconfirmed transactions using the keys derived from the master key.
- The master seed key SHOULD be derivable from a specific set of mnemonic word sequences using known word lists.
- It MAY enable the reconstruction of the master seed key by scanning a coded marker of a paper wallet.

[wallet]: Glossary.md#wallet
[Base Layer]: Glossary.md#base-layer
[tari token]: Glossary.md#tari-coin
[transaction]: Glossary.md#transaction
[mimblewimble]: Glossary.md#mimblewimble
[blockchain]: Glossary.md#blockchain
[base node]: Glossary.md#base-node
