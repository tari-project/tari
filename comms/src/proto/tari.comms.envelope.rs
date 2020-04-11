/// Parts contained within an Envelope. This is used to tell if an encrypted
/// message was successfully decrypted, by decrypting the envelope body and checking
/// if deserialization succeeds.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EnvelopeBody {
    #[prost(bytes, repeated, tag = "1")]
    pub parts: ::std::vec::Vec<std::vec::Vec<u8>>,
}
