# Common message pipeline

## Overview
When a message comes into the node, it runs through this pipeline, before being distributed using a pubsub connector. 
Many services in the node (TODO: confirm wallet as well) will listen for a particular TariMessageType and only receive
messages of those types.

> TODO: Add in overview diagram of bigger system and where this fits in

```mermaid
A[Start] --> B[Inbound pipeline and messaging protocol extension]
B --> C[Inbound DHT middleware]
C --> D[Pubsub connector]
D --> E[TODO:Base node stuff]
D --> F[TODO:Base node other stuff]
D --> G[TODO:Wallet stuff]
```

## Inbound pipeline and Messaging protocol extension

```mermaid

A[Start] --> B[pipeline = Create pipeline]
B --> C["Add Messaging protocol extension (pipeline, inbound_tx)"]
C --> D[be = Bounded executor]
D --> E["Inbound::run(be, inbound_rx)"]

```


# inbound dht middleware
```mermaid
A[start]--> B[MetricsLayer]
B --> C[inbound DeserializeLayer]
C --> D["FilterLayer(unsupported_saf_messages_filter) "]
D --..-- N1>"dht.rs line 383"]
D--> E["FilterLayer(discard_expired_messages)"]
E --..-- N2>"dht.rs line 437"]
E--> F[DecryptionLayer]
F--> G[DedupLayer]
G--> H["FilterLayer(filter_messages_to_rebroadcast)"]
H --..-- N3>"dht.rs line 406"]
H-->I[MessageLoggingLayer]
I-->J["saf StoreLayer"]
J-->K["saf ForwardLayer"]
K-->L["saf MessageHandlerLayer"]
L-->M["saf DhtHandlerLayer"]
M-->Z[end]
   N1:::note
   N2:::note
   N3:::note
    classDef note fill:#eee,stroke:#ccc



```

# pubsub connector

```mermaid

flowchart TD
   A[Start : todo]-->Aa1[[Inbound middleware pipeline]]
   Aa1--> A1[InboundDomainConnector::construct_peer_message] 
   A1 --..--N1>inbound_connector line 73] // Note: Can decode_part fail?
   A1 --> B[Pubsub connector]
   B --> C[if peer_message::TariMessageType is valid ] TODO: Ban peer
   C --> D[forward to publisher]
   D --> E[if topic == sub topic]
   E --> F[forward to subscription]

    N1:::note
    classDef note fill:#eee,stroke:#ccc

```

