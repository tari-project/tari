# RFC-0170: Network Communication Protocol

## Network Communication Protocol on base layer

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

## Related RFCs

## Description

### Assumptions

### Abstract

### Overview

The Tari communication network is a variant of a Kademlia network that allows fast discovery of nodes, with an added ability to perform gossip protocol based broadcasting of messages to the entire network. The network is primarily used for broadcasting of messages and node discovery. Once a desired node has been discovered, the contact information of that node can be stored in the local routing table and a direct Peer-to-peer (P2P) communication channel can be established between the two nodes for future communication.

The Tari communication network consist of a number of different entities that need to be able to communicate in a distributed and ad-hoc manner. 
The primary entities that need to communicate are Validator Nodes, Base Nodes, Wallets and Asset Managers.
Here are some examples of some of the communication task that need to be performed by these entities on the Tari Communication network:
- Base Nodes on the base layer need to propagate completed blocks and transactions to other Base Nodes using gossip protocol based broadcasting.
- Validator Nodes need to efficiently discover other specific Validator Nodes in the process of forming Validator Node committees. 
- Wallets need to communicate with other Wallets and Base Nodes to create Base Layer Transactions.
- Asset Managers need to communicate with Validator Node committees and other Asset Managers to construct and send instructions.

//TODO Communication Matrix

|                | Validator Node | Base Node | Wallet | Asset Manager|
|---             |---             |---        |---     |---           |
| Validator Node |                |           |        |              |
| Base Node      |                |           |        |              |
| Wallet         |                |           |        |              |
| Asset Manager  |                |           |        |              |


#### Consensus Nodes and Consensus Clients

These Tari communication network entities can be divided into two groups: Consensus Nodes and Consensus Clients.
Validator Nodes and Base Nodes are Consensus Nodes(CN), where Wallets and Asset Managers are Communication Clients(CC).
CNs form the core part of the Tari network and is responsible for maintaining the Tari communication network by receiving, forwarding and distributing discovery and data messages, and routing information on the Tari communication network.
CNs are not responsible for propagating any data messages or routing information, they only make use of the network to perform discovery tasks and send data messages.

| Entity Type    | Communication Node Type |
|---             |---                      |
| Validator Node | Consensus Node          |
| Base Node      | Consensus Node          |
| Wallet         | Consensus Client        |
| Asset Manager  | Consensus Client        |

#### Unique identification of Consensus Nodes and Consensus Clients 

In the Tari communication network, each entity makes use of a node id to determine their position in the network.
This node id is either assigned based on registration on the base layer or can be derived from the entities identification public key.
The method used to obtain a node id will limit the trustworthiness of each entity on the Tari communication network.
Nodes with registration assigned node ids are considered more trustworthy compared to nodes with derived node ids. 

Obtaining a node id from registration on the base layer is important as it will limit the potential of Eclipse attacks on the network.
It will make it more difficult for Bad players to position themselves in correct patterns on the network to perform disruptive operations and actions. 
Also, in sensitive situations or situations where the Kademlia style directed propagation of messages are vulnerable then gossip protocol based broadcasting of messages can be performed as a fallback. 

| Entity Type    | Communication Node Type | Node ID assignment |
|---             |---                      |---                 |
| Validator Node | Consensus Node          | Registration       |
| Base Node      | Consensus Node          | Derived            |
| Wallet         | Consensus Client        | Derived            |
| Asset Manager  | Consensus Client        | Derived            |





What is the NodeID?
Used as a unique identifier to locate that node on the network. The NodeID should be linked to a Private Key 

How do the different entities obtain NodeID:
VN - NodeID obtained by unique location of Validator Node registration on blockchain. 
BN - Self selected NodeID
W - Self selected NodeID
AM - Self selected NodeID
Mining Servers and Mining workers are excluded: Mining Servers will have a local or remote connection with a base node and a Mining Worker will have a local or remote connection with a Mining Server. They won't propagate anything on the network. The Base Node will do it on their behalf.








Messages can be transmitted on this network in an unencrypted or encrypted form. Typically messages that that has been sent in unencrypted form is of interest to all entities on the network and should be propagated so that everyone obtains a copy. Encrypted messages can only be opened by specific entities and is not of interest to the rest of the network. More appropriate and efficient transportation techniques can be applied to forward these messages to the correct parties.


