use link_crypto::PeerId;

pub trait LocalPeer {
    fn local(&self) -> PeerId;
}
