// Copyright 2024. The Tari Project
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
#[cfg(test)]
mod test {

    use std::{fs::File, io::Write};

    use blake2::Blake2b;
    use digest::consts::U32;
    use rand::rngs::OsRng;
    use tari_common_types::{
        tari_address::TariAddress,
        types::{Commitment, PrivateKey, PublicKey, Signature},
    };
    use tari_crypto::keys::{PublicKey as PkTrait, SecretKey as SkTrait};
    use tari_key_manager::key_manager_service::KeyManagerInterface;
    use tari_script::{ExecutionStack, Opcode::CheckMultiSigVerifyAggregatePubKey, TariScript};

    use crate::{
        consensus::DomainSeparatedConsensusHasher,
        one_sided::{public_key_to_output_encryption_key, FaucetHashDomain},
        transactions::{
            key_manager::{
                create_memory_db_key_manager,
                SecretTransactionKeyManagerInterface,
                TransactionKeyManagerBranch,
                TransactionKeyManagerInterface,
            },
            tari_amount::MicroMinotari,
            transaction_components::{
                encrypted_data::PaymentId,
                KernelFeatures,
                OutputFeatures,
                OutputFeaturesVersion,
                OutputType,
                RangeProofType,
                TransactionKernel,
                TransactionKernelVersion,
                TransactionOutput,
                TransactionOutputVersion,
                WalletOutputBuilder,
            },
            transaction_protocol::TransactionMetadata,
        },
    };

    pub async fn create_faucets(
        amount: MicroMinotari,
        num_faucets: usize,
        signature_threshold: u8,
        lock_height: u64,
        addresses: Vec<TariAddress>,
    ) -> (Vec<TransactionOutput>, TransactionKernel) {
        let mut list_of_spend_keys = Vec::new();
        let mut total_script_key = PublicKey::default();
        let key_manager = create_memory_db_key_manager();
        for address in &addresses {
            list_of_spend_keys.push(address.public_spend_key().clone());
            total_script_key = total_script_key + address.public_spend_key();
        }
        let view_key = public_key_to_output_encryption_key(&total_script_key).unwrap();
        let view_key_id = key_manager.import_key(view_key.clone()).await.unwrap();
        let address_len = addresses.len() as u8;
        let mut outputs = Vec::new();
        let mut total_private_key = PrivateKey::default();

        for _ in 0..num_faucets {
            let (spend_key_id, _spend_key_pk, script_key_id, _script_key_pk) =
                key_manager.get_next_spend_and_script_key_ids().await.unwrap();
            total_private_key = total_private_key + &key_manager.get_private_key(&spend_key_id).await.unwrap();
            let commitment = key_manager.get_commitment(&spend_key_id, &amount.into()).await.unwrap();
            let com_hash: [u8; 32] = DomainSeparatedConsensusHasher::<FaucetHashDomain, Blake2b<U32>>::new("com_hash")
                .chain(&commitment)
                .finalize()
                .into();

            let (sender_offset_key_id, sender_offset_key_pk) = key_manager
                .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
                .await
                .unwrap();
            let script = TariScript::new(vec![CheckMultiSigVerifyAggregatePubKey(
                signature_threshold,
                address_len,
                list_of_spend_keys.clone(),
                Box::new(com_hash),
            )]);
            let output = WalletOutputBuilder::new(amount, spend_key_id)
                .with_features(OutputFeatures::new(
                    OutputFeaturesVersion::get_current_version(),
                    OutputType::Standard,
                    lock_height,
                    Vec::new(),
                    None,
                    RangeProofType::RevealedValue,
                ))
                .with_script(script)
                .encrypt_data_for_recovery(&key_manager, Some(&view_key_id), PaymentId::Empty)
                .await
                .unwrap()
                .with_input_data(ExecutionStack::default())
                .with_version(TransactionOutputVersion::get_current_version())
                .with_sender_offset_public_key(sender_offset_key_pk)
                .with_script_key(script_key_id)
                .with_minimum_value_promise(amount)
                .sign_as_sender_and_receiver(&key_manager, &sender_offset_key_id)
                .await
                .unwrap()
                .try_build(&key_manager)
                .await
                .unwrap();
            outputs.push(output.to_transaction_output(&key_manager).await.unwrap());
        }
        // lets create a single kernel for all the outputs
        let r = PrivateKey::random(&mut OsRng);
        let tx_meta = TransactionMetadata::new_with_features(0.into(), 0, KernelFeatures::empty());
        let total_public_key = PublicKey::from_secret_key(&total_private_key);
        let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
            &TransactionKernelVersion::get_current_version(),
            &PublicKey::from_secret_key(&r),
            &total_public_key,
            &tx_meta,
        );
        let signature = Signature::sign_raw_uniform(&total_private_key, r, &e).unwrap();
        let excess = Commitment::from_public_key(&total_public_key);
        let kernel =
            TransactionKernel::new_current_version(KernelFeatures::empty(), 0.into(), 0, excess, signature, None);
        (outputs, kernel)
    }

    // Only run this when you want to create a new utxo file
    #[ignore]
    #[tokio::test]
    async fn print_faucet() {
        let mut addresses = Vec::new();
        addresses.push(
            TariAddress::from_base58(
                "f4bYsv3sEMroDGKMMjhgm7cp1jDShdRWQzmV8wZiD6sJPpAEuezkiHtVhn7akK3YqswH5t3sUASW7rbvPSqMBDSCSp",
            )
            .unwrap(),
        );
        addresses.push(
            TariAddress::from_base58(
                "f44jftbpTid23oDsEjTodayvMmudSr3g66R6scTJkB5911ZfJRq32FUJDD4CiQSkAPq574i8pMjqzm5RtzdH3Kuknwz",
            )
            .unwrap(),
        );
        addresses.push(
            TariAddress::from_base58(
                "f4GYN3QVRboH6uwG9oFj3LjmUd4XVd1VDYiT6rNd4gCpZF6pY7iuoCpoajfDfuPynS7kspXU5hKRMWLTP9CRjoe1hZU",
            )
            .unwrap(),
        );
        for address in &addresses {
            println!("{}", address.public_spend_key());
        }
        // lets create a faucet with 10 outputs of 1000T each
        let (outputs, kernel) = create_faucets(MicroMinotari::from(1000_000_000), 10, 2, 5, addresses).await;
        let mut utxo_file = File::create("utxos.json").expect("Could not create utxos.json");

        for output in outputs {
            dbg!(&output);
            let utxo_s = serde_json::to_string(&output).unwrap();
            utxo_file.write_all(format!("{}\n", utxo_s).as_bytes()).unwrap();
        }

        let kernel = serde_json::to_string(&kernel).unwrap();
        let _result = utxo_file.write_all(format!("{}\n", kernel).as_bytes());
    }
}
