# RFC-0111/BaseNodesArchitecture

## Base Node Architecture

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019 The Tari Development Community

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

The aim of this Request for Comment (RFC) is to describe the high-level Base Node architecture.

## Architectural Layout

The Base Node architecture is designed to be modular, robust and performant.

![Base Layer architecture](theme/images/base_layer_arch.png)

The major components are separated into separate modules. Each module exposes a public Application Programming Interface
(API), which communicates with other modules using asynchronous messages via futures.

### Base Node Service

The Base Node Service is an instantiation of a Tari Comms Service, which subscribes to and handles specific messages
coming from the P2P Tari network via the Comms Module of a live Tari communications node. The Base Node Service's job is
to delegate the jobs required by those messages to its sub-modules, consisting primarily of the Transaction Validation
Service, the Block Validation Service and the Block synchronisation service, using an asynchronous Request-Response
pattern.

The Base Node Service will pass messages back to the P2P network via the Comms Module, based on the results of its
actions.

The primary messages that a Base Node will subscribe to are:

* **NewTransaction.** A New Transaction is being propagated over the network. If it has not seen the transaction before,
  the Base Node will validate the transaction and, if it is valid:
  * add it to its mempool;
  * pass the transaction on to peers.

  Otherwise, the transaction is dropped.
* **NewBlock.** A newly mined block is being propagated over the network. If the node has not seen the block before, the
  node will validate it. Its action depends on the validation outcome:
  * _Invalid block_ - drop the block.
  * _Valid block appending to the longest chain_ - add the block to the local state; propagate the block to peers.
  * _Valid block forking off main chain_ - add the block to the local state; propagate the block to peers.
  * _Valid block building off unknown block_ - add the orphan block to the local state.

* **Sync Request.** A peer is synchronizing state and is asking for block data. The node can decide to:
  * Ignore or ban the peer (based on previous behaviour heuristics).
  * Try and provide the data, returning an appropriate response. Note that most nodes can only offer block data up until
    their pruning horizon. Only full archival nodes can return the full block history. Refer to
    [RFC-0140](RFC-0140_Syncing_and_seeding.md) for more details.

The validation procedures are complex and are thus encapsulated in their own sub-services. These services hold
references to the blockchain state API, the mempool API, a range proof service and whatever other modules they need to
complete their work. Each validation module has a single primary method, `validate_xxx()`, which takes in the
transaction or block to be validated and returns a future that resolves once the validation task is complete.

### Distributed Hash Table (DHT) Service

Peer discovery is a key service that blockchain nodes provide so that the peer mesh network can be navigated by the full
nodes making up the network.

In Tari, the peer-to-peer network is not only used by full nodes (Base Nodes), but also by Validator Nodes, and

Tari and Digital Assets Network (DAN) clients.

For this reason, peer management is handled internally by the Comms layer. If a Base Node wants to propagate a message, 
new block or transaction, for example, it simply selects a `BROADCAST` strategy for the message and the Comms layer
will do the rest.

When a node wishes to query a peer for its peer list, this request will be handled by the `DHTService`. It will
communicate with its Comms module's Peer Manager, and provide that information to the peer.

### Blockchain State Module

The blockchain state module is responsible for providing a persistent storage solution for blockchain state data. For
Tari, this is delivered using the Lightning Memory-mapped Database (LMDB). LMDB is highly performant, intelligent and
straightforward to use. An LMDB is essentially treated as a hash map data structure that transparently handles
memory caching, disk Input/Output (I/O) and multi-threaded access.

The blockchain module is able to run as a standalone service, but must be thread-safe. Block and transaction validation
requests are futures-based. These are asynchronous requests, which means that multiple validation requests can and
should be handled in parallel, in separate threads. Initially, all the logic for a single block or transaction
validation can be executed in sequence, wrapped inside a single future. However, there is scope to optimise this in
future; for example: Validating a block entails checking the proof-of-work (very slow), checking signatures (fast, but
many of them), and checking the accounting (slow). Each of these sub-tasks could also be spun off as a future, with a
master future co-ordinating the sub-futures and assembling the final results.

Tokio is becoming the _de facto_ standard for asynchronous programming in Rust.

Tokio's default task executor provides multi-threaded work-stealing work queues and CPU-bound worker threads out of the
box. This is a good fit for the type of work that base nodes must perform. In addition, the
[Tower project](https://github.com/tower-rs) provides a set of traits and middleware that will be very useful in Tari
services, and so it is recommended to follow the Services pattern as used by that project.

This RFC proposes that the 0.1 version of tokio is used in the Tari project until the standard
[futures](https://doc.rust-lang.org/std/future/index.html) library has stabilised before making a switch.


### Mempool Module

The mempool module tracks (valid) transactions that the node knows about, but that have not yet been included in a
block. The mempool is ephemeral and non-consensus critical, and as such may be a memory-only data structure. Maintaining
a large mempool is far more important for Base Nodes serving miners than those serving wallets. A mempool will slowly
rebuild after a node reboots.

That said, the mempool module must be thread safe. The Tari mempool module handles requests in the same way as the
Blockchain state module: via futures. The mempool structure itself is a set of hash maps as described in [RFC-0190]. For
performance reasons, it may be worthwhile using a [concurrent hash map] implementation.

### gRPC Interface

Base Nodes need to provide a local communication interface in addition to the P2P communication interface. This is
best achieved using [gRPC]. The Base Node gRPC interface provides access to the public API methods of the Base Node
Service, the mempool module and the blockchain state module, as discussed above.

gRPC access is useful for tools such as local User Interfaces (UIs) to a running Base Node; client wallets running on
the same machine as the Base Node that want a more direct communication interface to the node than the P2P network
provides; third-party applications such as block explorers; and, of course, miners.

A non-exhaustive list of methods the base node module API will expose includes:

* Blockchain state calls, including:
    * checking whether a given Unspent Transaction Output (UTXO) is in the current UTXO set;
    * requesting the latest block height;
    * requesting the total accumulated work on the longest chain;
    * requesting a specific block at a given height;
    * requesting the Merklish root commitment of the current UTXO set;
    * requesting a block header for a given height;
    * requesting the block header for the chain tip;
    * validating signatures for a given transaction kernel;
    * validating a new block without adding it to the state tree;
    * validating and adding a (validated) new block to the state, and informing of the result (orphaned, fork, re-org, etc.).
* Mempool calls
  * The number of unconfirmed transactions
  * The number of orphaned transactions
  * Returning a list of transaction ranked by some criterion (of interest to miners)
  * The current size of the mempool (in transaction weight)
* Block and transaction validation calls
* Block synchronisation calls


[concurrent hash map]: https://crates.io/crates/chashmap
[gRPC]: https://grpc.io/
[RFC-0190]: RFC-0190_Mempool.md