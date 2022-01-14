# RFC-0171/MessageSerialization

## Message Serialization

![status: outdated](theme/images/status-outofdate.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

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

The aim of this Request for Comment (RFC) is to describe the message serialization formats for message payloads used in the Tari network.

## Related Requests for Comment

[RFC-0710: Tari Communication Network and Network Communication Protocol](RFC-0170_NetworkCommunicationProtocol.md)

## Description

One way of interpreting the Tari network is that it is a large peer-to-peer messaging application. The entities chatting
on the network include:

* Users
* Wallets
* Base nodes
* Validator nodes

The types of messages that these entities send might include:

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
  * adding a message header to describe the type of payload;
  * encrypting the message;
  * signing the message;
  * adding destination/recipient metadata.
* The communication module then sends the message over the wire.
* The recipient receives the message and unwraps it, possibly performing any/all of the following:
  * decryption;
  * verifying signatures;
  * extracting the payload;
  * passing the serialized payload to modules that are interesting in that particular message type.
* The message is deserialized into the correct data structure for use by the receiving software

This document only covers the first and last steps, i.e. serializing data from in-memory objects to a format that can
be transmitted over the wire. The other steps are handled by the Tari communication protocol.

In addition to machine-to-machine communication, we also standardize on human-to-machine communication. Use cases for
this include:

* Handcrafting instructions or transactions. The ideal format here is a very human-readable format.
* Copying transactions or instructions from cold wallets. The ideal format here is a compact but easy-to-copy format.
* Peer-to-peer text messaging. This is just a special case of what has already been described, with the message
  structure containing a unicode `message_text` field.

When sending a message from a human to the network, the following happens:

* The message is deserialized into the native structure.
* The deserialization acts as an automatic validation step.
* Additional validation can be performed.
* The usual machine-to-machine process is followed, as described above.

### Binary Serialization Formats

The ideal properties for binary serialization formats are:

* widely used across multiple platforms and languages, but with excellent Rust support;
* compact binary representation; and
* serialization "Just Works"(TM) with little or no additional coding overhead.

Several candidates fulfill these properties to some degree.

#### [ASN.1](http://www.itu.int/ITU-T/asn1/index.html)

* Pros:
  * Very mature (was developed in the 1980s)
  * Large number of implementations
  * Dovetails nicely into ZMQ 
* Cons:
  * Limited Rust/Serde support
  * Requires schema (additional coding overhead if no automated tools for this exist)


#### [Message Pack](http://msgpack.org/)

* Pros:
  * Very compact
  * Fast
  * Multiple language support
  * Good Rust/Serde support
  * Dovetails nicely into ZMQ 
* Cons:
  * No metadata support

#### [Protobuf](https://code.google.com/p/protobuf/)

Similar to Message Pack, but also requires schemas to be written and compiled. Serialization 
performance and size
are similar to Message Pack. It Can work with ZMQ, but is better designed to be used with gRPC.

#### [Cap'n Proto](http://kentonv.github.io/capnproto/)

Similar to Protobuf, but claims to be much faster. Rust is supported.

#### Hand-rolled Serialization

[Hintjens recommends](http://zguide.zeromq.org/py:chapter7#Serialization-Libraries) using hand-rolled serialization for
bulk messaging. While Pieter usually offers sage advice, I'm going to argue against using custom serializers at this
stage for the following reasons:

* We're unlikely to improve hugely over MessagePack.
* Since Serde does 95% of our work for us with MessagePack, there's a significant development overhead (and new bugs)
  involved with a hand-rolled solution.
* We'd have to write de/serializers for every language that wants Tari bindings; whereas every major language has a
  MessagePack implementation.

### Serialization in Tari

Deciding between these protocols is largely a matter of preference, since there isn't that much difference between them.
Given that ZMQ is used in other places in the Tari network, MessagePack looks to be a good fit while offering a compact
data structure and highly performant de/serialization. In Rust, in particular, there's first-class support for MessagePack
in Serde.

For human-readable formats, it makes little sense to deviate from JSON. For copy-paste semantics, the extra compression
that Base64 offers over raw hex or Base58 makes it attractive.

Many Tari data types' binary representation will be the straightforward MessagePack version of each field in the related
`struct`. In these cases, a straightforward `#[derive(Deserialize, Serialize)]` is all that is required to enable the data
structure to be sent over the wire.

However, other structures might need fine-tuning, or hand-written serialization procedures. To capture both use cases,
it is proposed that a `MessageFormat` trait be defined:

```rust,compile_fail
pub trait MessageFormat: Sized {
    fn to_binary(&self) -> Result<Vec<u8>, MessageFormatError>;
    fn to_json(&self) -> Result<String, MessageFormatError>;
    fn to_base64(&self) -> Result<String, MessageFormatError>;

    fn from_binary(msg: &[u8]) -> Result<Self, MessageFormatError>;
    fn from_json(msg: &str) -> Result<Self, MessageFormatError>;
    fn from_base64(msg: &str) -> Result<Self, MessageFormatError>;
}
```

This trait will have default implementations to cover most use cases (e.g. a simple call through to `serde_json`). Serde
also offers significant ability to tweak how a given struct will be serialized through the use of
[attributes](https://serde.rs/attributes.html).
