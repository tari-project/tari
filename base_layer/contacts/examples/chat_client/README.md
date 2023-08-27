# Taiji Chat Client Example

This crate provides an example of setting up a Taiji chat client. The client will connect to the Taiji network with all necessary p2p intializations needed for network communication. The client itself then exposes a few simple functions like `get_messages`, `send_message`, `check_online_status`, and `add_contact`. With this minimal functionality the client can be used to chat!

## Initialization of the chat client

The client needs a network identity provided to it, and seed nodes to connect to on the network. Here's an example of making this connection:

```rust
let port = get_port(18000..18499).unwrap();
let temp_dir_path = get_base_dir()
    .join("chat_clients")
    .join(format!("port_{}", port))
    .join(name.clone());
let address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
let identity = NodeIdentity::random(&mut OsRng, address, PeerFeatures::COMMUNICATION_NODE);

let mut client = Client::new(
    identity,
    vec![seed_peer],
    temp_dir_path,
    Network::LocalNet,
);

client.initialize().await;
```

This crate is part of the [Taiji Cryptocurrency](https://taiji.com) project.
