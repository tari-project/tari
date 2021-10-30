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

#### Notes

Building docker images:

```
cd buildtools/docker_rig
docker build -t quay.io/tarilabs/tor:latest -f base_node.Dockerfile .
docker build -t quay.io/tarilabs/tari_base_node:latest -f base_node.Dockerfile ../../
```

Base node/Wallet config for using the Tor docker container:

```toml
tcp_listener_address = "/ip4/0.0.0.0/tcp/18189"
transport = "tor"
tor_control_address = "/dns4/tor/tcp/9051"
tor_control_auth = "password=asdf" # replace with your configured password
tor_onion_port = 18141
tor_forward_address = "/ip4/0.0.0.0/tcp/18189"
tor_socks_address_override="/dns4/tor/tcp/9050"
```

When attaching to a running container:

To detach the tty without exiting the shell/program, use the escape sequence ^P^Q (Ctrl+P followed by Ctrl+Q).
