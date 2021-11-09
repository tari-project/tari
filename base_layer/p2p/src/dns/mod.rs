mod client;
pub use client::DnsClient;

mod error;
pub use error::DnsClientError;

mod roots;
pub(crate) use roots::default_trust_anchor;

#[cfg(test)]
pub(crate) mod mock;