Should connections be kept alive?
BNs should keep connections alive with other BNs
VNs that are part of a committee should keep their connections alive with the other VNs in the committee.
Wallets should not keep connections alive, and should only connect with peers when needed.


Maintaining connections?
When more than 30 minutes has passed with a peer and communication has been exchanged then a heartbeat message should be send to the peer in an attempr to keep the connection alive.
In the case of a BN, if the peer has become unreachable, then a new peer can be selected to create a connection with.
In the case of a VN where the peer was a VN from shared committee, then the peer must wait and attempt to create a new connection with that same peer.
If more than 90 minutes have passed since the last successful communication with a peer node, then the channel can be closed and a new communication channel can be established with a different peer.

Bootstrapping?
To be able to connect to the Tari network, the internet address of at least one peer on the Tari network needs to be know. A peer that can be connected to can either be manually added or a seed list of nodes can be used to bootstrap your peer list or routing table.



Who will propagate messages/instructions/transactions on either the base-layer or DAN?
VN - Instructions on DAN
BN - Transactions on Base Layer
W - No
AM - No

Communication matrix:


Potential attacks
Only one registration of an comm address can be stored in the routing table to limit a bad player from spinning up multiple nodes from a single computer




Network Address:
| Field Size | Description  | Data type | Comments                                     |
|---         |---           |---        |---                                           |
|  2         | address type | uint4     | IPv4/IPv6/Url/Tor/I2P                        |
| 16         | address      | char[52]  | IPv4, IPv6, Tor(Base32), I2P(Base32) address |
|  2         | port         | uint16    | port number                                  |

The address type can used to determine how to interpret the address chars. An I2P address can be interpreted as "{52 address chars}.b32.i2p". The Tor address should be interpreted as "http://{16 or 52 address chars}.onion/". The IPv4 and IPv6 address can be stored in the address field without modification. URL addresses can be used for nodes with dynamic IPs, but they are limited to 52 chars. 

Peer Address:

| Field Size | Description      | Data type          | Comments                                                                 |
|---         |---               |---                 |---                                                                       |
|  -         | network address  | network_address    |                                                                          |
|  -         | Node ID          | node_id            | Assigned for VN, Self selected for BN,W and AM                           |
|  -         | public_key       | public_key         |                                                                          |
|  -         | node_type        | node_type          | VN,BN,W or AM                                                            |
|  -         | linked asset ids | list of asset ids  | Asset ids can be used as an address on Tari network similar to a node id |
|  -         | last_connection  | timestamp          | Time of last successful connection with peer                             |

Routing Table:





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





Functionality required of communication network:
- It MUST be able to send a directed message through the network to a entity where the entities Node ID is known but not their communication address.

- An entity on the CN must store all received data and routing information for that data, where the data id is similar to the node id of the entity.
- An entity that has received responsibility for managing an asset or specific data must request that all entities with similar node ids to the data id should add the that entities communication information in their routing table. 
- When an entity receives an unencrypted message that requires gossip based broadcasting then the entity must forward the message to all connected peers. 
- When an entity receives an encrypted message that requires directional forwarding or directed broadcasting the entity MUST forward the message to all entities in his routing table that are closer.
- If no other entities was found in the local routing table that had a closer nod id compared to the current entities node id then the encrypted message must be stored for set period of time. 



- Entities MUST have a mechanism to store a table of data

Functionality required of Consensus Nodes:
- A Consensus Nodes MUST have the ability to connect and maintain connections with a set of peers.
- The MUST have the ability to broadcast messages to a set of self selected peers.


Functionality required of Consensus Clients:
Registration
- It MUST select a cryptographic keypair used for identifying the client on the Tari Communication network 
- It MUST have a mechanism to derive a node_id from the self selected public key
- A new Consensus client MUST broadcast an add peer request to the Tari communication network so that its routing information can be stored by the Consensus nodes with similar node_ids.

Normal behavior
- A Consensus Client MUST maintain a small list of peers on the Tari Communication network.
- As the Consensus Client becomes aware of other Consensus Nodes and Clients on the communication network he SHOULD extend his routing table by including the newly discovered Node/Clients contact information.
- Peers from the Consensus Clients routing table that have been unreachable for a number of attempts SHOULD be removed from the routing table.
-



