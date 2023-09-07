# Inbound message pipeline

## Overview
Inbound messaging covers the raw messages being read off the substream and passed through the inbound pipeline
to the domain pubsub connector. Each message that comes in is processed by the inbound DHT messaging middlewares
in its own tokio task. The number of concurrent tasks spawned is limited by the BoundedExecutor. From there they arrive
at the pubsub connector which 
Services that are interested in network messages, in the _minotari node_ and _minotari console wallet_ subscribe to a
particular `TariMessageType`s 

> TODO: Add in overview diagram of bigger system and where this fits in

```mermaid
flowchart TD
    A[Start] --> B[1. trigger: Protocol Negotiation /t/msg/0.1]
    B --> C[2. protocols::messaging::InboundMessaging]
    C -->|InboundMessage| D[3. Inbound pipeline]
    D -->|"&lt;threads&gt;"| E[4. Inbound DHT middlewares]
    E -->|DecryptedDhtMessage| F[5. Pubsub connector]
    F --o|PeerMessage| G[6. ... Domain subscribers ...]
```
<figcaption>Fig. 1. Inbound message stack overview</figcaption>

### Inbound Messaging Protocol 

```mermaid
flowchart TD
A[Start] --> B[1. Protocol Negotiation /t/msg/0.1]
B -->|notify| C["2. event: NewInboundSubstream handled by protocol::MessagingProtocol"]
C --> D[3. Spawn InboundMessaging handler]
D --> E1["4a. wait and read message frame"]
E1 --> E2["4b. InboundMessage(sender PK, raw bytes)"]
E2 --> E3["4c. Send InboundMessage to Inbound pipeline"]
E3 --> E4["4d. Emit MessageReceived event"]
E4 -->E1
E1 -->|end of stream| F((5. worker exits))

```
<figcaption>Fig. 2. Yamux substream to inbound messaging</figcaption>

### Inbound Message Pipeline

```mermaid
flowchart TD
A[Start] --> B["1. InboundMessage received (mpsc) from Inbound Messaging Protocol"]
B --> C["2. BoundedExecutor::spawn(inbound_rx)"]
C -..- N1>"bounded executor concurrency: 4 (configurable)"]
C --> D[3. Inbound DHT middleware]
D --> E["4. mpsc channel to Pubsub connector"]
    N1:::note
    classDef note fill:#eee,stroke:#ccc
```
<figcaption>Fig. 3</figcaption>

### Inbound DHT middleware
```mermaid
flowchart TD
A[start]--> B[1. MetricsLayer]
B --> C[2. inbound DeserializeLayer]
C --> D["3. FilterLayer(unsupported_saf_messages_filter) "]
D -..- N1>"dht.rs line 383"]
D--> E["4. FilterLayer(discard_expired_messages)"]
E -..- N2>"dht.rs line 437"]
E--> F[5. DecryptionLayer]
F--> G[6. DedupLayer]
G--> H["7. FilterLayer(filter_messages_to_rebroadcast)"]
H -..- N3>"dht.rs line 406"]
H-->I[8. MessageLoggingLayer]
I-->J["9. saf StoreLayer"]
J-->K["10. saf ForwardLayer"]
K-->L["11. saf MessageHandlerLayer"]
L-->M["12. saf DhtHandlerLayer"]
M-->Z[end]
   N1:::note
   N2:::note
   N3:::note
    classDef note fill:#eee,stroke:#ccc



```
<figcaption>Fig. 4. Inbound message pipeline</figcaption>

# pubsub connector

```mermaid

flowchart TD
    A[Start]-->A1[[1. Inbound middleware pipeline]]
    A1--> A2["2. &lt;InboundDomainConnector as Service&gt;::call"]
    A2--> A3["3. InboundDomainConnector::construct_peer_message"]
    A3 -..-N1>inbound_connector line 73]
    A3 --> B[4. Pubsub connector]
    B --> C["5. if peer_message::TariMessageType is valid "]
    C --> D["6. publish to domain subscribers (tokio broadcast channel)"]
    D --> E[end]

    N1:::note
    classDef note fill:#eee,stroke:#ccc
```
<figcaption>Fig. 5</figcaption>

Subscribing to message topics (TariMessageType) sent by the PubsubConnector.
This is the general process followed however this is up to the domain implementer.

```mermaid
flowchart TD
    A[Start]-->B[1. subscribe to topic as a BroadcastStream]
    B --> D["2. filter stream: payload.topic == topic"]
    D --> E["3. map stream: map_decode::<T> (decode domain message type T)"]
    E -..-N1>"p2p::services::utils.rs (1)"]
    E --> F["4. filter stream: ok_or_skip_result"]
    F -->G[end]

    N1:::note
    classDef note fill:#eee,stroke:#ccc

```
<figcaption>Fig. 6 - subscription to inbound messages coming from the pubsub connector _Fig. 4_</figcaption>

* (1) [p2p/services/utils.rs:43](https://github.com/tari-project/tari/blob/08ba91af3031aa2a3c5357a5f494f95f9c8a0049/base_layer/p2p/src/services/utils.rs#L43)
* 

