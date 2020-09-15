/// A tari message type is an immutable 32-bit signed integer indicating the type of message being received or sent
/// over the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TariMessageType {
    None = 0,
    // -- NetMessages --
    PingPong = 1,
    // -- Blockchain messages --
    NewTransaction = 65,
    NewBlock = 66,
    SenderPartialTransaction = 67,
    ReceiverPartialTransactionReply = 68,
    BaseNodeRequest = 69,
    BaseNodeResponse = 70,
    MempoolRequest = 71,
    MempoolResponse = 72,
    TransactionFinalized = 73,
    /// -- DAN Messages --
    TransactionCancelled = 74,
    // -- Extended --
    Text = 225,
    TextAck = 226,
}
