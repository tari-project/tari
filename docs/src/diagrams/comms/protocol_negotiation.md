# Protocol negotiation

## Overview
Protocol negotiation occurs when a peer is interested in speaking a particular protocol with another peer
over a yamux substream. Each protocol MUST have a unique variable-length byte-string identifier. Once a 
substream is opened, either outbound or inbound, the protocol negotiation message must be sent within a specific timeout.

If a peer has prior knowledge of the protocols supported by a peer, the node MAY set the OPTIMISTIC flag which,
if successful, incurs almost zero additional time to complete. This is because the negotiation message is sent and 
the upstream protocol immediately continues. If unsuccessful, the remote peer immediately closes the substream.

The non-optimistic case can be used otherwise, however this requires a reply from the responder before the upstream
protocol can begin. This allows peers to negotiate protocol substreams without prior knowledge of the other peer's 
supported protocols. This is typically occurs when a node has never contacted a peer before. This process may proceed 
for a maximum of 5 rounds before terminating the substream.

The protocol is coded in comms/core/src/protocol/negotiation.rs

### Inbound Protocol negotiation 

Messages are the tuple (length (u8), flags (u8), protocol_name (variable 255 (0xff) max))

```mermaid
flowchart TD
A[Start] --> B[1. New substream opened by remote peer]
B --> C[2. PeerConnection starts ProtocolNegotiation]
C --> D[3.⌛️ Inbound ProtocolNegotiation waits for bytes to read]
D --> E["4. Read frame (len, flags, protocol_name)"]

%% Optimistic
E --> F{5. is OPTIMISTIC flag set?}
F -->|yes| H1{"6. is protocol_name is supported?"}
H1 -->|yes| H1a(("7. protocol_name is returned \n and downstream is notified"))
H1 -->|no| H1b((8. ProtocolOptimisticNegotiationFailed))

%% Not optimistic
E -->|no| H2{9. is TERMINATE flag is set?}
H2 -->|yes| H2_y((10. ProtocolNegotiationTerminatedByPeer))
H2 -->|no| H2_n{11. is protocol name is supported?}

H2_n -->|no| H2_n_n["12. Set PROTOCOL_NOT_SUPPORTED flag"]
H2_n_n --> H2_n_n1{"13. is MAX_ROUNDS_ALLOWED(5) reached?"}

H2_n_n1 -->|yes| h2_n_n1_y["13. Set TERMINATE flag"]
h2_n_n1_y-->G(("14. Send frame (0, flags, []) and close substream"))

H2_n_n1 -->|no| h2_n_n1_n["15. Set NOT_SUPPORTED flag"]
h2_n_n1_n-->I["16. Send frame (0, flags, [])"]
I --> E

H2_n -->|yes| H2_n_y(("17. Send frame (len, NONE, protocol_name)\nCOMPLETE"))
```
<figcaption>Fig. 1. Inbound protocol negotiation</figcaption>