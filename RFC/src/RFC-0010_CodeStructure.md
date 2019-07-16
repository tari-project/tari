# RFC-0010/CodeStructure

## Tari Code Structure and Organization

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright <YEAR> <COPYRIGHT HOLDER | The Tari Development Community>

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

The aim of this Request for Comment (RFC) is to describe and explain the Tari codebase layout.

## Related Requests for Comment

None.

## Description

The code follows a domain-driven design layout [(DDD)], with top-level directories falling into infrastructure, domain
and application layers.

### Infrastructure Layer

The infrastructure layer provides a set of crates that have general infrastructural utility. The rest of the Tari codebase can make use
of these crates to obtain persistence, communication and cryptographic services. The infrastructure layer doesn't know
anything about blockchains, transactions or digital assets.

We recommend that code in this layer generalizes infrastructure services behind abstraction layers as much as is
reasonable, so that specific implementations can be swapped out with relative ease.

### Domain Layer

The domain layer houses the Tari "business logic". All protocol-related concepts and procedures are defined and
implemented here.

This means that any and all terms defined in the [Glossary] will have a software implementation here, and only here.
They can be _used_ in the application layer, but must be *implemented* in the domain layer.

The domain layer can make use of crates in the infrastructure layer to achieve its goals.

### Application Layer

In the application layer, applications build on top of the domain layer to produce the executable software that is 
deployed as part of the Tari network.

As an example, the following base layer applications may be developed as part of the Tari protocol release:

* A standalone miner (tari_miner)
* A pool miner (tari_pool_miner)
* A command line interface (CLI) wallet for the Tari cryptocurrency (cli_wallet)
* A base node executable (tari_basenode)
* A REST Application Programming Interface (API) server for the base node

### Code Layout

The top-level source code directories in this repository reflect the respective [(DDD)] layers; except that there are two
domain layer directories, corresponding to the two network layers that make up the Tari network.

1. The `infrastructure` directory contains application-layer code and is not Tari-specific. It holds the following
   crates:
    - `comms` - the networking and messaging subsystem;
    - `crypto` - all cryptographic services, including a Curve25519 implementation;
    - `storage` - data persistence services, including a Lightning Memory-mapped Database (LMDB) persistence implementation;
    - `merklemountainrange` - an independent implementation of a Merkle Mountain Range;
   -  `derive` - a crate to contain `derive(...)` macros.

1. `base_layer` is a domain-layer directory and contains:
    - `blockchain` - the Tari consensus code;
    - `core` - common classes and traits, such as [Transaction]s and [Block]s;
    - `mempool` - the unconfirmed transaction pool implementation;
    - `mining` - the merged-mining modules;
    - `p2p` - the block and transaction propagation module;
    - `api` - interfaces for clients and wallets to interact with the base layer components.
1. `digital_assets_layer` is a domain-layer directory. It contains code related to the management of native Tari digital
   assets. Its substructure is to be determined.
1. `applications` contains crates for all the application-layer executables that form part of the Tari codebase.


[Glossary]: ../Glossary.md "Glossary"
[DDD]: https://en.wikipedia.org/wiki/Domain-driven_design "Wikipedia: Domain Driven Design"
[transaction]: ../Glossary.md#transaction
[block]: ../Glossary.md#block
