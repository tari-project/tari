# Minotari wallet gRPC client

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

A Rust client for the Minotari console wallet gRPC interface.

```rust,ignore
use minotari_wallet_grpc_client::{Certificate, ClientTlsConfig, GrpcAuthentication, WalletGrpcClient};

let ca = Certificate::from_pem(std::fs::read("wallet_ca.pem")?);
let mut client = WalletGrpcClient::builder("https://wallet.example:18143")
    .tls(ClientTlsConfig::new().ca_certificate(ca).domain_name("localhost"))
    .auth(&GrpcAuthentication::Basic { username, password })
    .connect()
    .await?;
```

`WalletGrpcClient::connect` and `connect_with_auth` are shorthands for the builder with default settings.
`WalletGrpcClient::from_channel` wraps a channel you built yourself.

## Trust model

The client authenticates the wallet only when you call `.tls()` with a CA you trust for that wallet. The wallet's
certificate is self-signed by default, so pin the wallet's own CA (`wallet_ca.pem` from the wallet's config
directory); no public root store is used. The default certificate only names `localhost`, `127.0.0.1` and `0.0.0.0`,
so for a remote wallet set `.domain_name("localhost")` or issue the wallet a certificate for its real name.

`.tls()` requires an `https://` address; with any other address `connect` returns
`WalletClientError::TlsRequiresHttps` instead of silently connecting without TLS. An `https://` address without
`.tls()` fails to connect. The client has no roots to verify the wallet against, and it
does not fall back to an unverified connection.

## Credentials over plaintext

`connect` refuses to send Basic credentials over plaintext `http://` to a host that is not loopback (`127.0.0.0/8`,
`::1` or `localhost`) and returns `WalletClientError::InsecureCredentialTransport`. Use `https://` with `.tls()`, or
call `.allow_insecure_credentials(true)` if the network path is protected some other way (for example an SSH tunnel or
VPN). Connections without credentials are never refused, and `from_channel` does not apply the check.

`localhost` is trusted by name, not resolved: on a host whose resolver forwards `localhost` to DNS, a spoofed answer
could receive the credentials. Use `127.0.0.1` or `[::1]` to avoid relying on the resolver.

## Transport defaults

| Setting                                  | Default |
|------------------------------------------|---------|
| `connect_timeout`                        | 10s     |
| `http2_keepalive` interval / timeout     | 30s / 20s, also while idle |
| `tcp_keepalive`                          | 60s     |
| `timeout` (per request)                  | none    |

`timeout` only bounds the time until the wallet starts responding, so it does not cut an open stream. It is off by
default because a deadline that suits unary calls is a per-call decision: set one with `Request::set_timeout` on each
unary call, or use `.timeout()` if the client never streams. Messages up to `MAX_GRPC_MESSAGE_SIZE` are accepted in both directions.
