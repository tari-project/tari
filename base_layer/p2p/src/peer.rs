use tari_comms::peer_manager::peer::Peer;
use tari_crypto::keys::PublicKey;

#[derive(Debug)]
pub enum PeerType {
    BaseNode,
    ValidatorNode,
    Wallet,
    TokenWallet,
}

#[derive(Debug)]
pub struct PeerWithType<K: PublicKey> {
    pub peer: Peer<K>,
    pub peer_type: PeerType,
}

impl<K> PeerWithType<K>
where K: PublicKey
{
    /// Constructs a new peer with peer type
    pub fn new(peer: Peer<K>, peer_type: PeerType) -> PeerWithType<K> {
        PeerWithType { peer, peer_type }
    }
}
