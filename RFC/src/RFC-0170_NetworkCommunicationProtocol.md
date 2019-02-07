# RFC-0170: Network Communication Protocol

## The Tari Communication Network and Network Communication Protocol

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Yuko Roodt] (https://github.com/neonknight64)

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

This document will introduce the Tari communication network and the communication protocol used to select, establish and maintain connections with peers on the network.
Consensus Nodes and Consensus clients will be introduced and their required functionality will be proposed.

## Related RFCs

* [RFC-0100: The Tari Base Layer](RFC-0100_BaseLayer.md)
* [RFC-0300: Digital asset network](RFC-0300_DAN.md)

## Description

### Assumptions

- A communication channel can be established between two peers once their online communication addresses are known to each other.
- A Validator Node is able to obtain a node id from the registration process on the Base Layer.

### Abstract

The Tari communication network consists of a large number of nodes that maintained peer connections between each other.
These nodes forward and propagate encrypted and unencrypted data messages through the network such as joining requests, discovery requests and completed blocks.
Network clients, not responsible for maintaining the network, are able to create ad hoc connections with nodes on the network to perform joining and discovery requests.
The majority of communication between clients and nodes will be performed using direct P2P communication once the discovery process was used to obtain the online communication addresses of peers.
Where possible the efficient Kademlia based directed forwarding of encrypted data messages can be used to perform quick node discovery and joining of clients and nodes on the Tari communication network. 
Where messages are of importance to a wide variety of entities on the network, Gossip protocol based message propagation can be performed to distribute the message to the entire network.

### Overview

The Tari communication network is a variant of a Kademlia network that allows for fast discovery of nodes, with an added ability to perform gossip protocol based broadcasting of data messages to the entire network.
The majority of the communication required on the Base Layer and DAN will be performed via direct Peer-to-peer (P2P) communication between known clients and nodes.
Alternatively, the Tari communication network can be used for broadcasting joining requests, discovery requests and propagating data messages such as completed blocks, transactions and messages that are of interest to a large part of the Tari communication network. 

The Tari communication network consist of a number of different entities that need to be able to communicate in a distributed and ad-hoc manner. 
The primary entities that need to communicate are Validator Nodes, Base Nodes, Wallets and Asset Managers.
Here are some examples of different communication tasks that need to be performed by these entities on the Tari Communication network:
- Base Nodes on the Base layer need to propagate completed blocks and transactions to other Base Nodes using Gossip protocol based broadcasting.
- Validator Nodes need to efficiently discover other Validator Nodes in the process of forming Validator Node committees. 
- Wallets need to communicate and negotiate with other Wallets to create transactions. They also need the ability to submit transactions to the mempool of Base Nodes.
- Asset Managers need to communicate with Validator Node committees and other Asset Managers to construct and send DAN instructions.

Here is communication matrix that represents what source entities need to communicate with other destination entities on the Tari Communication network:
| Source \ Destination            | Validator Node | Base Node | Wallet | Asset Manager|
|---             |---             |---        |---     |---           |
| Validator Node | Yes            | Yes       | Yes    | Yes          |
| Base Node      | No             | Yes       | No     | No           |
| Wallet         | No             | Yes       | Yes    | No           |
| Asset Manager  | Yes            | No        | Yes    | Yes          |


#### Consensus Nodes and Consensus Clients

To simplify the description of the Tari communication network, the different entities with similar behaviour were grouped into two groups: Consensus Nodes and Consensus Clients.
Validator Nodes and Base Nodes are Consensus Nodes(CN).
Wallets and Asset Managers are Communication Clients(CC).
CNs form the core communication infrastructure of the Tari communication network and is responsible for maintaining the Tari communication network by receiving, forwarding and distributing joining requests, discovery requests, data messages and routing information.
CCs are different to CNs in that they do not maintain the network and they are not responsible for propagating any joining requests, discovery requests, data messages and routing information.
They do make use of the network to submit their own joining requests and perform discovery request of other specific CNs and CCs when they need to communicate with them.
Once the CC has discovered the CC or CN they want to communicate with, they will establish a direct P2P channel with them.
The Tari communication network is unaware of this communication, once discovery was completed.

The different entity types were grouped into the different communication node types as follows:
| Entity Type    | Communication Node Type |
|---             |---                      |
| Validator Node | Consensus Node          |
| Base Node      | Consensus Node          |
| Wallet         | Consensus Client        |
| Asset Manager  | Consensus Client        |


#### Unique identification of Consensus Nodes and Consensus Clients 

In the Tari communication network, each CN or CC makes use of a node id to determine their position in the network.
This node id is either assigned based on registration on the base layer or can be derived from the CNs or CCs identification public key.
The method used to obtain a node id will either enhance or limit the trustworthiness of that entity on the Tari communication network when messages are propagated.
When performing broadcasting of data messages or propagating discovery requests, nodes with registration assigned node ids are considered more trustworthy compared to nodes with derived node ids. 

Obtaining a node id from registration on the base layer is important as it will limit the potential of some parties performing Eclipse attacks on the network.
Registration makes it more difficult for Bad Actors to position themselves in ideal patterns on the network to perform disruptive operations and actions. 
Also, in sensitive situations or situations where the Kademlia style directed propagation of messages are vulnerable then gossip protocol based broadcasting of messages can be performed as a less efficient, but safer alternative to ensure that the message will successfully reach the network.

The recommended method of node id assignment for each Tari communication network entity type are as follows:

| Entity Type    | Communication Node Type | Node ID Assignment |
|---             |---                      |---                 |
| Validator Node | Consensus Node          | Registration       |
| Base Node      | Consensus Node          | Derived            |
| Wallet         | Consensus Client        | Derived            |
| Asset Manager  | Consensus Client        | Derived            |

Note that Mining Servers and Mining Workers are excluded from the Tari communication network.
A Mining Worker will have a local or remote P2P connection with a Mining Server.
A Mining Server will have a local or remote P2P connection with a Base Node.
They are not responsible for propagating any messages on the network, when needed the parent Base Node will perform these tasks on their behalf.


#### Online Communication Address, Peer Address and Routing Table

Each CC and CN on the Tari communication network will have an identification cryptographic key and an online communication address.
The online communication address can be either an IPv4, IPv6, URL, Tor(Base32) or I2P(Base32) address and can be stored using the network address type as follows:

| Description  | Data type  | Comments                                     |
|---           |---         |---                                           |
| address type | uint4      | Specify if IPv4/IPv6/Url/Tor/I2P             |
| address      | char array | IPv4, IPv6, Tor(Base32), I2P(Base32) address |
| port         | uint16     | port number                                  |

The address type can used to determine how to interpret the address chars. An I2P address can be interpreted as "{52 address chars}.b32.i2p". The Tor address should be interpreted as "http://{16 or 52 address chars}.onion/". The IPv4 and IPv6 address can be stored in the address field without modification. URL addresses can be used for nodes with dynamic IPs. 

Each CC or CN has a local routing table that contains the online communication addresses of known CCs and CNs on the Tari communication network.
When a CC or CN wants to join the Tari communication network, the online communication address of at least one CN that is part of the network needs to be know.
The online communication address of the initial CN can either be manually provided or a bootstrapped list of "reliable" and persistent CNs can be provided with the Validator Node, Base Node, Wallet or Asset Manager software.
The new CC or CN can then request additional contact information of other CNs from the initial peers to extend the new CC's or CN's routing table.

The routing table consist of a list of peer addresses that link node ids, public identification keys and online communication address of each know CCs and CNs.

The Peer Address can be implemented as follows:

| Description      | Data type          | Comments                                                                 |
|---               |---                 |---                                                                       |
| network address  | network_address    | The online communication address of the CC or CN                         |
| node_id          | node_id            | Registration Assigned for VN, Self selected for BN, W and AM             |
| public_key       | public_key         | The public key of the identification cryptographic key of the CC or CN   |
| node_type        | node_type          | VN, BN, W or AM                                                          |
| linked asset ids | list of asset ids  | Asset ids can be used as an address on Tari network similar to a node id |
| last_connection  | timestamp          | Time of last successful connection with peer                             |

When a new CCs and CNs want to join the network they will submit joining requests to the network.
The joining request contains the peer address of the new CC or CN.
Each CN that receives the joining request can decide if they want to add the new CCs or CNs contact information to their local routing table.
When a CN that received the joining request has a similar node id to the new CC or CN then he must add the peer address to his routing table.

To limit potential attacks, only one registration with the same online communication address can be stored in the routing table of a CN.
This restriction will limit Bad Actors from spinning up multiple CNs on a single computer.

#### Joining the network using a joining request

A new CC or CN needs to register their peer address on the Tari communication network.
This is achieved by submitting a network joining request to a subset of CNs from the new CCs or CNs routing table.
These peers will forward the joining request with the peer address to the rest of the network until CNs with similar node ids have been reached.
CN with similar node ids will then add the peer address of the new node to their routing table. 

Other CCs and CNs will then be able to retrieve the new CCs or CNs peer address by submitting a discovery request.
Once the peer address of the desired CC or CN has been discovered then a direct P2P communication channel can be established between the two parties for any future communication.
After discovery, the rest of the Tari communication network will be unaware of any further communication between the two parties.


#### Sending data messages and discovery requests

The majority of all communication on the Tari communication network will be performed using direct P2P channels established between different CCs and CNs once they are aware of each others peer addresses that contain their online communication addresses.
Message propagation on the network will typically consist only of joining and discovery requests where a CC or CN wants to join the network or retrieve the peer address of another CC or CN so a direct P2P channel can be established.

Messages can be transmitted in this network in either an unencrypted or encrypted form.
Typically messages that have been sent in unencrypted form is of interest to a number of CNs on the network and should be propagated so that every CN that is interested in that data message obtains a copy.
Block and Transaction propagation are example of data messages where multiple entities on the Tari communication network are interested in that data message, this requires propagation through the entire Tari communication network in unencrypted form.

Encrypted data messages makes use of the source and destination's identification cryptographic keys to construct a shared secret with which the message can be encoded and decoded.
This ensures that only the two parties are able to decode the data message as it is propagated through the communication network.
This mechanism can be used to perform private discovery requests, where the online communication address of the source node is encrypted and propagated through the network until it reached the destination node.
Propagation of the discovery request can be performed as a broadcast using a gossip protocol or in a more efficient directed propagation using the Kademlia protocol.
As encrypted message tend to not be of interest to the rest of the network, directed propagation using the Kademlia protocol to forward these messages to the correct parties are preferred.

Encryption of the data message ensuring that only the destination node is able to view the online address of the source node as the data message moves through the network.
Once the destination node receives and decrypts the data message, that node is then able to establish a P2P communication channel with the source node for any further communication.

This same encryption technique can be used to send encrypted messages to a single node or a group of nodes, where the group of nodes have a shared identification key.
A Validation Committee is an example of a group of CNs that have a shared identification key, ensuring that all members of that committee are able to receive and decrypt data messages that were sent to them.

#### Maintaining connections with peers

CCs and CNs establish and maintain connections with peers differently.
CCs only create a few short lived ad hoc channels and CNs create and maintain long lived channels with a number of peers.

If a CC is unaware of a destination CN's or CC's online communication address then the address first needs to be obtained using a discovery request.
When a CC already know the communication address of the CC or CN that he wants to communicate with, then a direct P2P channel can be established between the two peers for the duration of the communication task.
The communication channel can then be closed as soon as the communication task has been completed.

CNs consisting of VNs and BNs typically attempt to maintain communication channels with a large number of peers.
The distribution of peers (VNs vs BNs) that a single CN keeps communication channels open with can change depending on the type of node that it is.
A CN that is also a BN should maintain more peer connections with other BNs, but should also have some connections with other VNs.

Having some connections with VNs are important as BNs have derived node ids and not registered node ids such as VNs, making it possible for the CN to be separated from the main network and an eclipse attack to be performed.
Having some connections with VNs will make it more difficult to separate that CN from the network and will ensure successful propagation of transactions and completed blocks. 

A CN that is also a VN should maintain more peer connections with other VNs, but also have some connections with BNs.
CNs that are part of Validator committees should attempt to maintain permanent connections with the other committee members to ensure that quick consensus can be achieved.

To maintaining a connection with a peer the following process can be performed.
Once some peers have been found using discovery requests, resulting in them successfully being added to the local routing table.
The CN can decide how the peer connections should be selected by either:
 - manually selecting a subset,
 - automatically selecting a random subset or
 - selecting a subset of neighbouring nodes with similar node ids. 

 Maintaining communication channels are important and the following process can be followed in an attempt to keep peer connections alive:
When more than 30 minutes has passed since the last communication with a peer, then a heartbeat message should be send to that peer in an attempt to keep the connection with the peer alive.
If that specific peer connection is not important then a new peer can be selected from the local routing table.
A new connection can then be established and maintained between the current node and the newly selected node.
If that specific connection is important, such as with the connections between committee members, then the current CN or CC must wait and attempt to create a new connection with that same peer.
If more than 90 minutes have passed since the last successful communication with the peer node, a new discovery request can be sent on the Tari communication network in an attempt to locate that peer again.
Losing of a peer might happen in cases where the CN or CC went temporarily offline and their dynamic communication addresses changed, requiring the discovery process to be performed again before a direct P2P communication channel can be established.

Functionality required of communication network:
- It MUST be able to send a directed message through the network to a entity where the entities Node ID is known but not their communication address.

- An entity on the CN must store all received data and routing information for that data, where the data id is similar to the node id of the entity.
- An entity that has received responsibility for managing an asset or specific data must request that all entities with similar node ids to the data id should add the that entities communication information in their routing table. 
- When an entity receives an unencrypted message that requires gossip based broadcasting then the entity must forward the message to all connected peers. 
- When an entity receives an encrypted message that requires directional forwarding or directed broadcasting the entity MUST forward the message to all entities in his routing table that are closer.
- If no other entities was found in the local routing table that had a closer nod id compared to the current entities node id then the encrypted message must be stored for set period of time. 


#### Functionality required of Consensus Nodes
- A Consensus Nodes MUST have the ability to connect and maintain connections with a set of peers.
- The MUST have the ability to broadcast messages to a set of self selected peers.


#### Functionality required of Consensus Clients
Registration
- It MUST select a cryptographic keypair used for identifying the client on the Tari Communication network 
- It MUST have a mechanism to derive a node_id from the self selected public key
- A new Consensus client MUST broadcast an add peer request to the Tari communication network so that its routing information can be stored by the Consensus nodes with similar node_ids.

Normal behavior
- A Consensus Client MUST maintain a small list of peers on the Tari Communication network.
- As the Consensus Client becomes aware of other Consensus Nodes and Clients on the communication network he SHOULD extend his routing table by including the newly discovered Node/Clients contact information.
- Peers from the Consensus Clients routing table that have been unreachable for a number of attempts SHOULD be removed from the routing table.







3 ways of propagating messages?
- Send message to single other node.
    Example: Such as a wallet sending a message to a VN
    Reason: Wallet do not track or have knowledge of many peers, so it asks a VN to propagate his message through the network
- Broadcast message to all connected peers or random subset of peers in routing table.
    Example: Used by base layer nodes to propagate blocks to other base layer nodes.
    Reason: BNs self select their node ids making them vulnerable to eclipse attacks, it is safer for them to broadcast to as many as possible peers to ensure that message will be successfully propagated. Network propagation on base layer can be slow as long as it is faster than the block time of the blockchain.
- Broadcast message to nodes in routing table that are closer to the destination
    Example: VN sends a directed message to other VNs that in-turn propagate it only to VNs that are even closer.
    Reason: A directed propagation ensures much quicker propagation of the message to the destination. It also reduces network traffic. This type of directed broadcast can only happen between VNs as their node ids are assigned during the Validator Node registration process making them less vulnerable to an eclipse attack.


Types of messages:






-



