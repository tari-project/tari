// Copyright 2019. The Tari Project
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
    transaction::{OutputFeatures, TransactionOutput, MAX_RANGE_PROOF_RANGE},
    transaction_protocol::{
        build_challenge,
        recipient::RecipientSignedTransactionData as RD,
        sender::SingleRoundSenderData as SD,
        TransactionProtocolError as TPE,
    },
    types::{CommitmentFactory, PublicKey, RangeProofService, SecretKey as SK, Signature, TariCommitment},
};
use tari_crypto::{keys::PublicKey as PK, range_proof::RangeProofService as RangeProofServiceTrait};

/// SingleReceiverTransactionProtocol represents the actions taken by the single receiver in the one-round Tari
/// transaction protocol. The procedure is straightforward. Upon receiving the sender's information, the receiver:
/// * Checks the input for validity
/// * Constructs his output, range proof and signature
/// * Constructs the reply
/// If any step fails, an error is returned.
pub(super) struct SingleReceiverTransactionProtocol {}

impl SingleReceiverTransactionProtocol {
    pub fn create(sender_info: &SD, nonce: SK, spending_key: SK, features: OutputFeatures) -> Result<RD, TPE> {
        SingleReceiverTransactionProtocol::validate_sender_data(sender_info)?;
        let output = SingleReceiverTransactionProtocol::build_output(sender_info, &spending_key, features)?;
        let public_nonce = PublicKey::from_secret_key(&nonce);
        let public_spending_key = PublicKey::from_secret_key(&spending_key);
        let e = build_challenge(&(&sender_info.public_nonce + &public_nonce), &sender_info.metadata);
        let signature = Signature::sign(spending_key, nonce, &e).map_err(TPE::SigningError)?;
        let data = RD {
            tx_id: sender_info.tx_id,
            output,
            public_spend_key: public_spending_key,
            partial_signature: signature,
        };
        Ok(data)
    }

    /// Validates the sender info
    fn validate_sender_data(sender_info: &SD) -> Result<(), TPE> {
        if sender_info.amount == 0 {
            return Err(TPE::ValidationError("Cannot send zero microTari".into()));
        }
        Ok(())
    }

    fn build_output(sender_info: &SD, spending_key: &SK, features: OutputFeatures) -> Result<TransactionOutput, TPE> {
        let commitment = CommitmentFactory::commit(sender_info.amount, &spending_key);
        let prover = RangeProofService::new(MAX_RANGE_PROOF_RANGE, CommitmentFactory::default())?;

        Ok(TransactionOutput::new(
            features,
            commitment,
            prover.construct_proof(&spending_key, sender_info.amount)?,
        ))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        transaction::OutputFeatures,
        transaction_protocol::{
            build_challenge,
            sender::SingleRoundSenderData,
            single_receiver::SingleReceiverTransactionProtocol,
            TransactionMetadata,
            TransactionProtocolError,
        },
        types::{PublicKey, SecretKey, TariCommitmentValidate},
    };
    use rand::OsRng;
    use tari_crypto::keys::{PublicKey as PK, SecretKey as SK};

    fn generate_output_parms() -> (SecretKey, SecretKey, OutputFeatures) {
        let mut rng = OsRng::new().unwrap();
        let r = SecretKey::random(&mut rng);
        let k = SecretKey::random(&mut rng);
        let of = OutputFeatures::empty();
        (r, k, of)
    }

    #[test]
    fn zero_amount_fails() {
        let info = SingleRoundSenderData::default();
        let (r, k, of) = generate_output_parms();
        match SingleReceiverTransactionProtocol::create(&info, r, k, of) {
            Ok(_) => panic!("Zero amounts should fail"),
            Err(TransactionProtocolError::ValidationError(s)) => assert_eq!(s, "Cannot send zero microTari"),
            Err(_) => panic!("Protocol fails for the wrong reason"),
        };
    }

    #[test]
    fn valid_request() {
        let mut rng = OsRng::new().unwrap();
        let (_xs, pub_xs) = PublicKey::random_keypair(&mut rng);
        let (_rs, pub_rs) = PublicKey::random_keypair(&mut rng);
        let (r, k, of) = generate_output_parms();
        let pubkey = PublicKey::from_secret_key(&k);
        let pubnonce = PublicKey::from_secret_key(&r);
        let m = TransactionMetadata {
            fee: 100,
            lock_height: 0,
        };
        let info = SingleRoundSenderData {
            tx_id: 500,
            amount: 1500,
            public_excess: pub_xs.clone(),
            public_nonce: pub_rs.clone(),
            metadata: m.clone(),
        };
        let prot = SingleReceiverTransactionProtocol::create(&info, r, k.clone(), of).unwrap();
        assert_eq!(prot.tx_id, 500, "tx_id is incorrect");
        // Check the signature
        assert_eq!(prot.public_spend_key, pubkey, "Public key is incorrect");
        let e = build_challenge(&(&pub_rs + &pubnonce), &m);
        assert!(
            prot.partial_signature.verify_challenge(&pubkey, &e),
            "Partial signature is incorrect"
        );
        let out = &prot.output;
        // Check the output that was constructed
        assert!(out.commitment.validate(info.amount, &k), "Output commitment is invalid");
        assert!(out.verify_range_proof(None).unwrap(), "Range proof is invalid");
        assert!(out.features.is_empty(), "Output features have changed");
    }
}
