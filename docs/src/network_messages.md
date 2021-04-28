# Network Messages

These are types of messages in the Tari comms layer and how they are propagated:

| Message Type | Proto Value | Description | Created by | First Propagation Type | Receiving Node Propagation | How handled? |
| --- | --- | --- | --- | --- | --- | --- |
| None |  0 | Not used | N/A | N/A | N/A | N/A |
| Join | 1 | NOTE: Disabled by default. Used to join the network, but only if configured | When Connectivity service decides that the node has come online (`ConnectivityStateOnline` event is fired), if `config.auto_join == true`, send join. | Closer only * propagation factor | Direct | Do not repropagate, otherwise could result in loops |
| Discovery | 2 | Find a specific node on the network | CloserOnly with propagation factor | Propagated via CloserOnly with propagation factor | Sends direct response |
| DiscoveryResponse | 3 | Response to discovery | Direct | N/A | Event is fired |
| SafRequestMessage | |
| SafStoredMessage | | 

