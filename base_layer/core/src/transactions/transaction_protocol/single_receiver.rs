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
    transaction_protocol::{
        recipient::RecipientSignedMessage as RD,
        sender::SingleRoundSenderData as SD,
        TransactionProtocolError as TPE,
    },
    types::{CryptoFactories, PrivateKey, PublicKey},
    OutputBuilder,
    OutputFeatures,
};
use tari_crypto::keys::PublicKey as PK;

/// SingleReceiverTransactionProtocol represents the actions taken by the single receiver in the one-round Tari
/// transaction protocol. The procedure is straightforward. Upon receiving the sender's information, the receiver:
/// * Checks the input for validity
/// * Constructs his output, range proof and signature
/// * Constructs the reply
/// If any step fails, we return an error.
pub struct SingleReceiverTransactionProtocol {}

impl SingleReceiverTransactionProtocol {
    pub fn create(
        sender_info: &SD,
        nonce: PrivateKey,
        spending_key: PrivateKey,
        features: OutputFeatures,
        factories: &CryptoFactories,
    ) -> Result<RD, TPE>
    {
        SingleReceiverTransactionProtocol::validate_sender_data(sender_info)?;
        let output = OutputBuilder::new()
            .with_spending_key(spending_key)
            .with_features(features)
            .with_value(sender_info.amount)
            .build(&factories.commitment)?;
        let pub_nonce = PublicKey::from_secret_key(&nonce);
        let sum_r = &pub_nonce + &sender_info.public_nonce;
        let signature = sign!(&output, &sender_info.metadata, nonce: nonce, pub_nonce: sum_r)?;
        let data = RD {
            tx_id: sender_info.tx_id,
            output: output.as_transaction_output(factories)?,
            public_blinding_factor: output.public_blinding_factor(),
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
}

#[cfg(test)]
mod test {
    use crate::transactions::{
        tari_amount::*,
        transaction_protocol::{
            build_challenge,
            sender::SingleRoundSenderData,
            single_receiver::SingleReceiverTransactionProtocol,
            TransactionMetadata,
            TransactionProtocolError,
        },
        types::{CryptoFactories, PrivateKey, PublicKey},
        OutputBuilder,
        OutputFeatures,
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
        match SingleReceiverTransactionProtocol::create(&info, r, k, of, &factories) {
            Ok(_) => panic!("Zero amounts should fail"),
            Err(TransactionProtocolError::ValidationError(s)) => assert_eq!(s, "Cannot send zero microTari"),
            Err(_) => panic!("Protocol fails for the wrong reason"),
        };
    }

    #[test]
    fn valid_request() {
        let factories = CryptoFactories::default();
        let value = 1500 * uT;
        let output = OutputBuilder::new()
            .with_value(value)
            .build(&factories.commitment)
            .unwrap();
        let (r, r_pub) = PublicKey::random_keypair(&mut OsRng);
        let m = TransactionMetadata {
            fee: MicroTari(100),
            lock_height: 0,
        };
        let info = SingleRoundSenderData {
            tx_id: 500,
            amount: value,
            public_excess: output.public_blinding_factor(),
            public_nonce: r_pub.clone(),
            metadata: m.clone(),
            message: "".to_string(),
        };
        let prot = SingleReceiverTransactionProtocol::create(
            &info,
            r,
            output.spending_key().clone(),
            output.features().clone(),
            &factories,
        )
        .unwrap();
        assert_eq!(prot.tx_id, 500, "tx_id is incorrect");
        // Check the signature
        let bf2 = output.public_blinding_factor();
        assert_eq!(prot.public_blinding_factor, bf2, "Public key is incorrect");
        let r_pub2 = prot.partial_signature.get_public_nonce();
        let e = build_challenge(&(&r_pub + r_pub2), &m);
        assert!(
            prot.partial_signature.verify_challenge(&bf2, &e),
            "Partial signature is incorrect"
        );
        let out = &prot.output;
        // Check the output that was constructed
        assert!(
            factories
                .commitment
                .open_value(output.blinding_factor(), info.amount.into(), out.commitment()),
            "Output commitment is invalid"
        );
        assert!(
            out.verify_range_proof(&factories.range_proof).unwrap(),
            "Range proof is invalid"
        );
        assert!(out.features().flags.is_empty(), "Output features flags have changed");
    }
}
