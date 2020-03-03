//// Represents a message which is about to go on or has just come off the wire.
//// As described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Envelope {
    #[prost(uint32, tag = "1")]
    pub version: u32,
    #[prost(message, optional, tag = "3")]
    pub header: ::std::option::Option<EnvelopeHeader>,
    #[prost(bytes, tag = "4")]
    pub body: std::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EnvelopeHeader {
    #[prost(bytes, tag = "1")]
    pub public_key: std::vec::Vec<u8>,
    #[prost(bytes, tag = "2")]
    pub signature: std::vec::Vec<u8>,
    #[prost(uint32, tag = "3")]
    pub flags: u32,
}
/// Parts contained within an Envelope. This is used to tell if an encrypted
/// message was successfully decrypted, by decrypting the envelope body and checking
/// if deserialization succeeds.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EnvelopeBody {
    #[prost(bytes, repeated, tag = "1")]
    pub parts: ::std::vec::Vec<std::vec::Vec<u8>>,
}
