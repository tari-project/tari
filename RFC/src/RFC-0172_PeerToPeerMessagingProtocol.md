# RFC-0172/PeerToPeerMessaging

## Peer to Peer Messaging Protocol

![status: outdated](theme/images/status-outofdate.svg)

**Maintainer(s)**: [Stanley Bondi](https://github.com/sdbondi), [Cayle Sharrock](https://github.com/CjS77) and [Yuko Roodt](https://github.com/neonknight64)

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

The aim of this Request for Comment (RFC) is to describe the peer-to-peer messaging protocol for [communication node]s 
and [communication client]s on the Tari network.

## Related Requests for Comment

- [RFC-0170: NetworkCommunicationProtocol](rfc-0170_NetworkCommunicationProtocol.md)
- [RFC-0171: MessageSerialization](RFC-0171_MessageSerialisation.md)

## Description

### Assumptions

- Either every [communication node] or [communication client] has access to a Tor/I2P proxy, or a native Tor/I2P implementation
  exists, which allows communication across the Tor network.
- All messages are de/serialized as per [RFC-0171: Message Serialisation].

### Broad Requirements

Tari network peer communication must facilitate secure, private and efficient communication
between peers. Broadly, a [communication node] or [communication client] MUST be capable of:

- bidirectional communication between multiple connected peers;
- private and secure over-the-wire communication;
- understanding and constructing Tari messages;
- encrypting and decrypting message payloads;
- gracefully reestablishing dropped connections; and (optionally)
- communicating to a SOCKS5 proxy (for connections over Tor and I2P).

Additionally, communication nodes MUST be capable of performing the following tasks:

- opening a control port for establishing secure peer channels;
- maintaining a list of known peers in the form of a routing table;
- forwarding directed messages to neighbouring peers; and
- broadcasting messages to neighbouring peers.

### Overall Architectural Design

The Tari communication layer has a modular design to allow for the various communicating nodes and clients to
use the same infrastructure code.

The design is influenced by an open-source library called [ZeroMQ] and the ZeroMQ C bindings are a dependency of
the project. ZeroMQ's over-the-wire protocol is relatively simple, and replicating ZeroMQ framing in a custom
implementation should not be prohibitively difficult. However, ZeroMQ offers many valuable features, which would be a 
significantly larger undertaking to reproduce. Fortunately, bindings or native ports are available in numerous languages.

To learn more about ZeroMQ, read [the guide](http://zguide.zeromq.org/page:all). It's an enjoyable and worthwhile read.

A quick overview of what ZeroMQ provides:

- A simple socket Application Programming Interface (API).
- Some well-defined patterns to connect sockets together.
- Sockets that are tiny asynchronous message queues, which:
  - abstract away complexity around the underlying socket;
  - are transport agnostic, meaning you can choose between Transmission Control Protocol (TCP), Pragmatic General 
  Multicast (PGM), Inter-process Communication (IPC) and in-process (inproc) transports with little or no changes to code; 
  and
  - transparently reconnect when connections are dropped.
- The `inproc` transport for message passing between threads without mutex locks.
- Built-in protocol for asymmetric encryption over the wire using Curve25519.
- Ability to send and receive multipart messages using a simple framing scheme. More info [here](http://zguide.zeromq.org/php:chapter2#toc11).
- Support for Secure Socket (SOCKS) proxies.

This document will refer to several [ZeroMQ socket]s. These are referred to by prepending `ZMQ_` and the name
of the socket in capitals. For example, `ZMQ_ROUTER`.

**Note about ZeroMQ frames and multipart messages:**

ZeroMQ frames are length-specified blocks of binary data and can be strung together to make multipart messages.

```text
|5|H|E|L|L|O|*|0|*|3|F|O|O|+|
* = more flag
+ = no more flag
```

_A multipart message consisting of three frames "HELLO", a zero-length frame and "FOO"_

When this RFC mentions 'multipart messages', this is what it's referring to.

#### Establishing a Connection

Every participating [communication node] SHOULD open a control socket (refer to [ControlService]) to allow peers to negotiate and
establish a peer connection. The [NetAddress] of the control socket is what is stored in peers' routing tables and will
be used to establish new ephemeral [PeerConnection]s. Any peer that wants to connect MUST establish a connection
to the control socket of the destination peer to negotiate a new encrypted PeerConnection.

Once a connection is established, messages can be sent and received directly to or from the [Peer].

Incoming messages are validated, deserialized and handled as appropriate.

#### Encryption

Two forms of encryption are used:

- Over-the-wire encryption, in which traffic between nodes is encrypted using ZMQ's [CURVE](http://curvezmq.org/page:read-the-docs)
  implementation.
- Payload encryption, in which the [MessageEnvelopeBody] is encrypted in such a way that it can only be decrypted by the 
destination recipient.

### Components

The following components are proposed:

<div class="mermaid">
graph TD
CSRV[ControlService]
IMS[InboundMessageService]
PM[PeerManager]
CM[ConnectionManager]
BCS[BroadcastStrategy]
OMS[OutboundMessageService] 
PC[PeerConnection] 
NA[NetAddress]
STR[Storage]
PEER[Peer]
RT[RoutingTable]
IMS --> PM
IMS --> CM
CSRV --> CM
CSRV --> PM
OMS -->|execute| BCS
OMS --> PM
OMS --> CM
PM --> RT
PM --> STR
CM --> PC
PC -->|has one| PEER
PEER -->|has many| NA
</div>

#### NetAddress

Represents:

- IP address;
- Onion address; or
- I2P address.

```rust,compile_fail
#[derive(Clone, PartialEq, Eq, Debug)]
/// Represents an address which can be used to reach a node on the network
pub enum NetAddress {
    /// IPv4 and IPv6
    IP(SocketAddress),
    Tor(OnionAddress),
    I2P(I2PAddress),
}
```

#### Messaging Structure

The following illustrates the structure of a Tari message:

```text
+----------------------------------------+
|             MessageEnvelope            |
|  +----------------------------------+  |
|  |        MessageEnvelopeHeader     |  |
|  +----------------------------------+  |
|  +----------------------------------+  |
|  |      MessageEnvelopeBody         |  |
|  |     (optionally encrypted)       |  |
|  | +------------------------------+ |  |
|  | |            Message           | |  |
|  | |   +-----------------------+  | |  |
|  | |   |     MessageHeader     |  | |  |
|  | |   +-----------------------+  | |  |
|  | |                              | |  |
|  | |   +-----------------------+  | |  |
|  | |   |      MessageBody      |  | |  |
|  | |   +-----------------------+  | |  |
|  | +------------------------------+ |  |
|  +----------------------------------+  |
+----------------------------------------+
```

#### MessageEnvelope Wire format

Every Tari message MUST use the MessageEnvelope format. This format consists of four frames of a multipart message.

A MessageEnvelope  represents a message that has either just come off or is about to go on to the wire. and consists of 
the following:

| Name     | Frame | Length (Octets) | Type      | Description                                                                                                                                            |
| -------- | ----- | --------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| identity | 0     | 8               | `[u8;8]`  | The identifier that a `ZMQ_ROUTER` socket expects so that it knows the intended destination of the message. This can be thought of as a session token. |
| version  | 1     | 1               | `u8`      | The wire protocol version.                                                                                                                             |
| header   | 2     | Varies          | `Vec<u8>` | Serialized bytes of data containing an unencrypted [MessageEnvelopeHeader].                                                                                      |
| body     | 3     | Varies          | `Vec<u8>` | Serialized bytes of data containing an unencrypted or encrypted [MessageEnvelopeBody].                                                                      |

The header and decrypted body MUST be deserializable as per [RFC-0171: MessageSerialization](RFC-0171_MessageSerialisation.md).

#### MessageEnvelopeHeader

Every MessageEnvelope MUST have an unencrypted header containing the following fields:

| Name      | Type                      | Description                                                                                        |
| --------- | ------------------------- | -------------------------------------------------------------------------------------------------- |
| version   | `u8`                      | Message protocol version.                                                                      |
| source    | `PublicKey`               | Source public key.                                                                             |
| dest      | `Option<NodeDestination>` | Destination [node ID] or public key. A destination is optional.                                |
| signature | `[u8]`                    | Signature of the message header and body, signed with the private key of the source.               |
| flags     | `u8`                      | <ul><li>bit 0: 1 indicates that the message body is encrypted</li><li>bits 1-7: reserved</li></ul>. |

A [communication node] and [communication client]:

- MUST validate the signature of the message using the source's public key.
- MUST reject the message if the signature verification fails.
- If the encryption bit flag is set:
  - MUST attempt to decrypt the [MessageEnvelopeBody]; or failing that
  - MUST forward the message to a subset of peers using the `Closest` [BroadcastStrategy].
  - MUST discard the message if the body is not encrypted.

#### MessageEnvelopeBody

A MessageEnvelopeBody is the payload of the [MessageEnvelope]. A [MessageEnvelopeBody] may be encrypted as required.

It consists of a [MessageHeader] and [Message] of a particular predefined [MessageType].

#### MessageType

An enumeration of the messages that are part of the Tari network. MessageTypes are represented
as an unsigned eight-bit integer and each value must be mapped to a corresponding Message struct.

All MessageTypes fall within a particular numerical range according to the message's concern:

| Category     | Range   | # Message Types | Description                                                  |
| ------------ | ------- | --------------- | ------------------------------------------------------------ |
| `reserved`     | 0       | 1               | Reserved for control messages such as `Ack`.                  |
| `net`        | 1-32    | 32              | Network-related messages such as `join` and `discover`.       |
| `peer`       | 33-64   | 32              | Peer connection messages, such as `establish connection`.     |
| `blockchain` | 65-96   | 32              | Messages related to the blockchain, such as `add block`.      |
| `vn`         | 97-224  | 128             | Messages related to the validator nodes, such as `execute instruction`. |
| `extended`   | 225-255 | 30              | Reserved for future use.                                      |

In documentation, MessageTypes can be referred to by the category and name. For example, `peer::EstablishConnection` and
`net::Discover`.

#### MessageHeader

Every Tari message MUST have a header containing the following fields:

| Name         | Type | Description                                                        |
| ------------ | ---- | ------------------------------------------------------------------ |
| version      | `u8` | The message version.                                               |
| message_type | `u8` | An enumeration of the message type of the body. Refer to [MessageType]. |

As this is part of the [MessageEnvelopeBody], it can be encrypted along with the rest of the message,
which keeps the type of message private.

#### MessageBody

Messages are an intention to perform a task. MessageType names should thus be a verb such as `net::Join` or `blockchain::AddBlock`.

All messages can be categorized as follows; each categorization has rules for how they should be handled:

- A propagation message
  - SHOULD NOT have a destination in the MessageHeader;
  - MUST be forwarded;
  - SHOULD use the `Random` BroadcastStrategy;
  - SHOULD discard a message that it has seen within the [DuplicateMessageWindow].
- A direct message
  - MUST have a destination in the MessageHeader;
  - SHOULD be discarded if it does not have a destination;
  - SHOULD discard a message that it has seen before;
  - MUST use the `Direct` BroadcastStrategy if a destination peer is known;
  - SHOULD use the `Closest` BroadcastStrategy if a destination peer is not known.
- An encrypted message
  - MUST undergo an attempt to be decrypted by all recipients;
  - MUST be forwarded by recipients if it cannot be decrypted;
  - SHOULD discard a message that it has seen before.

The [MessageType] in the header MUST be used to determine the type of the message deserialized.
If the deserialization fails, the message SHOULD be discarded.

#### DuplicateMessageWindow

A configurable length of time for which message signatures should be tracked in order to eliminate duplicate messages.
This should be long enough to make it highly unlikely that a particular message will be processed again
and short enough to not be a burden on the node.

#### InboundConnection

A thin wrapper around a `ZMQ_ROUTER` socket, which binds to a [NetAddress] and accepts incoming multipart messages.
This connection blocks until there is data to read, or a timeout is reached. In both cases, the `receive` method
can be called again (i.e. in a loop) to continue listening for messages. Client code should run this loop in its own thread.
`send` is only called (if at all) in response to an incoming message.

Fields may include

- a NetAddress;
- a timeout;
- underlying [ZeroMQ socket].

Methods may include:

- `receive()`
- `send(data)`
- `set_encryption(secret_key)`
- `set_socks_proxy(address)`
- `set_hwm(v)`

An InboundConnection:

- MUST perform the "server-side" [CurveZMQ](http://curvezmq.org/page:read-the-docs) encryption protocol if encryption is set.
  - Using [ZeroMQ]. this means setting the socketopts `ZMQ_CURVE_SERVER` to 1 and `ZMQ_CURVE_SECRETKEY` to the secret key before binding.
- MUST listen for and accept TCP connections.
  - For an IP NetAddress, bind on the given host IP and port.
  - For an Onion NetAddress, bind on 127.0.0.1 and the given port.
  - For an I2P NetAddress, as yet undetermined.
- MUST read multipart messages and return them to the caller.
  - If the timeout is reached, return an error to be handled by the calling code.

#### OutboundConnection

A thin wrapper around a `ZMQ_DEALER` socket, which connects to a [NetAddress] and sends outbound multipart messages.
This connection blocks until data can be written, or a timeout is reached. The timeout should never be reached, as
[ZeroMQ] internally queues messages to be sent.

Fields may include:

- a NetAddress;
- underlying [ZeroMQ socket].

Methods may include:

- `send(msg)`
- `receive()`
- `disconnect()`
- `set_encryption(server_pk, client_pk, client_sk)`
- `set_socks_proxy(address)`
- `set_hwm(v)`

An OutboundConnection:

- MUST perform the "client-side" [CurveZMQ](http://curvezmq.org/page:read-the-docs) encryption protocol if encryption is set.
  - Using ZeroMQ, this means setting the socketopts `ZMQ_CURVE_SERVERKEY`, `ZMQ_CURVE_SECRETKEY` and `ZMQ_CURVE_PUBLICKEY`.
- MUST connect to a TCP endpoint.
  - For an IP NetAddress, connect to the given host IP and port.
  - For an Onion NetAddress, connect to the onion address using the TCP, e.g. `tcp://xyz...123.onion:1234`.
  - For an I2P NetAddress, as yet undetermined.
- MUST write the parts of the given MessageEnvelope to the socket as a multipart message consisting of, in order:
  - identity;
  - version;
  - header;
  - body.
- If specified, MUST set a High Water Mark (HWM) on the underlying ZeroMQ socket.
- If the HWM is reached, a call to `send` MUST return an error and any messages received SHOULD be discarded.

#### Peer

A single peer that can communicate on the Tari network.

Fields may include:

- `addresses` - a list of [NetAddress]es associated with the peer, perhaps accompanied by some bookkeeping metadata, such 
as preferred address;
- `node_type` - the type of node or client, i.e. [BaseNode], [ValidatorNode], [Wallet] or [TokenWallet]);
- `last_seen` - a timestamp of the last time a message has been sent/received from this peer;
- `flags` - 8-bit flag;
  - bit 0: is_banned,
  - bit 1-7: reserved.

A peer may also contain reputation metrics (e.g. rejected_message_count, avg_latency) to be used to decide
if a peer should be banned. This mechanism is yet to be decided.

#### PeerConnection

Represents direct bidirectional connection to another node or client. As connections are bidirectional,
the PeerConnection need only hold a single [InboundConnection] or [OutboundConnection], depending on if the
node requested a peer connect to it or if it is connecting to a peer.

PeerConnection will send messages to the peer in a non-blocking, asynchronous manner as long as the connection
is maintained.

It has a few important functions:

- managing the underlying network connections, with automatic reconnection if necessary;
- forwarding incoming messages onto the given handler socket; and
- sending outgoing messages.

Unlike InboundConnection and OutboundConnection, which are essentially stateless,
`PeerConnection` maintains a particular `ConnectionState`.

- `Idle` - the connection has not been established.
- `Connecting` - the connection is in progress.
- `Connected` - the connection has been established.
- `Suspended` - the connection has been suspended. Incoming messages will be discarded, calls to `send()` will error.
- `Dead` - the connection is no longer active because the connection was dropped.
- `Shutdown` - the connection is no longer active because it was shut down.

Fields may include:

- a connection state;
- a control socket;
- a peer connection `NetAddress`;
- a direction (either `Inbound` or `Outbound`);
- a public key obtained from the connection negotiation;
- (optional) SOCKS proxy.

Methods may include:

- `establish()`
- `shutdown()`
- `suspend()`
- `resume()`
- `send(msg)`

A `PeerConnection`:

- MUST listen for data on the given [NetAddress] using an InboundConnection;
- MUST sequentially try to connect to one of the peer's NetAddresses until one succeeds or all fail using an OutboundConnection;
- MUST immediately reject and dispose of a multipart message not consisting of four parts, as detailed in MessageEnvelope;
- MUST construct a MessageEnvelope from the multiple parts;
- MUST pass the constructed MessageEnvelope to the message handler;
- MUST transition to `Connecting` state and retry the connection, should a connection drop;
- MUST send a `net::Disconnect` message and drop the connection when a shutdown signal is received.

#### ConnectionManager

The ConnectionManager manages a set of live PeerConnections. It provides an abstraction for other components
to initiate and use PeerConnections without having to worry about attaching the new PeerConnection to message handlers.

It consists of a list of active peer connections and an `inproc` message handler socket. This socket is 'written to' whenever
a message is received from any active [PeerConnection] for other components to act on.

Methods may include:

- `establish_connection(Peer)` - create and return a new PeerConnection;
- `disconnect(peer)` - disconnect a particular peer;
- `suspend()` - temporarily suspend connections;
- `resume()` - temporarily suspend connections;
- `shutdown` - cleanly shut down all PeerConnections.

The `ConnectionManager`:

- MUST call `suspend` on every PeerConnection if its `suspend` method is called;
- MUST call `resume` on every PeerConnection if its `resume` method is called;
- MUST call `shutdown` on every PeerConnection if its `shutdown` method is called
- MUST create a new PeerConnection with the given Peer and NetAddress, when `establish_connection` is called;
- MUST call `shutdown` on the PeerConnection and remove the connection for the given peer, when `disconnect(peer)` is called;
- MAY disconnect peers if the connection has not been used for an extended period;
- SHOULD disconnect the least recently used peer if the connection pool is greater than `max connections`

#### ControlService

The purpose of this service is to negotiate a new secure PeerConnection.

The control service accepts a single message:

- `peer::EstablishConnection(pk, curve_pk, net_address)`.

A ControlService:

- MUST listen for connections on a predefined CONTROL PORT;
- SHOULD deny connections from banned peers.

The steps to establish a peer connection are as follows:

Alice wants to connect to Bob

1. Alice creates a `PeerConnection` to which Bob can connect.
   - A new CURVE key pair is generated.
2. Alice connects to Bob's control server and Bob accepts the connection.
3. Alice sends a `peer::establish_connection` message, with:
   - the CURVE public key for the socket connection;
   - the node's public key corresponding to its [Node ID]; and
   - the [NetAddress] of the new PeerConnection.
4. Bob accepts this request and opens a new `PeerConnnection` socket using Alice's CURVE public key.
5. Bob connects to the given NetAddress and sends a `peer::establish_connection` message.
6. If Alice accepts the connection, they can begin sending messages. If not, both sides terminate the connection.

#### PeerManager

The PeerManager is responsible for managing the list of peers with which the node has previously interacted.
This list is called a routing table and is made up of [Peer]s.

The PeerManager can

- add a peer to the routing table;
- search for a peer given a node ID, public key or [NetAddress];
- delete a peer from the list;
- persist the peer list using a storage backend;
- restore the peer list from the storage backend;
- maintain lightweight views of peers, using a filter criterion, e.g. a list of peers that have been banned, i.e. a denylist; and
- prune the routing table based on a filter criterion, e.g. last date seen.

#### MessageDispatcher

The MessageContext contains:

- the requesting PeerConnection;
- the MessageHeader;
- the deserialized message;
- the OutboundMessageService.

Basically, all the tools the handler needs to interact with the network.

A MessageDispatcher is responsible for:

- constructing the MessageContext;
- finding the message handler that is associated with the MessageType;
- passing the MessageContext to the handler; and
- ignoring the message if the handler cannot be found.

An example API may be:

```rust,compile_fail
let dispatcher = MessageDispatcher::<MessageType>::new()
    .middleware(logger)
    .route(BlockchainMessageType::NewBlock, BlockHandlers::store_and_broadcast)
    ...
    .route(NetMessageType::Ping, send_pong);

inbound_msg_service.set_handler(dispatcher.handler);
```

#### InboundMessageService

InboundMessageService is a service that receives messages over a non-blocking asynchronous socket and
determines what to do with it. There are three options: handle, forward and discard.

A pool of worker threads (with a configurable size) is started and each one listens for messages on its $1:n$ `inproc` message
socket. A `ZMQ_DEALER` socket is suggested for fair-queueing work amongst workers, who listen for work with a `ZMQ_REP`.
The workers read off this socket and process the messages.

An InboundMessageService:

- MUST receive messages from all PeerConnections; and
- MUST write the message to the worker socket.

A worker:

- MUST deserialize the MessageHeader.
  - If unable to deserialize, MUST discard the message.
- MUST check the message signature.
  - MUST discard the message if the signature is invalid.
  - MUST discard the message if the signature has been processed within the [DuplicateMessageWindow].
- If the encryption flag is set:
  - MUST attempt to decrypt the message.
    - If successful, process and handle the message.
    - Otherwise, MUST forward the message using the `Random` BroadcastStrategy.
    - If the message is not encrypted, MUST discard it.
- If the destination [node ID] is set:
  - If the destination matches this node's ID - process and handle the message.
  - If the destination does not match this node's ID - MUST forward the message using the `Closest` BroadcastStrategy.
- If the destination is not set
  - If the MessageType is a kind of propagation message:
    - MUST handle the message;
    - MUST forward the message using the `Random` BroadcastStrategy.
  - If the MessageType is a kind of encrypted message:
    - MUST attempt to decrypt and handle the message;
    - if successful, MUST handle the message;
    - if unsuccessful, MUST forward the message using the `Random` or `Flood` BroadcastStrategy,

#### OutboundMessageService

OutboundMessageService is responsible for using the connection and peer infrastructure to
send messages to the rest of the network.

In particular, it is responsible for:

- serializing the message body;
- constructing the MessageEnvelope;
- executing the required BroadcastStrategy; and
- sending messages using the [ConnectionManager].

The actual sending of messages can be requested via the public `send_message` method, which takes a
MessageHeader, MessageBody and BroadcastStrategy as parameters.

`send_message` then selects an appropriate peer(s) from the ConnectionManager according to the
BroadcastStrategy and sends the message to each of the selected peers.

BroadcastStrategy determines how a set of peer nodes will be selected and can be:

- `Direct` - send to a particular peer matching the given [node ID];
- `Flood` - send to all known peers who are not [communication clients];
- `Closest` - send to $n$ closest peers who are not [communication clients]; or
- `Random` - send to a random set of peers of size $n$ who are not [communication clients].

### Privacy Features

The following privacy features are proposed:

- A [communication node] or [communication client] MAY communicate solely over the Tor/I2P networks.
- All traffic (with the exception of the control service) MUST be encrypted.
- Messages MAY encrypt the body of a MessageEnvelope, which only a particular recipient can decrypt.
- The `destination` header field can be omitted when used in conjunction with body encryption; the destination is
  completely unknown to the rest of the network.

### Store and Forward Strategy

Sometimes it may be desirable for messages to be sent without a destination node/client being online. This
is especially important for a modern chat/messaging application.

The mechanism for this is proposed as follows:

Each [communication node] MUST allocate some disk space for storage of messages for offline recipients.
Only some allowlisted MessageTypes are permitted to be stored. A sender sends a message destined for a particular
[node ID] to its closest peers, which forward the message to their closest peers, and so on.

Eventually, the message will reach nodes that either know the destination or are very close to the destination.
These nodes MUST store the message in some pending message bucket for the destination. The maximum number of
buckets and the size of each bucket SHOULD be sufficiently large as to be unlikely to overflow, but not so
large as to approach disk space problems. Individual messages should be small and responsibilities for
storage spread over the entire network.

A communication node

- MUST store messages for later retransmission, if all of the following conditions are true:
  - the MessageType is permitted to be stored;
  - there are fewer than $n$ closer online peers to the destination.
- MUST retransmit pending messages when a closer peer comes online or is added to the routing table.
- MAY remove a bucket, in any of the following conditions:
  - The bucket is empty;
  - A configured maximum number of buckets has been reached. Discard the bucket with the earliest creation timestamp.
  - The number of closer online peers to the destination is equal to or has exceeded $n$.
- MAY expire individual messages after a sufficiently long Time to Live (TTL)

This approach has the following benefits:

- When a destination comes online, it will receive pending messages without having to query them.
- The "closer within a threshold" metric is simple.
- Messages are stored on multiple peers, which makes it less likely for messages to disappear as nodes come and go
  (depending on threshold $n$).

### Queue Overflow Strategy

Inbound/OutboundConnections (and therefore PeerConnection) have an HWM set.

If the HWM is hit:

- any call to `send()` should return an error; and
- incoming messages should be silently discarded.

### Outstanding Items

- A PeerConnection will probably need to implement a heartbeat to detect if a peer has gone offline.
- InboundConnection(Service) may want to send small replies (such as OK, ERR) when the message has been accepted or rejected.
- OutboundConnection(Service) may want to receive and handle small replies.
- Encrypted communication for the [ControlService] would be better privacy, but since ZMQ requires a CURVE public key before
  the connection is bound, a dedicated "secure connection negotiation socket" would be needed.
- Details of distributed message storage.
- Which [NetAddress] to use if a peer has many.

[basenode]: Glossary.md#base-node
[broadcaststrategy]: #outboundmessageservice
[communication client]: Glossary.md#communication-client
[communication node]: Glossary.md#communication-node
[connectioncontext]: #connectioncontext
[controlservice]: #controlservice
[duplicatemessagewindow]: #duplicatemessagewindow
[inboundconnection]: #inboundconnection
[message]: #message
[MessageEnvelopeBody]: #messageenvelopebody
[MessageEnvelopeHeader]: #messageenvelopeheader
[messageheader]: #messageheader
[messagetype]: #messagetype
[netaddress]: #netaddress
[node id]: Glossary.md#node-id
[outboundconnection]: #outboundconnection
[peer]: #peer
[peerconnection]: #peerconnection
[rfc-0171: message serialisation]: RFC-0171_MessageSerialisation.md
[tokenwallet]: Glossary.md#token-wallet
[validatornode]: Glossary.md#validator-node
[wallet]: Glossary.md#wallet
[zeromq socket]: http://api.zeromq.org/2-1:zmq-socket
[zeromq]: http://zeromq.org/
