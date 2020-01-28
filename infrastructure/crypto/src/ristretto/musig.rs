// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    musig::{JointKey, JointKeyBuilder, MuSigError},
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    signatures::SchnorrSignature,
};
use digest::Digest;
use std::marker::PhantomData;
use tari_utilities::{fixed_set::FixedSet, ByteArray};

//-----------------------------------------  Constants and aliases    ------------------------------------------------//

type JKBuilder = JointKeyBuilder<RistrettoPublicKey, RistrettoSecretKey>;
type JointPubKey = JointKey<RistrettoPublicKey, RistrettoSecretKey>;
type MessageHash = Vec<u8>;
type MessageHashSlice = [u8];

/// MuSig signature aggregation. [MuSig](https://blockstream.com/2018/01/23/musig-key-aggregation-schnorr-signatures/)
/// is a 3-round signature aggregation protocol.
/// We assume that all the public keys are known and publicly accessible. A [Joint Public Key](structs.JointKey.html)
/// is constructed by all participants.
/// 1. In the first round, participants share the hash of their nonces.
/// 2. Participants then share their public nonce, \\( R_i \\), and all participants calculate the shared nonce,
///   \\( R = \sum R_i \\).
/// 3. Each participant then calculates a partial signature, with the final signature being the sum of all the
/// partial signatures.
///
/// This protocol is implemented as a Finite State Machine. MuSig is a simple wrapper around a `MusigState` enum that
/// holds the various states that the MuSig protocol can be in, combined with a `MuSigEvents` enum that enumerates
/// the relevant input events that can  occur. Any attempt to invoke an invalid transition, or any other failure
/// condition results in the `Failure` state; in which case the MuSig protocol should be abandoned.
///
/// Rust's type system is leveraged to prevent any rewinding of state; old state variables are destroyed when
/// transitioning to new states. The MuSig variable also _takes ownership_ of the nonce key, reducing the risk of
/// nonce reuse (though obviously it doesn't eliminate it). Let's be clear: REUSING a nonce WILL result in your secret
/// key being discovered. See
/// [this post](https://tlu.tarilabs.com/cryptography/digital_signatures/introduction_schnorr_signatures.html#musig)
/// for details.
///
/// The API is fairly straightforward and is best illustrated with an example. Alice and Bob are going to construct a
/// 2-of-2 aggregated signature.
///
/// ```edition2018
///       # use tari_crypto::ristretto::{ musig::RistrettoMuSig, ristretto_keys::* };
///       # use tari_utilities::ByteArray;
///       # use tari_crypto::keys::PublicKey;
///       # use sha2::Sha256;
///       # use digest::Digest;
///       let mut rng = rand::thread_rng();
///       // Create a new MuSig instance. The number of signing parties must be known at this time.
///       let mut alice = RistrettoMuSig::<Sha256>::new(2);
///       let mut bob = RistrettoMuSig::<Sha256>::new(2);
///       // Set the message. This can only be done once to prevent replay attacks. Any attempt to assign another
///       // message will result in a Failure state.
///       alice = alice.set_message(b"Discworld");
///       bob = bob.set_message(b"Discworld");
///       // Collect public keys
///       let (k_a, p_a) = RistrettoPublicKey::random_keypair(&mut rng);
///       let (k_b, p_b) = RistrettoPublicKey::random_keypair(&mut rng);
///       // Add public keys to MuSig (in any order. They get sorted automatically when _n_ keys have been collected.
///       alice = alice
///           .add_public_key(&p_a)
///           .add_public_key(&p_b);
///       bob = bob
///           .add_public_key(&p_b)
///           .add_public_key(&p_a);
///       // Round 1 - Collect nonce hashes - each party does this individually and keeps the secret keys secret.
///       let (r_a, pr_a) = RistrettoPublicKey::random_keypair(&mut rng);
///       let (r_b, pr_b) = RistrettoPublicKey::random_keypair(&mut rng);
///       let h_a = Sha256::digest(pr_a.as_bytes()).to_vec();
///       let h_b = Sha256::digest(pr_b.as_bytes()).to_vec();
///       bob = bob
///           .add_nonce_commitment(&p_b, h_b.clone())
///           .add_nonce_commitment(&p_a, h_a.clone());
///       // State automatically updates:
///       assert!(bob.is_collecting_nonces());
///       alice = alice
///           .add_nonce_commitment(&p_a, h_a.clone())
///           .add_nonce_commitment(&p_b, h_b.clone());
///       assert!(alice.is_collecting_nonces());
///        // Round 2 - Collect Nonces
///        bob = bob
///           .add_nonce(&p_b, pr_b.clone())
///           .add_nonce(&p_a, pr_a.clone());
///       assert!(bob.is_collecting_signatures());
///       alice = alice
///           .add_nonce(&p_a, pr_a.clone())
///           .add_nonce(&p_b, pr_b.clone());
///       assert!(alice.is_collecting_signatures());
///       // round 3 - Collect partial signatures
///       let s_a = alice.calculate_partial_signature(&p_a, &k_a, &r_a).unwrap();
///       let s_b = bob.calculate_partial_signature(&p_b, &k_b, &r_b).unwrap();
///       alice = alice
///           .add_signature(&s_a, true)
///           .add_signature(&s_b, true);
///       assert!(alice.is_finalized());
///       bob = bob
///           .add_signature(&s_b, true)
///           .add_signature(&s_a, true);
///       assert!(bob.is_finalized());
///       assert_eq!(alice.get_aggregated_signature(), bob.get_aggregated_signature());
/// ```
pub struct RistrettoMuSig<D: Digest> {
    state: MuSigState,
    digest_type: PhantomData<D>,
}

