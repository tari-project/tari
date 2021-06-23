# RFC-0173/Versioning

## Versioning

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Philip Robinson](https://github.com/philipr-za)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2021 The Tari Development Community

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

The aim of this Request for Comment (RFC) is to describe the various types of versioning that nodes on the Tari network 
will use during interaction with other nodes.

## Related Requests for Comment

- [RFC-0710: Tari Communication Network and Network Communication Protocol](RFC-0170_NetworkCommunicationProtocol.md)
- [RFC-0171: MessageSerialization](RFC-0171_MessageSerialisation.md)

## Description

In a decentralized system the set of nodes on the network will run a variety of software versions as time goes on. Some 
of these versions will be compatible and others not. For example, if a crucial consensus change is added during a hard 
fork event. Furthermore, there will be multiple networks running Tari code, i.e. Mainnet vs Testnet. Versioning refers 
to the strategies we will use for nodes to determine if they can communicate.

Tari will contain three different versioning schemes:
1. WireMode is the first byte a peer sends when connecting to another peer, used to identify the network and/or protocol bytes that follow 
2. P2P message versions that will accompany every P2P message,
3. Consensus rules versions that will be exchanged on connection and are included in each block header.

### WireMode byte
In the Bitcoin P2P protocol messages are preceded by 4 
[magic values](https://en.bitcoin.it/wiki/Protocol_documentation#Common_structures) or bytes. These values are used to 
delimit when a new message starts in a byte stream and also are used to indicate which of the Bitcoin networks the node 
is speaking on, such as TestNet or Mainnet.

Tari message packets are encapsulated using the Noise protocol so we do not need the delimiting functionality of these 
bytes but Tari will include a single WireMode byte at the beginning of every connection session. This byte will indicate 
which network a node is communicating on, so that if the counterparty is on a different network it can reject this 
connection cheaply without having to perform any further operations, like completing the Noise protocol handshake.

The following is a proposed mapping of the WireMode byte. Space is left between Mainnet, Stagenet and Testnet bytes for
future use. 
```rust,ignore
   #[repr(u8)]
   enum Network {
      Mainnet = 0x00,
      Stagenet1 = 0x51,
      Testnet1 = 0xa1,
      Testnet2 = 0xa2
   }
```
### P2P message version
Peer to Peer messages on the Tari network are encapsulated into message envelopes. The body of message envelopes are 
defined, serialized and deserialized using Protobuf. These messages will only be updated by adding new fields to the 
Protobuf definitions, never removing fields. This is done in order to preserve backwards compatibility where newer nodes 
can still communicate with older nodes. 

The P2P messaging protocol will see many changes in its lifetime. Some will be minor changes that are fully backwards 
compatible and some changes will be breaking where older nodes will not be able to communicate with newer nodes. In 
order to document these two classes of changes each P2P message header will contain a `version` field that will use
a two-part semantic versioning scheme with the format of `major.minor` integer versions. The `minor` version will be 
incremented whenever there is any change. The `major` version be incremented when there is a breaking change made to 
the P2P protocol. Each integer can be stored separately.

### Consensus version
The final aspect of the Tari system that will be versioned are the Consensus rules. These rules will change as the 
network matures. Changes to consensus rules can be achieved using either a Soft fork or Hard fork. Soft forks are where 
new consensus rules are added that older nodes will see as valid (thus backwards compatible) but newer nodes will reject 
blocks from older nodes that are not aware of the new consensus rules. A hard fork means that the new consensus rules 
are not backwards compatible and so only nodes that know about the new rules will be able to produce and validate new 
transactions and blocks.

The consensus version will be used by a node to determine if it can interact with another node successfully or not. A 
list of fork versions will be maintained within the code. When a connection is started with a new node the two nodes 
will exchange `Version` messages detailing the consensus version they are each running and the blockheight at which they
are currently operating. Both nodes will need to reply with a `Version Acknowledge` message to confirm that they are 
compatible with the counterparty's version. It is possible for a newer node to downgrade its protocol to speak to an 
older node so this must be decided during this handshake process. Only once the acknowledgments have been exchanged can 
further messages be exchanged by the parties. This is the method currently employed on the 
[Bitcoin network](https://developer.bitcoin.org/devguide/p2p_network.html#connecting-to-peers)

For example, if we have two nodes, Node A and Node B, where Node A is ahead of Node B in version and block height. 
During the handshake Node B will not recognize Node A's version but should wait for Node A to reject or confirm the
connection because Node A could potentially downgrade their version to match Node B's. Node A will speak to Node B if
and only if Node A recognizes Node B's version and Node B's block height is in the correct range for its advertised 
version according to Node A's fork version list.

Tari Block Headers contain a `version` field which will be used to indicate the version of consensus rules that are 
used in the construction and validation of this block. Consensus rules versions will only consist of breaking changes
and as such will be represented with a single incremented integer. This coupled with the internal list of fork versions,
that includes the height at which they came into effect, will be used to validate whether the consensus rules specified 
in the header are valid for that block's height.
