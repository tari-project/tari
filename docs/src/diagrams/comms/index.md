  

# Top level triggers
1. New Inbound TCP Socket Connection
1. New Outbound TCP Socket Connection
1. New Yamux Stream
   1. [Protocol negotiation](./protocol_negotiation.md)
1. New Messaging Protocol Stream
   1. [Inbound Messaging](./inbound_messaging.md)
   2. [Outbound Messaging TODO](./outbound_messaging.md)
1. New RPC session handshake
1. Timed triggers
    1. Dial backoff
    2. Connectivity manager state refresh/cleanup (periodic)
1. Start up triggers
   1. LivenessCheck (checks if node can be contacted from public address)