//----------------------------------------------      RistrettoMuSig       -------------------------------------------//

impl<D: Digest> RistrettoMuSig<D> {
    /// Create a new, empty MuSig ceremony for _n_ participants
    pub fn new(n: usize) -> RistrettoMuSig<D> {
        let state = match Initialization::new::<D>(n) {
            Ok(s) => MuSigState::Initialization(s),
            Err(e) => MuSigState::Failed(e),
        };
        RistrettoMuSig {
            state,
            digest_type: PhantomData,
        }
    }

    /// Convenience wrapper function to determined whether a signing ceremony has failed
    pub fn has_failed(&self) -> bool {
        match self.state {
            MuSigState::Failed(_) => true,
            _ => false,
        }
    }

    /// IF `has_failed()` is true, you can obtain the specific error that caused the failure
    pub fn failure_reason(&self) -> Option<MuSigError> {
        match &self.state {
            MuSigState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// Convenience function to determine whether we're in Round One of MuSig (nonce hash collection)
    pub fn is_collecting_hashes(&self) -> bool {
        match self.state {
            MuSigState::NonceHashCollection(_) => true,
            _ => false,
        }
    }

    /// Convenience function to determine whether we'rein Round Two of MuSig (public nonce collection)
    pub fn is_collecting_nonces(&self) -> bool {
        match self.state {
            MuSigState::NonceCollection(_) => true,
            _ => false,
        }
    }

    /// Convenience function to determine whether we're in Round Three of MuSig (partial signature collection)
    pub fn is_collecting_signatures(&self) -> bool {
        match self.state {
            MuSigState::SignatureCollection(_) => true,
            _ => false,
        }
    }

    /// Convenience function to determine whether The MuSig protocol is complete (the aggregate signature is ready)
    pub fn is_finalized(&self) -> bool {
        match self.state {
            MuSigState::Finalized(_) => true,
            _ => false,
        }
    }

    /// Return the index of the public key in the MuSig ceremony. If were still collecting public keys, the state has
    /// been finalised, or the pub_key isn't in the list, then None is returned.
    pub fn index_of(&self, pub_key: &RistrettoPublicKey) -> Option<usize> {
        let joint_key = match &self.state {
            MuSigState::NonceHashCollection(s) => &s.joint_key,
            MuSigState::NonceCollection(s) => &s.joint_key,
            MuSigState::SignatureCollection(s) => &s.joint_key,
            _ => return None,
        };
        joint_key.index_of(pub_key).ok()
    }

    /// Add a public key to the MuSig ceremony. Public keys can only be added in the `Initialization` state and the
    /// MuSig state will only progress to the next state (nonce hash collection) once exactly `n` unique public keys
    /// have been added.
    pub fn add_public_key(self, key: &RistrettoPublicKey) -> Self {
        let key = key.clone();
        self.handle_event(MuSigEvent::AddKey(key))
    }

    /// Set the message to be signed in this MuSig ceremony
    pub fn set_message(self, msg: &[u8]) -> Self {
        let msg = D::digest(msg).to_vec();
        self.handle_event(MuSigEvent::SetMessage(msg))
    }

    /// Adds a Round 1 public nonce commitment to the MuSig state. Once _n_ commitments have been collected, the
    /// MuSig state will automatically switch to nonce collection.
    pub fn add_nonce_commitment(self, pub_key: &RistrettoPublicKey, commitment: MessageHash) -> Self {
        self.handle_event(MuSigEvent::AddNonceHash(pub_key, commitment))
    }

    /// Adds a public nonce to the MuSig ceremony. Be careful never to re-use public nonces for different MuSig
    /// ceremonies. This risk is mitigated by the MuSig object taking ownership of the nonce, meaning that you need
    /// to explicitly call `clone()` on your nonce if you want to use it elsewhere.
    /// The MuSig state will automatically switch to `SignatureCollection` once _n_ valid nonces have been collected.
    pub fn add_nonce(self, pub_key: &RistrettoPublicKey, nonce: RistrettoPublicKey) -> Self {
        self.handle_event(MuSigEvent::AddNonce(pub_key, nonce))
    }

    /// Adds a partial signature to the MuSig ceremony. Each party can calculate their own partial signature by
    /// calling `calculate_partial_signature(k, r)` and share the result with the other signing parties. You can
    /// choose to validate each partial signature as it is added (in which case, if the state reaches Finalized, the
    /// aggregate signature will automatically be valid). This is slower than just checking the aggregate signature,
    /// but you will also know exactly _which_ signature failed.
    /// Otherwise pass `false` to `should_validate` and verify the aggregate signature.
    pub fn add_signature(self, s: &RistrettoSchnorr, should_validate: bool) -> Self {
        self.handle_event(MuSigEvent::AddPartialSig(Box::new(s.clone()), should_validate))
    }

    /// Return a reference to the standard challenge $$ H(R_{agg} || P_{agg} || m) $$, or `None` if the requisite data
    /// isn't available
    pub fn get_challenge(&self) -> Option<&RistrettoSecretKey> {
        match &self.state {
            MuSigState::SignatureCollection(s) => Some(&s.challenge),
            MuSigState::Finalized(s) => Some(&s.challenge),
            _ => None,
        }
    }

    /// If the MuSig ceremony is finalised, returns a reference to the aggregated signature, otherwise returns None.
    /// This function returns a standard Schnorr signature, so you can use it anywhere you can use a
    /// Schnorr signature.
    pub fn get_aggregated_signature(&self) -> Option<&RistrettoSchnorr> {
        match &self.state {
            MuSigState::Finalized(s) => Some(&s.s_agg),
            _ => None,
        }
    }

    /// Once all public keys have been collected, this function returns a reference to the joint public key as
    /// defined by the MuSig algorithm. If public keys are still being collected, this returns None.
    pub fn get_aggregated_public_key(&self) -> Option<&RistrettoPublicKey> {
        match &self.state {
            MuSigState::NonceHashCollection(s) => Some(s.joint_key.get_joint_pubkey()),
            MuSigState::NonceCollection(s) => Some(s.joint_key.get_joint_pubkey()),
            MuSigState::SignatureCollection(s) => Some(s.joint_key.get_joint_pubkey()),
            MuSigState::Finalized(s) => Some(s.joint_key.get_joint_pubkey()),
            _ => None,
        }
    }

    fn get_public_nonce(&self, index: usize) -> Option<&RistrettoPublicKey> {
        match &self.state {
            MuSigState::SignatureCollection(s) => s.public_nonces.get_item(index),
            _ => None,
        }
    }

    /// Calculate my partial MuSig signature, based on the information collected in the MuSig ceremony to date, using
    /// the secret key and secret nonce supplied.
    pub fn calculate_partial_signature(
        &self,
        pub_key: &RistrettoPublicKey,
        secret: &RistrettoSecretKey,
        nonce: &RistrettoSecretKey,
    ) -> Option<RistrettoSchnorr>
    {
        let index = self.index_of(pub_key)?;
        let pub_nonce = self.get_public_nonce(index)?;
        let ai = self.get_musig_scalar(pub_key)?;
        let e = self.get_challenge()?;
        let s = nonce + ai * e * secret;
        let sig = SchnorrSignature::new(pub_nonce.clone(), s);
        Some(sig)
    }

    /// Once all public keys have been collected, this function returns a reference to the joint public key as
    /// defined by the MuSig algorithm. If public keys are still being collected, this returns None.
    pub fn get_musig_scalar(&self, pub_key: &RistrettoPublicKey) -> Option<&RistrettoSecretKey> {
        let jk = match &self.state {
            MuSigState::NonceHashCollection(s) => &s.joint_key,
            MuSigState::NonceCollection(s) => &s.joint_key,
            MuSigState::SignatureCollection(s) => &s.joint_key,
            MuSigState::Finalized(s) => &s.joint_key,
            _ => return None,
        };
        match jk.index_of(pub_key) {
            Ok(i) => Some(jk.get_musig_scalar(i)),
            Err(_) => None,
        }
    }

    /// Private convenience function that returns a Failed state with the `InvalidStateTransition` error
    fn invalid_transition() -> MuSigState {
        MuSigState::Failed(MuSigError::InvalidStateTransition)
    }

    /// Implement a finite state machine. Each combination of State and Event is handled here; for each combination, a
    /// new state is determined, consuming the old one. If `MuSigState::Failed` is ever returned, the protocol must be
    /// abandoned.
    fn handle_event(self, event: MuSigEvent) -> Self {
        let state = match self.state {
            // On initialization, you can add keys until you reach `num_signers` at which point the state
            // automatically flips to `NonceHashCollection`; we're forced to use nested patterns because of error
            MuSigState::Initialization(s) => match event {
                MuSigEvent::AddKey(p) => s.add_pubkey::<D>(p),
                MuSigEvent::SetMessage(m) => s.set_message(m),
                _ => RistrettoMuSig::<D>::invalid_transition(),
            },
            // Nonce Hash collection
            MuSigState::NonceHashCollection(s) => match event {
                MuSigEvent::AddNonceHash(p, h) => s.add_nonce_hash::<D>(p, h.clone()),
                MuSigEvent::SetMessage(m) => s.set_message(m),
                _ => RistrettoMuSig::<D>::invalid_transition(),
            },
            // Nonce Collection
            MuSigState::NonceCollection(s) => match event {
                MuSigEvent::AddNonce(p, nonce) => s.add_nonce::<D>(p, nonce),
                MuSigEvent::SetMessage(m) => s.set_message::<D>(m),
                _ => RistrettoMuSig::<D>::invalid_transition(),
            },
            // Signature collection
            MuSigState::SignatureCollection(s) => match event {
                MuSigEvent::AddPartialSig(sig, validate) => s.add_partial_signature::<D>(*sig, validate),
                _ => RistrettoMuSig::<D>::invalid_transition(),
            },
            // There's no way back from a Failed State.
            MuSigState::Failed(_) => RistrettoMuSig::<D>::invalid_transition(),
            _ => RistrettoMuSig::<D>::invalid_transition(),
        };
        RistrettoMuSig {
            state,
            digest_type: PhantomData,
        }
    }
}

//------------------------------------  RistrettoMuSig Event Definitions ---------------------------------------------//

/// The set of possible input events that can occur during the MuSig signature aggregation protocol.
pub enum MuSigEvent<'a> {
    /// This event is used to add a new public key to the pool of participant keys
    AddKey(RistrettoPublicKey),
    /// Provides the message to be signed for the MuSig protocol
    SetMessage(MessageHash),
    /// This event is used by participants to commit the the public nonce that they will be using the signature
    /// aggregation ceremony
    AddNonceHash(&'a RistrettoPublicKey, MessageHash),
    /// This event is used to add a public nonce to the pool of nonces for a particular signing ceremony
    AddNonce(&'a RistrettoPublicKey, RistrettoPublicKey),
    /// In the 3rd round of MuSig, participants provide their partial signatures, after which any party can
    /// calculate the aggregated signature.
    AddPartialSig(Box<RistrettoSchnorr>, bool),
}

//-------------------------------------  RistrettoMuSig State Definitions   ------------------------------------------//

/// This (private) enum represents the set of states that define the MuSig protocol. Metadata relevant to a given
/// state is supplied as an associated struct of the same name as the struct. Illegal state transitions are prevented
/// by a) there being no way to move from a given state's methods to another state using an invalid transition and b)
/// the global `match` clause in the [RistrettoMuSig](structs.RistrettoMuSig.html) struct implementation. Any invalid
/// transition attempt leads to the `Failed` state.
enum MuSigState {
    Initialization(Initialization),
    NonceHashCollection(Box<NonceHashCollection>),
    NonceCollection(Box<NonceCollection>),
    SignatureCollection(Box<SignatureCollection>),
    Finalized(Box<FinalizedMuSig>),
    Failed(MuSigError),
}

struct Initialization {
    joint_key_builder: JKBuilder,
    message: Option<MessageHash>,
}

impl Initialization {
    pub fn new<D: Digest>(n: usize) -> Result<Initialization, MuSigError> {
        // Ristretto requires a 256 bit hash
        if D::output_size() != 32 {
            return Err(MuSigError::IncompatibleHashFunction);
        }
        let joint_key_builder = JKBuilder::new(n)?;
        Ok(Initialization {
            joint_key_builder,
            message: None,
        })
    }

    pub fn add_pubkey<D: Digest>(mut self, key: RistrettoPublicKey) -> MuSigState {
        match self.joint_key_builder.add_key(key) {
            Ok(_) => {
                if self.joint_key_builder.is_full() {
                    match self.joint_key_builder.build::<D>() {
                        Ok(jk) => MuSigState::NonceHashCollection(Box::new(NonceHashCollection::new(jk, self.message))),
                        Err(e) => MuSigState::Failed(e),
                    }
                } else {
                    MuSigState::Initialization(self)
                }
            },
            Err(e) => MuSigState::Failed(e),
        }
    }

    pub fn set_message(mut self, msg: MessageHash) -> MuSigState {
        if self.message.is_some() {
            return MuSigState::Failed(MuSigError::MessageAlreadySet);
        }
        self.message = Some(msg.to_vec());
        MuSigState::Initialization(self)
    }
}

struct NonceHashCollection {
    joint_key: JointPubKey,
    nonce_hashes: FixedSet<MessageHash>,
    message: Option<MessageHash>,
}

impl NonceHashCollection {
    fn new(joint_key: JointPubKey, msg: Option<MessageHash>) -> NonceHashCollection {
        let n = joint_key.size();
        NonceHashCollection {
            joint_key,
            nonce_hashes: FixedSet::new(n),
            message: msg,
        }
    }

    fn add_nonce_hash<D: Digest>(mut self, pub_key: &RistrettoPublicKey, hash: MessageHash) -> MuSigState {
        match self.joint_key.index_of(pub_key) {
            Ok(i) => {
                self.nonce_hashes.set_item(i, hash);
                if self.nonce_hashes.is_full() {
                    MuSigState::NonceCollection(Box::new(NonceCollection::new(self)))
                } else {
                    MuSigState::NonceHashCollection(Box::new(self))
                }
            },
            Err(_) => MuSigState::Failed(MuSigError::ParticipantNotFound),
        }
    }

    pub fn set_message(mut self, msg: MessageHash) -> MuSigState {
        if self.message.is_some() {
            return MuSigState::Failed(MuSigError::MessageAlreadySet);
        }
        self.message = Some(msg);
        MuSigState::NonceHashCollection(Box::new(self))
    }
}

struct NonceCollection {
    joint_key: JointPubKey,
    nonce_hashes: FixedSet<MessageHash>,
    public_nonces: FixedSet<RistrettoPublicKey>,
    message: Option<MessageHash>,
}

impl NonceCollection {
    fn new(init: NonceHashCollection) -> NonceCollection {
        let n = init.joint_key.size();
        NonceCollection {
            joint_key: init.joint_key,
            nonce_hashes: init.nonce_hashes,
            public_nonces: FixedSet::new(n),
            message: init.message,
        }
    }

    fn is_valid_nonce<D: Digest>(nonce: &RistrettoPublicKey, expected: &MessageHashSlice) -> bool {
        let calc = D::digest(nonce.as_bytes()).to_vec();
        &calc[..] == expected
    }

    // We definitely want to consume `nonce` here to discourage nonce re-use
    fn add_nonce<D: Digest>(mut self, pub_key: &RistrettoPublicKey, nonce: RistrettoPublicKey) -> MuSigState {
        match self.joint_key.index_of(pub_key) {
            Ok(i) => {
                // Check that the nonce matches the commitment
                let expected = self.nonce_hashes.get_item(i);
                if expected.is_none() {
                    return MuSigState::Failed(MuSigError::MissingHash);
                }
                if !NonceCollection::is_valid_nonce::<D>(&nonce, expected.unwrap()) {
                    return MuSigState::Failed(MuSigError::MismatchedNonces);
                }
                self.public_nonces.set_item(i, nonce);
                // Transition to round three iff we have all the nonces and the message has been set
                if self.public_nonces.is_full() && self.message.is_some() {
                    MuSigState::SignatureCollection(Box::new(SignatureCollection::new::<D>(self)))
                } else {
                    MuSigState::NonceCollection(Box::new(self))
                }
            },
            Err(_) => MuSigState::Failed(MuSigError::ParticipantNotFound),
        }
    }

    pub fn set_message<D: Digest>(mut self, msg: MessageHash) -> MuSigState {
        if self.message.is_some() {
            return MuSigState::Failed(MuSigError::MessageAlreadySet);
        }
        self.message = Some(msg);
        if self.public_nonces.is_full() {
            MuSigState::SignatureCollection(Box::new(SignatureCollection::new::<D>(self)))
        } else {
            MuSigState::NonceCollection(Box::new(self))
        }
    }
}

struct SignatureCollection {
    joint_key: JointPubKey,
    public_nonces: FixedSet<RistrettoPublicKey>,
    partial_signatures: FixedSet<RistrettoSchnorr>,
    challenge: RistrettoSecretKey,
}

impl SignatureCollection {
    fn new<D: Digest>(init: NonceCollection) -> SignatureCollection {
        let n = init.joint_key.size();
        let agg_nonce = init.public_nonces.sum().unwrap();
        let message = init.message.unwrap();
        let challenge =
            SignatureCollection::calculate_challenge::<D>(&agg_nonce, init.joint_key.get_joint_pubkey(), &message);
        SignatureCollection {
            joint_key: init.joint_key,
            public_nonces: init.public_nonces,
            partial_signatures: FixedSet::new(n),
            challenge,
        }
    }

    fn calculate_challenge<D: Digest>(
        r_agg: &RistrettoPublicKey,
        p_agg: &RistrettoPublicKey,
        m: &MessageHashSlice,
    ) -> RistrettoSecretKey
    {
        let e = D::new()
            .chain(r_agg.as_bytes())
            .chain(p_agg.as_bytes())
            .chain(m)
            .result();
        RistrettoSecretKey::from_bytes(&e).expect("Found a u256 that does not map to a valid Ristretto scalar")
    }

    fn validate_partial_signature<D: Digest>(&self, index: usize, signature: &RistrettoSchnorr) -> bool {
        // s_i = r_i + a_i k_i e, so
        // s_i.G = R_i + a_i P_i e
        let pub_key = self.joint_key.get_pub_keys(index);
        let a_i = self.joint_key.get_musig_scalar(index);
        let p = a_i * pub_key;
        let e = &self.challenge;
        signature.verify(&p, e)
    }

    fn calculate_agg_signature(&self) -> RistrettoSchnorr {
        self.partial_signatures.sum().unwrap()
    }

    fn set_signature<D: Digest>(mut self, index: usize, signature: RistrettoSchnorr) -> MuSigState {
        if !self.partial_signatures.set_item(index, signature) {
            return MuSigState::Failed(MuSigError::MismatchedSignatures);
        }
        if self.partial_signatures.is_full() {
            MuSigState::Finalized(Box::new(FinalizedMuSig::new(self)))
        } else {
            MuSigState::SignatureCollection(Box::new(self))
        }
    }

    fn add_partial_signature<D: Digest>(self, signature: RistrettoSchnorr, validate: bool) -> MuSigState {
        match self.public_nonces.search(signature.get_public_nonce()) {
            None => MuSigState::Failed(MuSigError::ParticipantNotFound),
            Some(i) => {
                if validate && !self.validate_partial_signature::<D>(i, &signature) {
                    MuSigState::Failed(MuSigError::InvalidPartialSignature(i))
                } else {
                    self.set_signature::<D>(i, signature)
                }
            },
        }
    }
}

struct FinalizedMuSig {
    s_agg: RistrettoSchnorr,
    challenge: RistrettoSecretKey,
    joint_key: JointPubKey,
}

impl FinalizedMuSig {
    fn new(init: SignatureCollection) -> Self {
        let s_agg = init.calculate_agg_signature();
        let joint_key = init.joint_key;
        let challenge = init.challenge;
        FinalizedMuSig {
            s_agg,
            challenge,
            joint_key,
        }
    }
}

//-------------------------------------------------------------------------------------------------------------------//
//------------------------------------               Tests                  -----------------------------------------//
//------------------------------------ -------------------------------------------------------------------------------//

#[cfg(test)]
mod test {
    use super::*;
    use crate::keys::{PublicKey, SecretKey};
    use rand::{CryptoRng, Rng};
    use sha2::Sha256;

    struct MuSigTestData {
        pub pub_keys: Vec<RistrettoPublicKey>,
        pub indices: Vec<usize>,
        // The position of the data in the sorted list
        pub secret_keys: Vec<RistrettoSecretKey>,
        pub nonces: Vec<RistrettoSecretKey>,
        pub public_nonces: Vec<RistrettoPublicKey>,
        pub r_agg: RistrettoPublicKey,
        pub nonce_hashes: Vec<MessageHash>,
        pub partial_sigs: Vec<RistrettoSchnorr>,
    }

    fn get_key_and_nonce<R: CryptoRng + Rng>(
        rng: &mut R,
    ) -> (
        RistrettoSecretKey,
        RistrettoPublicKey,
        RistrettoSecretKey,
        RistrettoPublicKey,
        MessageHash,
    ) {
        let (k, pubkey) = RistrettoPublicKey::random_keypair(rng);
        let (r, nonce) = RistrettoPublicKey::random_keypair(rng);
        let hash = Sha256::digest(nonce.as_bytes()).to_vec();
        (k, pubkey, r, nonce, hash)
    }

    /// Utility test function that creates a MuSig ceremony at Round 1, where public keys have been set and now we
    /// are ready to accept nonce hashes.
    /// You can also optionally provide a message at this stage to be signed.
    /// The function returns the MuSig struct as well as a data structure that holds the secret and public keys, the
    /// nonces and public nonces, and the nonce hashes to aid with testing
    fn create_round_one_musig(n: usize, msg: Option<&[u8]>) -> (RistrettoMuSig<Sha256>, MuSigTestData) {
        let mut rng = rand::thread_rng();
        let mut musig = RistrettoMuSig::<Sha256>::new(n);
        let mut pub_keys = Vec::with_capacity(n);
        let mut secret_keys = Vec::with_capacity(n);
        let mut nonces = Vec::with_capacity(n);
        let mut public_nonces = Vec::with_capacity(n);
        let mut nonce_hashes = Vec::with_capacity(n);
        let partial_sigs = Vec::with_capacity(n);
        for _ in 0..n {
            let (k, pk, r, pr, h) = get_key_and_nonce(&mut rng);
            secret_keys.push(k);
            pub_keys.push(pk);
            nonces.push(r);
            public_nonces.push(pr);
            nonce_hashes.push(h);
        }
        for p in &pub_keys {
            musig = musig.add_public_key(p);
        }
        let mut r_agg = public_nonces[0].clone();
        for r in public_nonces[1..].iter() {
            r_agg = r_agg + r;
        }
        assert_eq!(musig.has_failed(), false);
        if msg.is_some() {
            musig = musig.set_message(msg.unwrap());
        }
        // We should now have switched to Round 1 automatically
        assert!(musig.is_collecting_hashes());
        // Collect the positions of the pubkeys in the sorted list
        let indices = pub_keys.iter().map(|p| musig.index_of(p).unwrap()).collect();
        (musig, MuSigTestData {
            pub_keys,
            indices,
            secret_keys,
            nonces,
            public_nonces,
            r_agg,
            nonce_hashes,
            partial_sigs,
        })
    }

    /// Utility test function that creates a MuSig ceremony at Round 2 (nonce collection). Building on from
    /// `create_round_one_musig`, this function calls `MuSig::add_nonce_commitment` for each nonce hash in the test
    /// data structure leaving the MuSig structure ready to accept public nonces. If the message is supplied, it is
    /// added after the nonce commitments have been added
    fn create_round_two_musig(n: usize, msg: Option<&[u8]>) -> (RistrettoMuSig<Sha256>, MuSigTestData) {
        let (mut musig, data) = create_round_one_musig(n, None);
        for (p, h) in data.pub_keys.iter().zip(&data.nonce_hashes) {
            musig = musig.add_nonce_commitment(p, h.clone());
        }
        assert_eq!(musig.has_failed(), false);
        if msg.is_some() {
            musig = musig.set_message(&msg.unwrap());
        }
        // We should now have switched to Round 2 automatically
        assert!(musig.is_collecting_nonces());
        (musig, data)
    }

    /// Utility test function that creates a MuSig ceremony at Round 3 (signature collection). This function takes
    /// the result from `create_round_two_musig` and adds the public nonces found in `data`. If the message is
    /// provided, it is added after this. The MuSig structure that is returned is ready to accept partial signatures
    fn create_round_three_musig(n: usize, msg: Option<&[u8]>) -> (RistrettoMuSig<Sha256>, MuSigTestData) {
        let (mut musig, mut data) = create_round_two_musig(n, None);
        for (p, r) in data.pub_keys.iter().zip(&data.public_nonces) {
            musig = musig.add_nonce(p, r.clone())
        }
        if msg.is_some() {
            musig = musig.set_message(&msg.unwrap());
        }
        assert!(musig.is_collecting_signatures());
        let e = musig.get_challenge().unwrap();
        // Calculate partial signatures
        for (i, r) in data.nonces.iter().enumerate() {
            let k = data.secret_keys.get(i).unwrap();
            let ai = musig.get_musig_scalar(data.pub_keys.get(i).unwrap()).unwrap();
            let sig = r + ai * e * k;
            data.partial_sigs
                .push(RistrettoSchnorr::new(data.public_nonces[i].clone(), sig));
        }
        (musig, data)
    }

    /// Utility test function to create a finalised MuSig struct: All partial signatures have been collected and
    /// verified, and the sum of partial signatures is returned independently
    fn create_final_musig(n: usize, msg: &[u8]) -> (RistrettoMuSig<Sha256>, MuSigTestData, RistrettoSchnorr) {
        let (mut musig, data) = create_round_three_musig(n, Some(msg));
        assert_eq!(musig.has_failed(), false);
        // Add the partial signatures
        for s in data.partial_sigs.iter() {
            musig = musig.add_signature(&s, true);
            assert_eq!(
                musig.has_failed(),
                false,
                "Partial signature addition failed. {:?}",
                musig.failure_reason()
            );
        }
        let mut iter = data.partial_sigs.iter();
        let v0 = iter.next().unwrap().clone();
        let sum = iter.fold(v0, |acc, s| s + acc);
        assert!(musig.is_finalized());
        (musig, data, sum)
    }

    #[test]
    fn add_too_many_pub_keys() {
        let mut rng = rand::thread_rng();
        let musig = RistrettoMuSig::<Sha256>::new(2);
        let (_, p1) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, p2) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, p3) = RistrettoPublicKey::random_keypair(&mut rng);
        let musig = musig.add_public_key(&p1).add_public_key(&p2);
        assert_eq!(musig.has_failed(), false);
        let musig = musig.add_public_key(&p3);
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::InvalidStateTransition));
    }

    #[test]
    fn zero_sized_musig() {
        let musig = RistrettoMuSig::<Sha256>::new(0);
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::NotEnoughParticipants));
    }

    #[test]
    fn duplicate_pub_key() {
        let mut rng = rand::thread_rng();
        let musig = RistrettoMuSig::<Sha256>::new(3);
        let (_, p1) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, p2) = RistrettoPublicKey::random_keypair(&mut rng);
        let musig = musig.add_public_key(&p1).add_public_key(&p2).add_public_key(&p1);
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::DuplicatePubKey));
    }

    #[test]
    fn add_msg_in_round_one() {
        let (mut musig, _) = create_round_one_musig(5, None);
        musig = musig.set_message(b"Hello Discworld");
        assert_eq!(musig.has_failed(), false);
        // Haven't collected nonces yet, so challenge is still undefined
        assert_eq!(musig.get_challenge(), None);
    }

    #[test]
    fn must_wait_until_full() {
        let mut rng = rand::thread_rng();
        let musig = RistrettoMuSig::<Sha256>::new(3);
        let (k1, p1) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, p2) = RistrettoPublicKey::random_keypair(&mut rng);
        let mut musig = musig.add_public_key(&p1).add_public_key(&p2);
        assert_eq!(musig.has_failed(), false);
        musig = musig.add_nonce_commitment(&p1, k1.to_vec());
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::InvalidStateTransition));
    }

    #[test]
    fn cannot_add_more_keys_after_round0() {
        let mut rng = rand::thread_rng();
        let (_, p) = RistrettoPublicKey::random_keypair(&mut rng);
        let (mut musig, _) = create_round_one_musig(25, None);
        // We can't add pub keys anymore!
        musig = musig.add_public_key(&p);
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::InvalidStateTransition));
    }

    #[test]
    fn must_wait_for_all_nonce_hashes() {
        let (mut musig, data) = create_round_one_musig(3, None);
        musig = musig.add_nonce_commitment(&data.pub_keys[2], data.nonce_hashes[2].clone());
        assert_eq!(musig.has_failed(), false);
        // Try add nonce before all hashes have been collected
        musig = musig.add_nonce(&data.pub_keys[2], data.public_nonces[2].clone());
        assert_eq!(musig.failure_reason(), Some(MuSigError::InvalidStateTransition));
    }

    #[test]
    fn can_add_hashes_in_any_order() {
        let (mut musig, data) = create_round_one_musig(3, None);
        musig = musig
            .add_nonce_commitment(&data.pub_keys[2], data.nonce_hashes[2].clone())
            .add_nonce_commitment(&data.pub_keys[0], data.nonce_hashes[0].clone())
            .add_nonce_commitment(&data.pub_keys[1], data.nonce_hashes[1].clone());
        assert!(musig.is_collecting_nonces());
    }

    #[test]
    fn can_add_nonces_in_any_order() {
        let (mut musig, data) = create_round_two_musig(3, Some(b"message"));
        musig = musig
            .add_nonce(&data.pub_keys[2], data.public_nonces[2].clone())
            .add_nonce(&data.pub_keys[0], data.public_nonces[0].clone())
            .add_nonce(&data.pub_keys[1], data.public_nonces[1].clone());
        assert!(musig.is_collecting_signatures());
    }

    #[test]
    fn invalid_nonce_causes_failure() {
        let (mut musig, data) = create_round_two_musig(25, None);
        musig = musig.add_nonce(&data.pub_keys[0], data.public_nonces[1].clone());
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::MismatchedNonces));
    }

    #[test]
    fn invalid_partial_signature_causes_failure() {
        let mut rng = rand::thread_rng();
        let (mut musig, data) = create_round_three_musig(15, Some(b"message"));
        let s = RistrettoSecretKey::random(&mut rng);
        // Create a signature with a valid nonce, but the signature is invalid
        let bad_sig = RistrettoSchnorr::new(data.public_nonces[1].clone(), s);
        let index = data.indices[1];
        musig = musig.add_signature(&bad_sig, true);
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::InvalidPartialSignature(index)));
    }

    #[test]
    fn bad_partial_signature_causes_failure() {
        let mut rng = rand::thread_rng();
        let (mut musig, _) = create_round_three_musig(3, Some(b"message"));
        let (s, r) = RistrettoPublicKey::random_keypair(&mut rng);
        // Create a signature with an invalid nonce
        let bad_sig = RistrettoSchnorr::new(r, s);
        musig = musig.add_signature(&bad_sig, true);
        assert!(musig.has_failed());
        assert_eq!(musig.failure_reason(), Some(MuSigError::ParticipantNotFound));
    }

    #[test]
    fn adding_pubkey_after_initialization_causes_failure() {
        let mut rng = rand::thread_rng();
        let (_, p, _, _, _) = get_key_and_nonce(&mut rng);
        let (mut musig, _) = create_round_one_musig(5, None);
        musig = musig.add_public_key(&p);
        assert!(musig.has_failed());

        let (mut musig, _) = create_round_two_musig(5, None);
        musig = musig.add_public_key(&p);
        assert!(musig.has_failed());

        let (mut musig, _) = create_round_three_musig(5, Some(b"message"));
        musig = musig.add_public_key(&p);
        assert!(musig.has_failed());
    }

    #[test]
    fn aggregated_signature_validates() {
        let (musig, data, s_agg) = create_final_musig(15, b"message");
        let sig = musig.get_aggregated_signature().unwrap();
        let p_agg = musig.get_aggregated_public_key().unwrap();
        let m_hash = Sha256::digest(b"message");
        let challenge = Sha256::new()
            .chain(data.r_agg.as_bytes())
            .chain(p_agg.as_bytes())
            .chain(&m_hash)
            .result();
        assert!(sig.verify_challenge(p_agg, &challenge));
        assert_eq!(&s_agg, sig);
    }

    #[test]
    fn multiparty_musig() {
        // Everyone sets up their MuSig
        let m = b"Discworld".to_vec();
        let (mut alice, data) = create_round_three_musig(2, Some(&m));
        // Aliases to Alice's and Bob's public key
        let p_a = data.pub_keys.get(0).unwrap();
        let p_b = data.pub_keys.get(1).unwrap();
        let mut bob = RistrettoMuSig::<Sha256>::new(2);
        // Setup Bob's MuSig
        bob = bob
            .add_public_key(p_b)
            .add_public_key(p_a)
            // Round 1 - Collect nonce hashes
            .add_nonce_commitment(p_b, data.nonce_hashes[1].clone())
            .add_nonce_commitment(p_a, data.nonce_hashes[0].clone())
            // Round 2 - Collect Nonces
            .add_nonce(p_b, data.public_nonces[1].clone())
            .add_nonce(p_a, data.public_nonces[0].clone())
            .set_message(&m);
        assert!(bob.is_collecting_signatures());
        // round 3 - Collect partial signatures
        let s_a = alice
            .calculate_partial_signature(p_a, &data.secret_keys[0], &data.nonces[0])
            .unwrap();
        let s_b = bob
            .calculate_partial_signature(p_b, &data.secret_keys[1], &data.nonces[1])
            .unwrap();
        alice = alice.add_signature(&s_a, true).add_signature(&s_b, true);
        assert!(alice.is_finalized());
        bob = bob.add_signature(&s_b, true).add_signature(&s_a, true);
        assert!(bob.is_finalized());
        assert_eq!(alice.get_aggregated_signature(), bob.get_aggregated_signature());
    }
}

