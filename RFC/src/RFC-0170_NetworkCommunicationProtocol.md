# RFC-0170/NetworkCommunicationProtocol

## The Tari Communication Network and Network Communication Protocol

![status: Outdated](theme/images/status-outofdate.svg)

**Maintainer(s)**: [Yuko Roodt](https://github.com/neonknight64)

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

This document will introduce the Tari communication network and the communication protocol used to select, establish and maintain connections between peers on the network.
[Communication Node]s and [Communication Client]s will be introduced and their required functionality will be proposed.

## Related RFCs

* [RFC-0100: The Tari Base Layer](RFC-0100_BaseLayer.md)
* [RFC-0300: Digital asset network](RFCD-0300_DAN.md)

## Description

### Assumptions

- A communication channel can be established between two peers once their online communication addresses are known to each other.
- A [Validator Node] is able to derive a [node ID] from the Validator Node registration transaction on the Base Layer.

### Abstract

The backbone of the Tari communication network consists of a large number of nodes that maintain peer connections between each other.
These nodes forward and propagate encrypted and unencrypted data messages through the network such as joining requests, discovery requests, [transaction]s and completed [block]s.
Network clients, not responsible for maintaining the network, are able to create ad hoc connections with nodes on the network to perform joining and discovery requests.
The majority of communication between clients and nodes will be performed using direct Peer-to-peer (P2P) communication once the discovery process was used to obtain the online communication addresses of peers.
Where possible the efficient Kademlia based directed forwarding of encrypted data messages can be used to perform quick node discovery and joining of clients and nodes on the Tari communication network. 
Where messages are of importance to a wide variety of entities on the network, Gossip protocol based message propagation can be performed to distribute the message to the entire network.

### Overview

The Tari communication network is a variant of a [Kademlia](https://en.wikipedia.org/wiki/Kademlia) network that allows for fast discovery of nodes, with an added ability to perform Gossip protocol based broadcasting of data messages to the entire network.
The majority of the communication required on the [Base Layer] and [Digital Asset Network] (DAN) will be performed via direct P2P communication between known clients and nodes.
Alternatively, the Tari communication network can be used for broadcasting joining requests, discovery requests and propagating data messages such as completed blocks, transactions and data messages that are of interest to a large part of the Tari communication network. 

The Tari communication network consists of a number of different entities that need to communicate in a distributed and ad-hoc manner. 
The primary entities that need to communicate are Validator Nodes (VN), [Base Node]s (BN), [Wallet]s (W) and [Token Wallet]s (TW).
Here are some examples of different communication tasks that need to be performed by these entities on the Tari Communication network:
- Base Nodes on the Base Layer need to propagate completed blocks and transactions to other Base Nodes using Gossip protocol based broadcasting.
- Validator Nodes need to efficiently discover other Validator Nodes in the process of forming Validator Node committees. 
- Wallets need to communicate and negotiate with other Wallets to create transactions. They also need the ability to submit transactions to the [mempool] of Base Nodes.
- Token Wallets need to communicate with Validator Node [committee]s and other Token Wallets to construct and send DAN instructions.

Here is an overview communication matrix that show which source entities SHOULD initiate communication with destination entities on the Tari Communication network:

| &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;\Destination<br>Source | Validator Node | Base Node | Wallet | Token Wallet |
|---                     |---             |---        |---     |---           |
| Validator Node         | Yes            | Yes       | Yes    | Yes          |
| Base Node              | No             | Yes       | No     | No           |
| Wallet                 | No             | Yes       | Yes    | No           |
| Token Wallet           | Yes            | No        | Yes    | Yes          |

#### Communication Nodes and Communication Clients

To simplify the description of the Tari communication network, the different entities with similar behaviour were grouped into two groups: Communication Nodes and Communication Clients.
Validator Nodes and Base Nodes are Communication Nodes (CN).
Wallets and Token Wallets are Communication Clients (CC).
CNs form the core communication infrastructure of the Tari communication network and are responsible for maintaining the Tari communication network by receiving, forwarding and distributing joining requests, discovery requests, data messages and routing information.
CCs are different from CNs in that they do not maintain the network and they are not responsible for propagating any joining requests, discovery requests, data messages and routing information.
They do make use of the network to submit their own joining requests and perform discovery request of other specific CNs and CCs when they need to communicate with them.
Once a CC has discovered the CC or CN they want to communicate with, they will establish a direct P2P channel with them.
The Tari communication network is unaware of this direct P2P communication once discovery is completed.

The different entity types MUST be grouped into the different communication node types as follows:

| Entity Type    | Communication Node Type |
|---             |---                      |
| Validator Node | Communication Node      |
| Base Node      | Communication Node      |
| Wallet         | Communication Client    |
| Token Wallet   | Communication Client    |

#### Unique identification of Communication Nodes and Communication Clients 

In the Tari communication network, each CN or CC makes use of a node ID to determine their position in the network.
This node ID is either assigned based on registration on the Base Layer or can be derived from the CNs or CCs identification public key.
The method used to obtain a node ID will either enhance or limit the trustworthiness of that entity when propagating messages through them on the Tari communication network.
When performing the broadcasting of data messages or propagating discovery requests, nodes with registration assigned node IDs are considered more trustworthy compared to nodes with derived node IDs. 

Obtaining a node ID from registration on the Base Layer is important as it will limit the potential of some parties performing [Eclipse attacks](https://eprint.iacr.org/2015/263.pdf) on the network.
Registration makes it more difficult for [Bad Actor]s to position themselves in ideal patterns on the network to perform disruptive operations and actions. 
In sensitive situations or situations where the Kademlia-style directed propagation of messages are vulnerable, gossip protocol-based broadcasting of messages can be performed as a less efficient, but safer alternative to ensure that the message will successfully reach the rest of the network.

The similarity or distance between different node IDs can be calculated by performing the [Hamming distance](https://en.wikipedia.org/wiki/Hamming_distance) between the bits of the two node ID numbers.
The Hamming distance can be implemented as an Exclusive OR (XOR) between the bits of the numbers and the summation of the resulting true bits.
CCs and/or CNs that have similar node IDs, that produce a small Hamming distance, are located in similar regions of the Tari communication network.
This does not mean that their geographic locations are near each other, but rather that their location in the network is similar.
A thresholding scheme can be applied to the Hamming distance to ensure that only neighboring CNs with similar node IDs are allowed to share and propagate specific information.
As an example, only routing table information that contains similar node IDs to the requesting CCs or CNs node ID should be shared with them.
Limiting the sharing of routing table information makes it more difficult to map the entire Tari communication network.

The recommended method of node ID assignment for each Tari communication network entity type MUST be implemented as follows:

| Entity Type    | Communication Node Type | Node ID Assignment |
|---             |---                      |---                 |
| Validator Node | Communication Node      | Registration       |
| Base Node      | Communication Node      | Derived            |
| Wallet         | Communication Client    | Derived            |
| Token Wallet   | Communication Client    | Derived            |

Note that [Mining Server]s and [Mining Worker]s are excluded from the Tari communication network.
A Mining Worker will have a local or remote P2P connection with a Mining Server.
A Mining Server will have a local or remote P2P connection with a Base Node.
They do not need to make use of the communication network and they are not responsible for propagating any messages on the network.
The parent Base Node will perform any communication tasks on the Tari communication network on their behalf.

#### Online Communication Address, Peer Address and Routing Table

Each CC and CN on the Tari communication network will have identification cryptographic keys, a node ID and an online communication address.
The online communication address SHOULD be either an IPv4, IPv6, URL, Tor (Base32) or I2P (Base32) address and can be stored using the network address type as follows:

| Description  | Data type  | Comments                                            |
|:-------------|:-----------|:----------------------------------------------------|
| address type | uint4      | Specify if IPv4/IPv6/Url/Tor/I2P                    |
| address      | char array | IPv4, IPv6, URL, Tor (Base32), I2P (Base32) address |
| port         | uint16     | port number                                         |

The address type is used to determine how to interpret the address characters. An I2P address can be interpreted as "{52
address characters}.b32.i2p". The Tor address should be interpreted as "http://{16_or_52_address_chars}.onion/". The
IPv4 and IPv6 address can be stored in the address field without modification. URL addresses can be used for nodes with
dynamic IP addresses.

A Tor or I2P address can be used when anonymity is important for a CC or CN.
The IPv4, IPv6 and URL address types do not provide any privacy features but do provide increased bandwidth. 

Each CC or CN has a local routing table that contains the online communication addresses of all CCs and CNs on the Tari communication network known to that CC or CN.
When a CC or CN wants to join the Tari communication network, the online communication address of at least one other CN that is part of the network needs to be known.
The online communication address of the initial CN can either be manually provided or a bootstrapped list of "reliable" and persistent CNs can be provided with the Validator Node, Base Node, Wallet or Token Wallet software.
The new CC or CN can then request additional peer contact information of other CNs from the initial peers to extend their own routing table.

The routing table consists of a list of peer addresses that link node IDs, public identification keys and online communication addresses of each known CC and CN.

The Peer Address stored in the routing table MAY be implemented as follows:

| Description      | Data type         | Comments                                                                 |
|:-----------------|:------------------|:-------------------------------------------------------------------------|
| network address  | network_address   | The online communication address of the CC or CN                         |
| node_ID          | node_ID           | Registration Assigned for VN, Self selected for BN, W and TW             |
| public_key       | public_key        | The public key of the identification cryptographic key of the CC or CN   |
| node_type        | node_type         | VN, BN, W or TW                                                          |
| linked asset IDs | list of asset IDs | Asset IDs can be used as an address on Tari network similar to a node ID |
| last_connection  | timestamp         | Time of last successful connection with peer                             |
| update_timestamp | timestamp         | A timestamp for the last peer address update                             |

When a new CC or CN wants to join the Tari communication network they need to submit a joining request to the rest of the network.
The joining request contains the peer address of the new CC or CN.
Each CN that receives the joining request can decide if they want to add the new CCs or CNs contact information to their local routing table.
When a CN, that received the joining request, has a similar node ID to the new CC or CN then that node must add the peer address to their routing table.
All CNs with similar node IDs to the new CC or CN should have a copy of the new peer address in their routing tables.

To limit potential attacks, only one registration for a specific node type with the same online communication address can be stored in the routing table of a CN.
This restriction will limit Bad Actors from spinning up multiple CNs on a single computer.

#### Joining the Network using a Joining Request

A new CC or CN needs to register their peer address on the Tari communication network.
This is achieved by submitting a network joining request to a subset of CNs selected from the routing table of the new CC or CN.
These peers will forward the joining request, with the peer address, to the rest of the network until CNs with similar node IDs have been reached.
CNs with similar node IDs will then add the new peer address of the new node to their routing table, allowing for fast discovery of the new CC or CN. 

Other CCs and CNs will then be able to retrieve the new CCs or CNs peer address by submitting discovery requests.
Once the peer address of the desired CC or CN has been discovered then a direct P2P communication channel can be established between the two parties for any future communication.
After discovery, the rest of the Tari communication network will be unaware of any further communication between the two parties.

#### Sending Data Messages and Discovery Requests

The majority of all communication on the Tari communication network will be performed using direct P2P channels established between different CCs and CNs once they are aware of the peer addresses of each other that contain their online communication addresses.
Message propagation on the network will typically consist only of joining and discovery requests where a CC or CN wants to join the network or retrieve the peer address of another CC or CN so that a direct P2P channel can be established.

Messages can be transmitted in this network in either an unencrypted or encrypted form.
Typically messages that have been sent in unencrypted form are of interest to a number of CNs on the network and should be propagated so that every CN that is interested in that data message obtains a copy.
Block and Transaction propagation are examples of data messages where multiple entities on the Tari communication network are interested in that data message; this requires propagation through the entire Tari communication network in unencrypted form.

Encrypted data messages make use of the source and destinations identification cryptographic keys to construct a shared secret with which the message can be encoded and decoded.
This ensures that only the two parties are able to decode the data message as it is propagated through the communication network.
This mechanism can be used to perform private discovery requests, where the online communication address of the source node is encrypted and propagated through the network until it reached the destination node.
Private discovery requests can only be performed if both parties are online at the same time.
Encryption of the data message ensures that only the destination node is able to view the online address of the source node as the data message moves through the network.
Once the destination node receives and decrypts the data message, that node is then able to establish a P2P communication channel with the source node for any further communication.

Propagation of completely private discovery request, hidden as an encrypted data message, can be performed as a broadcast through the entire network using the Gossip protocol.
Propagation of public discovery requests can be performed using more efficient directed propagation using the Kademlia protocol.
As encrypted message with visible destinations tend to not be of interest to the rest of the network, directed propagation using the Kademlia protocol to forward these messages to the correct parties are preferred.
Privacy of a CCs online address, whom is sending a transaction to a Base Node, may be enhanced if the transaction is encrypted and sent to a Base Node with a node ID that is not the closest to the CCs node ID. This should prevent linking a transaction to the originating online address.

This same encryption technique can be used to send encrypted messages to a single node or a group of nodes, where the group of nodes have shared identification keys.
A Validation Committee is an example of a group of CNs that have shared identification keys for the committee.
The shared identification keys ensure that all members of that committee are able to receive and decrypt data messages that were sent to the committee.

#### Maintaining connections with peers

CCs and CNs establish and maintain connections with peers differently.
CCs only create a few short-lived ad hoc channels and CNs create and maintain long-lived channels with a number of peers.

If a CC is unaware of a destination CNs or CCs online communication address then the address first needs to be obtained using a discovery request.
When a CC already knows the communication address of the CC or CN that he wants to communicate with, then a direct P2P channel can be established between the two peers for the duration of the communication task.
The communication channel can then be closed as soon as the communication task has been completed.

CNs consisting of VNs and BNs typically attempt to maintain communication channels with a large number of peers.
The distribution of peers (VNs vs BNs) that a single CN keeps communication channels open with can change depending on the type of node.
A CN that is also a BN should maintain more peer connections with other BNs, but should also have some connections with other VNs.

Having some connections with VNs are important as BNs have derived node IDs and not registered node IDs such as VNs, making it possible for the CN to be separated from the main network and become victim to an [Eclipse attack](https://www.radixdlt.com/post/what-is-an-eclipse-attack).
Having some connections with VNs will make it more difficult to separate the CN from the network and will ensure successful propagation of transactions and completed blocks from that CN. 

A CN that is also a VN should maintain more peer connections with other VNs, but also have some connections with BNs.
CNs that are part of Validator Node committees should attempt to maintain permanent connections with the other members of the committee to ensure that quick consensus can be achieved.

To maintain connections with peers, the following process can be performed.
Discover peers using discovery requests, and add their details to the local routing table.
The CN can decide how the peer connections should be selected from the routing table by either:
 - manually selecting a subset,
 - automatically selecting a random subset or
 - selecting a subset of neighbouring nodes with similar node IDs. 

Maintaining communication channels is important and the following process can be followed in an attempt to keep peer connections alive:
For an existing peer connection.
When more than 30 minutes have passed since the last communication with that peer, then a heartbeat message should be sent to the peer in an attempt to keep the connection with the peer alive.
If the peer connection was not established for a specific purpose, such as with the connections between committee members, then a new replacement peer can be selected from the local routing table.
A new connection can then be established and maintained between the current node and the newly selected node.
If that specific connection is important, such as with the connections between committee members, then the current CN or CC must wait and attempt to create a new connection with that same peer.
If more than 90 minutes have passed since the last successful communication with the peer node, a new discovery request can be sent on the Tari communication network in an attempt to locate that peer again.
Losing of a peer might happen in cases where the CN or CC went temporarily offline and their dynamic communication address changed, requiring the discovery process to be performed again before a direct P2P communication channel can be established.

#### Functionality Required of Communication Nodes

- It MUST select a cryptographic key pair used for identification on the Tari Communication network.
- A CN MAY request the peer addresses of CNs with similar node IDs from other CNs to extend their local routing table. 
- If a CN is a VN, then a node ID MUST be obtained by registering on the Base layer.
- If a CN is a BN, then a node ID MUST be derived from the nodes identification public key.
- A new CN MUST submit a joining request to the Tari communication network so that the nodes peer address can be added to the routing table of neighbouring peers in the network.
- If a CN receives a new joining request with a similar node ID (within a network selected threshold), then the peer address specified in the joining request MUST be added to its local routing table.
- When a CN receives an encrypted message, the node MUST attempt to open the message.
- When a CN receives an encrypted message that the node is unable to open, and the destination node ID is known then the CN MUST forward it to all connected peers that have node IDs that are closer to the destination.
- When a CN receives an encrypted message that the node is unable to open and the destination node is unknown then the CN MUST forward the message to all connected peers.
- A CN MUST have the ability to verify the content of unencrypted messages to limit the propagation of spam messages.
- If an unencrypted message is received by the CN with a unspecified destination node ID, then the node MUST verify the content of the message and forward the message to all connected peers.
- If an unencrypted message is received by the CN with an specified destination node ID, then the node MUST verify the content of the message and forward the message to all connected peers that have closer node IDs.
- A CN MUST have the ability to select a set of peer connections from its routing table.
- Connections with the selected set of peers MUST be maintained by the CN.
- A CN MUST have a mechanism to construct encrypted and unencrypted joining requests, discovery requests or data messages.
- A CN MUST construct and provide a list of peer addresses from its routing table that is similar to a requested node ID so that other CCs and CNs can extend their routing tables.
- A CN MUST keep its routing table up to date by removing unreachable peer addresses and adding newly received addresses.
- It MUST have a mechanism to determine if a node ID was obtained through registration or was derived from an identification public key.
- A CN MUST calculate the similarity between different node IDs by calculating the Hamming distance between the bits of the two node ID numbers.

#### Functionality Required of Communication Clients

- It MUST select a cryptographic key pair used for identification on the Tari Communication network.
- It MUST have a mechanism to derive a node ID from the self-selected identification public key.
- A CC must have the ability to construct a peer address that links its identification public key, node ID and an online communication address.
- A new CC MUST broadcast a joining request with its peer address to the Tari communication network so that CNs with similar node IDs can add the peer address of the new CC to their routing tables.
- A CC MAY request the peer addresses of CNs with similar node IDs from other CNs to extend their local routing table.
- A CC MUST have a mechanism to construct encrypted and unencrypted joining and discovery requests.
- A CC MUST maintain a small persistent routing table of Tari Communication network peers with which ad hoc connections can be established.
- As the CC becomes aware of other CNs and CCs on the communication network, the CC SHOULD extend its local routing table by including the newly discovered CCs or CNs contact information.
- Peers from the CCs routing table that have been unreachable for a number of attempts SHOULD be removed from the its routing table.
- A CC MUST calculate the similarity between different node IDs by calculating the Hamming distance between the bits of the two node ID numbers.

[communication node]: Glossary.md#communication-node
[communication client]: Glossary.md#communication-client
[validator node]: Glossary.md#validator-node
[node id]: Glossary.md#node-id
[transaction]: Glossary.md#transaction
[block]: Glossary.md#block
[base layer]: Glossary.md#base-layer
[digital asset network]: Glossary.md#digital-asset-network
[base node]: Glossary.md#base-node
[wallet]: Glossary.md#wallet
[token wallet]: Glossary.md#token-wallet
[mempool]: Glossary.md#mempool
[committee]: Glossary.md#committee
[Bad Actor]: Glossary.md#bad-actor
[mining server]: Glossary.md#mining-server
[mining worker]: Glossary.md#mining-worker