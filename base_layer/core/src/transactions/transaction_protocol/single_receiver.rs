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

use tari_common_types::types::{PrivateKey as SK, PublicKey, RangeProof, Signature};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    errors::RangeProofError,
    keys::PublicKey as PK,
    range_proof::RangeProofService as RPS,
    tari_utilities::byte_array::ByteArray,
};

use crate::transactions::{
    crypto_factories::CryptoFactories,
    transaction_components::{
        EncryptedOpenings,
        RangeProofType,
        TransactionKernel,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
    },
    transaction_protocol::{
        recipient::RecipientSignedMessage as RD,
        sender::SingleRoundSenderData as SD,
        RecoveryData,
        TransactionProtocolError as TPE,
    },
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
        factories: &CryptoFactories,
        recovery_data: Option<&RecoveryData>,
    ) -> Result<RD, TPE> {
        SingleReceiverTransactionProtocol::validate_sender_data(sender_info)?;
        let output =
            SingleReceiverTransactionProtocol::build_output(sender_info, &spending_key, factories, recovery_data)?;
        let public_nonce = PublicKey::from_secret_key(&nonce);
        let tx_meta = if output.is_burned() {
            let mut meta = sender_info.metadata.clone();
            meta.burn_commitment = Some(output.commitment().clone());
            meta
        } else {
            sender_info.metadata.clone()
        };
        let public_spending_key = PublicKey::from_secret_key(&spending_key);
        let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
            &TransactionKernelVersion::get_current_version(),
            &(&sender_info.public_nonce + &public_nonce),
            &(&sender_info.public_excess + &public_spending_key),
            &tx_meta,
        );
        let signature = Signature::sign_raw(&spending_key, nonce, &e).map_err(TPE::SigningError)?;
        let data = RD {
            tx_id: sender_info.tx_id,
            output,
            public_spend_key: public_spending_key,
            partial_signature: signature,
            tx_metadata: tx_meta,
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
        factories: &CryptoFactories,
        recovery_data: Option<&RecoveryData>,
    ) -> Result<TransactionOutput, TPE> {
        let commitment = factories
            .commitment
            .commit_value(spending_key, sender_info.amount.into());

        let proof = factories
            .range_proof
            .construct_proof(spending_key, sender_info.amount.into())?;

        let sender_features = sender_info.features.clone();

        let encrypted_openings = recovery_data
            .as_ref()
            .map(|rd| {
                EncryptedOpenings::encrypt_openings(&rd.encryption_key, &commitment, sender_info.amount, spending_key)
            })
            .transpose()
            .map_err(|_| TPE::EncryptionError)?
            .unwrap_or_default();

        let minimum_value_promise = sender_info.minimum_value_promise;

        let partial_metadata_signature = TransactionOutput::create_receiver_partial_metadata_signature(
            TransactionOutputVersion::get_current_version(),
            sender_info.amount,
            spending_key,
            &sender_info.script,
            &sender_features,
            &sender_info.sender_offset_public_key,
            &sender_info.ephemeral_public_nonce,
            &sender_info.covenant,
            &encrypted_openings,
            minimum_value_promise,
            // TODO: Provide user options to use `RangeProofType::RevealedValue`
            RangeProofType::BulletProofPlus,
        )?;

        let output = TransactionOutput::new_current_version(
            sender_features,
            commitment,
            RangeProof::from_bytes(&proof).map_err(|_| {
                TPE::RangeProofError(RangeProofError::ProofConstructionError(
                    "Creating transaction output".to_string(),
                ))
            })?,
            sender_info.script.clone(),
            sender_info.sender_offset_public_key.clone(),
            partial_metadata_signature,
            sender_info.covenant.clone(),
            encrypted_openings,
            minimum_value_promise,
        );
        Ok(output)
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_common_types::types::{PrivateKey, PublicKey};
    use tari_crypto::{
        commitment::HomomorphicCommitmentFactory,
        keys::{PublicKey as PK, SecretKey as SK},
    };
    use tari_script::TariScript;

    use crate::transactions::{
        crypto_factories::CryptoFactories,
        tari_amount::*,
        transaction_components::{OutputFeatures, OutputType, TransactionKernel, TransactionKernelVersion},
        transaction_protocol::{
            sender::SingleRoundSenderData,
            single_receiver::SingleReceiverTransactionProtocol,
            TransactionMetadata,
            TransactionProtocolError,
        },
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
        let (r, k, _) = generate_output_parms();
        #[allow(clippy::match_wild_err_arm)]
        match SingleReceiverTransactionProtocol::create(&info, r, k, &factories, None) {
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
        let m = TransactionMetadata::new(MicroTari(100), 0);
        let script_offset_secret_key = PrivateKey::random(&mut OsRng);
        let sender_offset_public_key = PublicKey::from_secret_key(&script_offset_secret_key);
        let private_commitment_nonce = PrivateKey::random(&mut OsRng);
        let ephemeral_public_nonce = PublicKey::from_secret_key(&private_commitment_nonce);
        let script = TariScript::default();
        let info = SingleRoundSenderData {
            tx_id: 500u64.into(),
            amount: MicroTari(1500),
            public_excess: pub_xs.clone(),
            public_nonce: pub_rs.clone(),
            metadata: m.clone(),
            message: "".to_string(),
            features: of,
            script,
            sender_offset_public_key,
            ephemeral_public_nonce,
            covenant: Default::default(),
            minimum_value_promise: MicroTari::zero(),
        };
        let prot = SingleReceiverTransactionProtocol::create(&info, r, k.clone(), &factories, None).unwrap();
        assert_eq!(prot.tx_id.as_u64(), 500, "tx_id is incorrect");
        // Check the signature
        assert_eq!(prot.public_spend_key, pubkey, "Public key is incorrect");
        let excess = &pub_xs + PublicKey::from_secret_key(&k);
        let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
            &TransactionKernelVersion::get_current_version(),
            &(&pub_rs + &pubnonce),
            &excess,
            &m,
        );
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
        out.verify_range_proof(&factories.range_proof).unwrap();
        assert_eq!(
            out.features.output_type,
            OutputType::Standard,
            "Output features flags have changed"
        );
    }
}
