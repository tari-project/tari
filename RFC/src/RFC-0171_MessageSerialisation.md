# RFC-0711/MessageSerialisation

## Message Serialization

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019. The Tari Development Community

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

This document describes the message serialisation formats for message payloads used in the Tari network.

## Related RFCs

[RFC-0710: The Tari Communication Network and Network Communication Protocol](RFC-0170_NetworkCommunicationProtocol.md)

## Description

One way of interpreting the Tari network is that it is a large peer-to-peer messaging application. The entities chatting
on the network include

* Users
* Wallets
* Base nodes
* Validator nodes

The types of messages that these entities send might include

* Text messages
* Transaction messages
* Block propagation messages
* Asset creation instructions
* Asset state change instructions
* State Checkpoint messages

For successful communication to occur, the following needs to happen:

* The message is translated from its memory storage format into a standard payload format that will be transported over
  the wire.
* The communication module wraps the payload into a message format, which may entail any/all of
  * adding a message header to describe the type of payload,
  * encrypting the message,
  * signing the message, and
  * adding destination /recipient metadata.
* The communication module then sends the message over the wire.
* The recipient receives the message and unwraps it, possibly performing any/all of the following:
  * decryption,
  * verifying signatures,
  * extracting the payload,
  * passing the serialised payload to modules that are interesting in that particular message type
* The message is deserialised into the correct data structure for use by the receiving software

This document only covers the first and last steps: _viz_: serialising data from in-memory objects to a format that can
be transmitted over the wire. The other steps are handled by the Tari communication protocol.

In addition to machine-to-machine communication, we also standardise on human-to-machine communication. Use cases for
this include

* Handcrafting instructions or transactions. The ideal format here is a very human-readable format.
* Copying transactions or instructions from cold wallets. The ideal format here is a compact but easy-to-copy format.
* Peer-to-peer text messaging. This is just a special case of what has already been described, with the message
  structure containing a unicode `message_text` field.

When sending a message from a human to the network, the following happens:

* The message is deserialised into the native structure.
* The deserialisation acts as an automatic validation step.
* Additional validation can be performed.
* The usual machine-to-machine process is followed as described above.

### Binary serialisation formats

The ideal properties for binary serialisation formats are:

* Widely used across multiple platforms and languages, but with excellent Rust support.
* Compact binary representation
* Serialisation "Just Works"(TM) with little or no additional coding overhead.

Several candidates fulfil these properties to some degree.

#### [ASN.1](http://www.itu.int/ITU-T/asn1/index.html)

* Pros:
  * Very mature (was developed in the 80s)
  * Large number of implementations
  * Dovetails into ZMQ nicely
* Cons:
  * Limited Rust / Serde support
  * Requires schema (additional coding overhead if no automated tools for this exist)


#### [Message Pack](http://msgpack.org/)

* Pros:
  * Very compact
  * Fast
  * Multiple language support
  * Good Rust / SerDe support
  * Dovetails into ZMQ nicely
* Cons:
  * No metadata support

#### [Protobuf](https://code.google.com/p/protobuf/)

Similar to [Message Pack](#message-pack), but also requires schema's to be written and compiled. Serialisation performance and size
is similar to Message Pack. Can work with ZMQ but is better designed to be used with gRPC.

#### [Cap'n Proto](http://kentonv.github.io/capnproto/)

Similar to [Protobuf](#protobuf), but claims to be much faster. Rust is supported.

#### Hand-rolled serialisation

[Hintjens recommends](http://zguide.zeromq.org/py:chapter7#Serialization-Libraries) using hand-rolled serialization for
bulk messaging. While Pieter usually offers sage advice, I'm going to argue against using custom serialisers at this
point in time for the following reasons:
* We're unlikely to improve hugely over MessagePack
* Since serde does 95% of our work for us with MessagePack, there's significant development overhead (and new bugs)
  involved with a hand-rolled solution.
* We'd have to write de/serialisers for every language that wants Tari bindings; whereas every major language has a
  MessagePack implementation.

### Serialisation in Tari

Deciding between these protocols is largely a matter of preference, since there isn't that much to choose between them.
Given that ZMQ is used in other places in the Tari network; MessagePack looks to be a good fit while offering a compact
data structure and highly performant de/serialisation. In Rust in particular, there's first-class support for Message
Pack in serde.

For human-readable formats, it makes little sense to deviate from JSON. For copy-paste semantics, the extra compression
that Base64 offers over raw hex or Base58 makes it attractive.

Many Tari data types' binary representation will be the straightforward MessagePack version of each field in the related
Struct. In these cases, as straightforward `#[derive(Deserialize, Serialize)]` is all that is required to make the data
structure able to be sent over the wire.

However, other structures might need fine tuning, or hand-written serialisation procedures. To capture both use cases,
it is proposed that a `MessageFormat` trait be defined:

```rust
{{#include ../../base_layer/core/src/message.rs:41:49}}
```

This trait will have default implementations to cover most use cases (e.g. a simple call through to `serde_json`). Serde
also offers significant ability to tweak how a given struct will be serialised through the use of
[attributes](https://serde.rs/attributes.html).