#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate bitflags;

#[cfg(test)]
mod test_utils;

pub mod aggregated_body;
pub mod bullet_rangeproofs;
pub mod consensus;
pub mod emission;
pub mod fee;
pub mod proto;
pub mod tari_amount;
pub mod transaction;
#[allow(clippy::op_ref)]
pub mod transaction_protocol;
pub mod types;
// Re-export commonly used structs
pub use transaction_protocol::{recipient::ReceiverTransactionProtocol, sender::SenderTransactionProtocol};
// Re-export the crypto crate to make exposing traits etc easier for clients of this crate
pub use tari_crypto as crypto;
