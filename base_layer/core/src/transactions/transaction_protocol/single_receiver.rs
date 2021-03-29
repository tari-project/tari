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

use crate::transactions::{
    transaction::{OutputFeatures, TransactionOutput},
    transaction_protocol::{
        build_challenge,
        recipient::RecipientSignedMessage as RD,
        sender::SingleRoundSenderData as SD,
        RewindData,
        TransactionProtocolError as TPE,
    },
    types::{CryptoFactories, PrivateKey as SK, PrivateKey, PublicKey, RangeProof, Signature},
};
use digest::Input;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    hash::blake2::Blake256,
    keys::PublicKey as PK,
    range_proof::{RangeProofError, RangeProofService as RPS},
    tari_utilities::byte_array::ByteArray,
};

/// SingleReceiverTransactionProtocol represents the actions taken by the single receiver in the one-round Tari
/// transaction protocol. The procedure is straightforward. Upon receiving the sender's information, the receiver:
/// * Checks the input for validity
/// * Constructs his output, range proof and signature
/// * Constructs the reply
/// If any step fails, an error is returned.
pub struct SingleReceiverTransactionProtocol {}

impl SingleReceiverTransactionProtocol {
    pub fn create(
        sender_info: &SD,
        nonce: SK,
        spending_key: SK,
        features: OutputFeatures,
        factories: &CryptoFactories,
        rewind_data: Option<&RewindData>,
    ) -> Result<RD, TPE>
    {
        SingleReceiverTransactionProtocol::validate_sender_data(sender_info)?;
        let output = SingleReceiverTransactionProtocol::build_output(
            sender_info,
            &spending_key,
            features,
            factories,
            rewind_data,
        )?;
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
        if sender_info.amount == 0.into() {
            return Err(TPE::ValidationError("Cannot send zero microTari".into()));
        }
        Ok(())
    }

    fn build_output(
        sender_info: &SD,
        spending_key: &SK,
        features: OutputFeatures,
        factories: &CryptoFactories,
        rewind_data: Option<&RewindData>,
    ) -> Result<TransactionOutput, TPE>
    {
        let commitment = factories
            .commitment
            .commit_value(&spending_key, sender_info.amount.into());

        let beta_hash = Blake256::new()
            .chain(sender_info.script_hash.clone())
            .chain(features.to_bytes())
            .chain(sender_info.script_offset_public_key.clone().as_bytes())
            .result()
            .to_vec();
        let beta = PrivateKey::from_bytes(beta_hash.as_slice()).map_err(|_| TPE::SerializationError)?;

        let proof = if let Some(rewind_data) = rewind_data {
            factories.range_proof.construct_proof_with_rewind_key(
                &(spending_key.clone() + beta),
                sender_info.amount.into(),
                &rewind_data.rewind_key,
                &rewind_data.rewind_blinding_key,
                &rewind_data.proof_message,
            )?
        } else {
            factories
                .range_proof
                .construct_proof(&spending_key, sender_info.amount.into())?
        };
        Ok(TransactionOutput::new(
            features,
            commitment,
            RangeProof::from_bytes(&proof)
                .map_err(|_| TPE::RangeProofError(RangeProofError::ProofConstructionError))?,
            sender_info.script_hash.clone(),
            sender_info.script_offset_public_key.clone(),
        ))
    }
}

#[cfg(test)]
mod test {
    use crate::transactions::{
        tari_amount::*,
        transaction::OutputFeatures,
        transaction_protocol::{
            build_challenge,
            sender::SingleRoundSenderData,
            single_receiver::SingleReceiverTransactionProtocol,
            TransactionMetadata,
            TransactionProtocolError,
        },
        types::{CryptoFactories, PrivateKey, PublicKey},
    };
    use rand::rngs::OsRng;
    use tari_crypto::{
        commitment::HomomorphicCommitmentFactory,
        keys::{PublicKey as PK, SecretKey as SK},
    };

    fn generate_output_parms() -> (PrivateKey, PrivateKey, OutputFeatures) {
        let r = PrivateKey::random(&mut OsRng);
        let k = PrivateKey::random(&mut OsRng);
        let of = OutputFeatures::default();
        (r, k, of)
    }

    #[test]
    fn zero_amount_fails() {
        let factories = CryptoFactories::default();
        let info = SingleRoundSenderData::default();
        let (r, k, of) = generate_output_parms();
        match SingleReceiverTransactionProtocol::create(&info, r, k, of, &factories, None) {
            Ok(_) => panic!("Zero amounts should fail"),
            Err(TransactionProtocolError::ValidationError(s)) => assert_eq!(s, "Cannot send zero microTari"),
            Err(_) => panic!("Protocol fails for the wrong reason"),
        };
    }

    #[test]
    fn valid_request() {
        let factories = CryptoFactories::default();
        let (_xs, pub_xs) = PublicKey::random_keypair(&mut OsRng);
        let (_rs, pub_rs) = PublicKey::random_keypair(&mut OsRng);
        let (r, k, of) = generate_output_parms();
        let pubkey = PublicKey::from_secret_key(&k);
        let pubnonce = PublicKey::from_secret_key(&r);
        let m = TransactionMetadata {
            fee: MicroTari(100),
            lock_height: 0,
        };
        let info = SingleRoundSenderData {
            tx_id: 500,
            amount: MicroTari(1500),
            public_excess: pub_xs,
            public_nonce: pub_rs.clone(),
            metadata: m.clone(),
            message: "".to_string(),
            script_hash: vec![],
            script_offset_public_key: Default::default(),
        };
        let prot = SingleReceiverTransactionProtocol::create(&info, r, k.clone(), of, &factories, None).unwrap();
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
        assert!(
            factories.commitment.open_value(&k, info.amount.into(), &out.commitment),
            "Output commitment is invalid"
        );
        assert!(
            out.verify_range_proof(&factories.range_proof).unwrap(),
            "Range proof is invalid"
        );
        assert!(out.features.flags.is_empty(), "Output features flags have changed");
    }
}
