# The complete Docker guide to Tari

## Quick Start

Install docker

Run the universal installer
Select 'docker' as the system layout
Configure the containers to run
Other config options incl tor password
"Go!"


## Layout

      +-----------------------+
      |                       |
+---->|    Console Wallet     +------------------+
|     |                       |                  |
|     +----------+------------+                  |
|                |                               |
|                | gRPC                          |
|                |                               |
|                |                               |
|     +----------v------------+           +------v-----+
|     |                       |  Socks5   |            |
|     |       Base Node       +---------->|     Tor    |----> Network
|     |                       |           |            |
|     +----------^------------+           +------------+
|                |
|                |
|                |
|     +----------+------------+
|     |                       |
+-----+      SHA3-Miner       |
|     |                       |
|     +-----------------------+
|
|
|
|     +-----------------------+
|     |                       |
+-----+        XMRRig etc     |
      |                       |
      +-----------------------+