#[cfg(test)]
mod test_joint_key {
    use super::*;
    use crate::{keys::PublicKey, musig::MAX_SIGNATURES};
    use sha2::Sha256;
    use tari_utilities::hex::Hex;

    #[test]
    fn zero_sized_jk() {
        let jk = JKBuilder::new(0);
        assert_eq!(jk.err().unwrap(), MuSigError::NotEnoughParticipants);
    }

    #[test]
    fn too_many_participants() {
        let jk = JKBuilder::new(MAX_SIGNATURES + 1);
        assert_eq!(jk.err().unwrap(), MuSigError::TooManyParticipants);
    }

    #[test]
    fn too_many_keys() {
        let mut rng = rand::thread_rng();
        let mut jk = JKBuilder::new(2).unwrap();
        assert_eq!(jk.num_signers(), 2);
        let (_, p1) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, p2) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, p3) = RistrettoPublicKey::random_keypair(&mut rng);
        // Add first key
        assert_eq!(jk.key_exists(&p1), false);
        assert_eq!(jk.add_key(p1).unwrap(), 1);
        assert_eq!(jk.is_full(), false);
        // Add second key
        assert_eq!(jk.key_exists(&p2), false);
        assert_eq!(jk.add_key(p2).unwrap(), 2);
        assert!(jk.is_full());
        // Try add third key
        assert_eq!(jk.key_exists(&p3), false);
        assert_eq!(jk.add_key(p3).err(), Some(MuSigError::TooManyParticipants));
    }

    #[test]
    fn duplicate_key() {
        let mut rng = rand::thread_rng();
        let mut jk = JKBuilder::new(3).unwrap();
        let (_, p1) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, p2) = RistrettoPublicKey::random_keypair(&mut rng);
        // Add first key
        assert_eq!(jk.key_exists(&p1), false);
        assert_eq!(jk.add_key(p1.clone()).unwrap(), 1);
        assert_eq!(jk.is_full(), false);
        // Add second key
        assert_eq!(jk.key_exists(&p2), false);
        assert_eq!(jk.add_key(p2).unwrap(), 2);
        assert_eq!(jk.is_full(), false);
        // Try add third key
        assert_eq!(jk.key_exists(&p1), true);
        assert_eq!(jk.add_key(p1).err(), Some(MuSigError::DuplicatePubKey));
    }

    #[test]
    fn three_keys() {
        let mut key_builder = JKBuilder::new(3).unwrap();
        assert_eq!(key_builder.num_signers(), 3);
        let p1 =
            RistrettoPublicKey::from_hex("aa52e000df2e16f55fb1032fc33bc42742dad6bd5a8fc0be0167436c5948501f").unwrap();
        let p2 =
            RistrettoPublicKey::from_hex("46376b80f409b29dc2b5f6f0c52591990896e5716f41477cd30085ab7f10301e").unwrap();
        let p3 =
            RistrettoPublicKey::from_hex("e0c418f7c8d9c4cdd7395b93ea124f3ad99021bb681dfc3302a9d99a2e53e64e").unwrap();
        assert_eq!(
            key_builder.add_keys(vec![p1.clone(), p2.clone(), p3.clone()]).unwrap(),
            3
        );
        assert!(key_builder.is_full());
        let joint_key = key_builder.build::<Sha256>().unwrap();
        assert_eq!(joint_key.size(), 3);
        // The keys have been sorted
        assert_eq!(joint_key.get_pub_keys(0), &p2);
        assert_eq!(joint_key.get_pub_keys(1), &p1);
        assert_eq!(joint_key.get_pub_keys(2), &p3);
        // Calculate ell and partials
        let ell = Sha256::new()
            .chain(p2.as_bytes())
            .chain(p1.as_bytes())
            .chain(p3.as_bytes())
            .result()
            .to_vec();
        // Check Ell
        let ell = RistrettoSecretKey::from_vec(&ell).unwrap();
        assert_eq!(joint_key.get_common(), &ell);
        // Check partial scalars
        let hash = |p: &RistrettoPublicKey| {
            let h = Sha256::new()
                .chain(ell.as_bytes())
                .chain(p.as_bytes())
                .result()
                .to_vec();
            RistrettoSecretKey::from_vec(&h).unwrap()
        };
        let a1 = hash(&p1);
        let a2 = hash(&p2);
        let a3 = hash(&p3);
        assert_eq!(joint_key.get_musig_scalar(0), &a2);
        assert_eq!(joint_key.get_musig_scalar(1), &a1);
        assert_eq!(joint_key.get_musig_scalar(2), &a3);
        // Check joint public key
        let key = (a1 * p1) + (a2 * p2) + (a3 * p3);
        assert_eq!(joint_key.get_joint_pubkey(), &key);
    }
}
