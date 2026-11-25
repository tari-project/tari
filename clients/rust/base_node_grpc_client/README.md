# Tari Base Node grpc client

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

```rust,ignore
use minotari_node_grpc_client::{BaseNodeGrpcClient, GrpcClientConfig, grpc};

let mut client = BaseNodeGrpcClient::connect("http://127.0.0.1:18142", &GrpcClientConfig::default()).await?;
let tip = client.get_tip_info(grpc::Empty {}).await?;
```

`GrpcClientConfig::default()` sets a 5 s connect timeout, a 30 s per-request timeout (`request_timeout: None`
disables it), HTTP/2 and TCP keepalive, and an 8 MiB message limit in both directions. An `https://` address needs
`tls` with the node's CA certificate. Plaintext to a host other than loopback is allowed but logs a warning: anyone on
the network path can alter templates and blocks and read Basic credentials.

`RawBaseNodeGrpcClient<T>` is the bare generated client, for callers that build their own transport.
