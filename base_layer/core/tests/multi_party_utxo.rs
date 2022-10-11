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

use rand::rngs::OsRng;
use shamirsecretsharing::{combine_shares, create_shares};
use tari_common_types::types::{ComSignature, Commitment, PrivateKey, PublicKey, RangeProof};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        test_helpers,
        transaction_components::{
            EncryptedValue,
            OutputFeatures,
            TransactionInput,
            TransactionInputVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
        CryptoFactories,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::SecretKey,
    range_proof::RangeProofService,
    ristretto::pedersen::PedersenCommitment,
};
use tari_script::{script, ExecutionStack, StackItem, TariScript};
use tari_utilities::ByteArray;

fn generate_multi_party_utxo_keys_sender() -> (PrivateKey, PublicKey, PrivateKey, PublicKey) {
    // Public nonce and offset key [eqn (2)/(10)]
    let sender_nonce = test_helpers::generate_keys();
    let sender_offset_key = test_helpers::generate_keys();

    (
        sender_nonce.k,
        sender_nonce.pk,
        sender_offset_key.k,
        sender_offset_key.pk,
    )
}

fn create_spending_key_shares_receiver(
    factories: &CryptoFactories,
    m: u8,
    n: u8,
) -> (PrivateKey, PrivateKey, PublicKey, PedersenCommitment, Vec<Vec<u8>>) {
    // Generate the spending key share
    let spending_key_share = test_helpers::generate_keys();
    // SSS (Keep share 1, each other party gets 1 shard)
    let mut secret_buffer = [0u8; shamirsecretsharing::DATA_SIZE];
    secret_buffer[0..32].copy_from_slice(spending_key_share.k.as_bytes());
    let spending_key_shards = create_shares(&secret_buffer, n, m).unwrap();
    // Share of the commitment
    let commitment_share = factories.commitment.commit_value(&spending_key_share.k, 0);
    assert_eq!(&spending_key_share.pk, commitment_share.as_public_key());
    // Share of the public nonce
    let nonce_b = test_helpers::generate_keys(); // Kept secret
    (
        spending_key_share.k,
        nonce_b.k,
        nonce_b.pk,
        commitment_share,
        spending_key_shards,
    )
}

fn aggregate_public_shares_receiver(
    value: MicroTari,
    factories: &CryptoFactories,
    commitment_shares: Vec<PedersenCommitment>,
    public_nonce_b_shares: Vec<PublicKey>,
) -> (PedersenCommitment, PedersenCommitment, PrivateKey) {
    // Aggregated value commitment (leader) [eqn (1b)]
    let commitment_value = factories.commitment.commit_value(&PrivateKey::default(), value.into());
    let commitment = commitment_shares.iter().fold(commitment_value, |acc, x| &acc + x);

    // Aggregate public nonce (leader) [eqn (3b)]
    let receiver_nonce_a = PrivateKey::random(&mut OsRng); // Leader
    let public_receiver_nonce_a = factories.commitment.commit(&PrivateKey::default(), &receiver_nonce_a); // Leader
    let public_receiver_nonce = public_nonce_b_shares
        .iter()
        .fold(public_receiver_nonce_a, |acc, x| &acc + x);

    (commitment, public_receiver_nonce, receiver_nonce_a)
}

fn create_signature_shares_receiver(
    encrypted_value: &EncryptedValue,
    features: &OutputFeatures,
    script: &TariScript,
    covenant: &Covenant,
    minimum_value_promise: MicroTari,
    public_sender_nonce: &PublicKey,
    public_sender_offset_key: &PublicKey,
    commitment: &PedersenCommitment,
    public_receiver_nonce: &PedersenCommitment,
    private_spending_key_share: &PrivateKey,
    nonce_b_private_key_share: &PrivateKey,
) -> PrivateKey {
    // Calculate challenge (all parties) [eqn (4)]
    let aggregated_public_nonce = public_receiver_nonce + public_sender_nonce;
    let e = TransactionOutput::build_metadata_signature_challenge(
        TransactionOutputVersion::get_current_version(),
        script,
        features,
        public_sender_offset_key,
        &aggregated_public_nonce,
        commitment,
        covenant,
        encrypted_value,
        minimum_value_promise,
    );
    let e = PrivateKey::from_bytes(&e).unwrap();

    // Create commitment signature parts [eqn (5b)]
    let ex = &e * private_spending_key_share;
    nonce_b_private_key_share + &ex // u
}

#[allow(clippy::too_many_arguments)]
fn aggregate_signature_shares_receiver(
    factories: &CryptoFactories,
    value: MicroTari,
    encrypted_value: &EncryptedValue,
    features: &OutputFeatures,
    script: &TariScript,
    covenant: &Covenant,
    minimum_value_promise: MicroTari,
    public_sender_nonce: &PublicKey,
    public_sender_offset_key: &PublicKey,
    commitment: &PedersenCommitment,
    public_receiver_nonce: &PedersenCommitment,
    receiver_nonce_a: &PrivateKey,
    signature_shares: Vec<PrivateKey>,
) -> (ComSignature, Vec<u8>) {
    // Calculate challenge [eqn (4)]
    let aggregated_public_nonce = public_receiver_nonce + public_sender_nonce;
    let e = TransactionOutput::build_metadata_signature_challenge(
        TransactionOutputVersion::get_current_version(),
        script,
        features,
        public_sender_offset_key,
        &aggregated_public_nonce,
        commitment,
        covenant,
        encrypted_value,
        minimum_value_promise,
    );
    let e = PrivateKey::from_bytes(&e).unwrap();

    // Combine the commitment signature parts (leader) [eqn (5b)]
    let value_as_private_key = PrivateKey::from(value.as_u64());

    let ev = &e * &value_as_private_key;
    let sig_a_term = receiver_nonce_a + &ev;
    let sig_b_term = signature_shares.iter().fold(PrivateKey::default(), |acc, x| &acc + x);

    let receiver_metadata_signature = ComSignature::new(public_receiver_nonce.clone(), sig_b_term, sig_a_term);
    assert!(receiver_metadata_signature.verify(commitment, &e, factories.commitment.as_ref()));

    // Create the multi-party range proof
    // TODO: Multi-party range proof
    let spending_key = PrivateKey::default();
    let range_proof = factories
        .range_proof
        .construct_proof(&spending_key, MicroTari::zero().as_u64())
        .unwrap();

    (receiver_metadata_signature, range_proof)
}

#[allow(clippy::too_many_arguments)]
fn finalize_multi_party_utxo_sender(
    encrypted_value: &EncryptedValue,
    factories: &CryptoFactories,
    features: &OutputFeatures,
    script: &TariScript,
    covenant: &Covenant,
    minimum_value_promise: MicroTari,
    private_sender_nonce: &PrivateKey,
    public_sender_nonce: &PublicKey,
    private_sender_offset_key: &PrivateKey,
    public_sender_offset_key: &PublicKey,
    commitment: &PedersenCommitment,
    range_proof: &[u8],
    public_receiver_nonce: &PedersenCommitment,
    receiver_metadata_signature: &ComSignature,
) -> TransactionOutput {
    // Calculate challenge [eqn (4)]
    let aggregated_public_nonce = public_receiver_nonce + public_sender_nonce;
    let e = TransactionOutput::build_metadata_signature_challenge(
        TransactionOutputVersion::get_current_version(),
        script,
        features,
        public_sender_offset_key,
        &aggregated_public_nonce,
        commitment,
        covenant,
        encrypted_value,
        minimum_value_promise,
    );
    let e = PrivateKey::from_bytes(&e).unwrap();

    // Create metadata signature [eqn (11)]
    let sig_a_term = PrivateKey::default();
    let e_ko = &e * private_sender_offset_key;
    let sig_b_term = private_sender_nonce + &e_ko;
    let sender_metadata_signature =
        ComSignature::new(Commitment::from_public_key(public_sender_nonce), sig_b_term, sig_a_term);
    assert!(sender_metadata_signature.verify(
        &PedersenCommitment::from_public_key(public_sender_offset_key),
        &e,
        factories.commitment.as_ref()
    ));

    // Create aggregated metadata signature [eqn (12)]
    let aggregated_metadata_signature = &sender_metadata_signature + receiver_metadata_signature;

    // Sender: Finalizes UTXO
    TransactionOutput::new_current_version(
        features.clone(),
        commitment.clone(),
        RangeProof::from(range_proof.to_vec()),
        script.clone(),
        public_sender_offset_key.clone(),
        aggregated_metadata_signature,
        covenant.clone(),
        encrypted_value.clone(),
        minimum_value_promise,
    )
}

fn generate_sender_nonces_for_spending() -> (PrivateKey, PublicKey) {
    // Public nonce and offset key [eqn (14d)]
    let sender_nonce = test_helpers::generate_keys();

    (sender_nonce.k, sender_nonce.pk)
}

fn generate_script_key_for_spending() -> (PrivateKey, PublicKey) {
    // Public nonce and offset key [eqn (14d)]
    let script_key = test_helpers::generate_keys();

    (script_key.k, script_key.pk)
}

fn generate_aggregate_public_share_for_spending(
    factories: &CryptoFactories,
    // Shared amongst parties
    public_nonce_b_shares: Vec<PublicKey>,
) -> (PedersenCommitment, PrivateKey) {
    // Aggregate public nonce (leader) [eqn (14d)]
    let sender_nonce_a = PrivateKey::random(&mut OsRng); // Leader
    let public_sender_nonce_a = factories.commitment.commit(&PrivateKey::default(), &sender_nonce_a); // Leader
    let public_sender_nonce = public_nonce_b_shares
        .iter()
        .fold(public_sender_nonce_a, |acc, x| &acc + x);

    (public_sender_nonce, sender_nonce_a)
}

fn create_signature_shares_sender_for_spending(
    // Public data
    public_sender_nonce: &PedersenCommitment,
    input_data: &ExecutionStack,
    script: &TariScript,
    commitment: &PedersenCommitment,
    // Shared amongst parties
    public_script_key: &PublicKey,
    // Private data
    private_spending_key_share: &PrivateKey,
    nonce_b_private_key_share: &PrivateKey,
) -> PrivateKey {
    // Calculate challenge [eqn (14d)]
    let e = TransactionInput::build_script_challenge(
        TransactionInputVersion::get_current_version(),
        public_sender_nonce,
        script,
        input_data,
        public_script_key,
        commitment,
    );
    let e = PrivateKey::from_bytes(&e).unwrap();

    // Create commitment signature parts [eqn (14d)]
    let ek = &e * private_spending_key_share;
    nonce_b_private_key_share + &ek // part of 'b_S'
}

#[allow(clippy::too_many_arguments)]
fn finalize_multi_party_transaction_input_sender(
    factories: &CryptoFactories,
    // Public data
    public_sender_nonce: &PedersenCommitment,
    input_data: &ExecutionStack,
    script: &TariScript,
    commitment: &PedersenCommitment,
    encrypted_value: &EncryptedValue,
    features: &OutputFeatures,
    covenant: &Covenant,
    minimum_value_promise: MicroTari,
    public_sender_offset_key: &PublicKey,
    // Shared amongst parties
    value: MicroTari,
    public_script_key: &PublicKey,
    signature_shares: Vec<PrivateKey>,
    // Private data
    private_script_key: &PrivateKey,
    sender_nonce_a: &PrivateKey,
) -> TransactionInput {
    // Calculate challenge (leader) [eqn (14d)]
    let e = TransactionInput::build_script_challenge(
        TransactionInputVersion::get_current_version(),
        public_sender_nonce,
        script,
        input_data,
        public_script_key,
        commitment,
    );
    let e = PrivateKey::from_bytes(&e).unwrap();

    // Commitment signature 'a_Si' term (leader) [eqn (14d)]
    let value_as_private_key = PrivateKey::from(value.as_u64());
    let ev = &e * &value_as_private_key;
    let sig_a_term = sender_nonce_a + &ev;

    // Commitment signature 'b_Si' term (leader) [eqn (14d)]
    let ek_s = &e * private_script_key;
    let sig_b_term = signature_shares.iter().fold(ek_s, |acc, x| &acc + x);

    // Finalize the script signature (leader) [eqn (14d)]
    let sender_script_signature = ComSignature::new(public_sender_nonce.clone(), sig_b_term, sig_a_term);
    assert!(sender_script_signature.verify(&(commitment + public_script_key), &e, factories.commitment.as_ref()));

    TransactionInput::new_with_output_data(
        TransactionInputVersion::get_current_version(),
        features.clone(),
        commitment.clone(),
        script.clone(),
        input_data.clone(),
        sender_script_signature,
        public_sender_offset_key.clone(),
        covenant.clone(),
        encrypted_value.clone(),
        minimum_value_promise,
    )
}

#[test]
#[allow(clippy::too_many_lines)]
// Refer to 'RFC-0201_TariScript.html#transaction-output-changes' and
// 'RFC-0201_TariScript.html#multi-party-transaction-output' for equation numbers used below
fn multi_party_utxo() {
    const SSS_RECEIVER_PARTIES: u8 = 3;
    const SSS_THRESHOLD: u8 = 2;

    // ---------------------------------------------------------
    // Create the multi-party UTXO
    // ---------------------------------------------------------

    // 1 Sender

    // 1.1 Sender data
    let factories = CryptoFactories::default();
    let value = MicroTari::from(500_000_000);
    let encrypted_value = EncryptedValue::default();
    let features = OutputFeatures::default();
    let script = script!(Nop);
    let covenant = Covenant::default();
    let minimum_value_promise = MicroTari::zero();

    // 1.2 Sender keys
    let (private_sender_nonce, public_sender_nonce, private_sender_offset_key, public_sender_offset_key) =
        generate_multi_party_utxo_keys_sender();

    // 2 Receiver

    // 2.1 Key shares and shards (per party)

    // 2.1.1 Receiver party 1 key shares and shards
    let (
        // Private data
        party_1_spending_key_share,
        party_1_nonce_b_private_key,
        // Public data
        party_1_nonce_b_public_key,
        party_1_commitment_share,
        // Shards - to share confidentially
        party_1_spending_key_shards,
    ) = create_spending_key_shares_receiver(&factories, SSS_THRESHOLD, SSS_RECEIVER_PARTIES);

    // 2.1.2 Receiver party 2 key shares and shards
    let (
        // Private data
        party_2_spending_key_share,
        party_2_nonce_b_private_key,
        // Public data
        party_2_nonce_b_public_key,
        party_2_commitment_share,
        // Shards - to share confidentially
        party_2_spending_key_shards,
    ) = create_spending_key_shares_receiver(&factories, SSS_THRESHOLD, SSS_RECEIVER_PARTIES);

    // 2.1.3 Receiver party 3 key shares and shards
    let (
        // Private data
        party_3_spending_key_share,
        party_3_nonce_b_private_key,
        // Public data
        party_3_nonce_b_public_key,
        party_3_commitment_share,
        // Shards - to share confidentially
        party_3_spending_key_shards,
    ) = create_spending_key_shares_receiver(&factories, SSS_THRESHOLD, SSS_RECEIVER_PARTIES);

    // 2.2 Exchange shards with other parties (This is plain SSS, not verifiable without leaking the shares)

    // 2.2.1 Party 1's shards to keep (from self and other parties)
    #[allow(unused_variables)]
    let party_1_shard_from_party_1 = party_1_spending_key_shards[0].clone();
    #[allow(unused_variables)]
    let party_1_shard_from_party_2 = party_2_spending_key_shards[1].clone();
    #[allow(unused_variables)]
    let party_1_shard_from_party_3 = party_3_spending_key_shards[1].clone();

    // 2.2.2 Party 2's shards to keep (from self and other parties)
    #[allow(unused_variables)]
    let party_2_shard_from_party_2 = party_2_spending_key_shards[0].clone();
    #[allow(unused_variables)]
    let party_2_shard_from_party_1 = party_1_spending_key_shards[1].clone();
    #[allow(unused_variables)]
    let party_2_shard_from_party_3 = party_3_spending_key_shards[2].clone();

    // 2.2.3 Party 3's shards to keep (from self and other parties)
    #[allow(unused_variables)]
    let party_3_shard_from_party_3 = party_3_spending_key_shards[0].clone();
    #[allow(unused_variables)]
    let party_3_shard_from_party_1 = party_1_spending_key_shards[2].clone();
    #[allow(unused_variables)]
    let party_3_shard_from_party_2 = party_2_spending_key_shards[2].clone();

    // 2.2.4 Test SSS shards - not an actual protocol step, this should not be done as it will leak the shares
    let party_1_spending_key_share_test = combine_shares(&[party_2_shard_from_party_1, party_3_shard_from_party_1])
        .unwrap()
        .unwrap();
    assert_eq!(
        party_1_spending_key_share,
        PrivateKey::from_bytes(&party_1_spending_key_share_test[0..32]).unwrap()
    );

    let party_2_spending_key_share_test =
        combine_shares(&[party_1_shard_from_party_2.clone(), party_3_shard_from_party_2.clone()])
            .unwrap()
            .unwrap();
    assert_eq!(
        party_2_spending_key_share,
        PrivateKey::from_bytes(&party_2_spending_key_share_test[0..32]).unwrap()
    );

    let party_3_spending_key_share_test = combine_shares(&[party_1_shard_from_party_3, party_2_shard_from_party_3])
        .unwrap()
        .unwrap();
    assert_eq!(
        party_3_spending_key_share,
        PrivateKey::from_bytes(&party_3_spending_key_share_test[0..32]).unwrap()
    );

    // 2.3 Receiver aggregate public shares (leader)
    let (commitment, public_receiver_nonce, receiver_nonce_a) = aggregate_public_shares_receiver(
        value,
        &factories,
        vec![
            party_1_commitment_share,
            party_2_commitment_share,
            party_3_commitment_share,
        ],
        vec![
            party_1_nonce_b_public_key,
            party_2_nonce_b_public_key,
            party_3_nonce_b_public_key,
        ],
    );

    // 2.4 Receiver party signature shares (per party)

    // 2.4.1 Receiver party 1 signature shares
    let party_1_signature_share = create_signature_shares_receiver(
        // Public data
        &encrypted_value,
        &features,
        &script,
        &covenant,
        minimum_value_promise,
        &public_sender_nonce,
        &public_sender_offset_key,
        &commitment,
        &public_receiver_nonce,
        // Private data
        &party_1_spending_key_share,
        &party_1_nonce_b_private_key,
    );

    // 2.4.2 Receiver party 2 signature shares
    let party_2_signature_share = create_signature_shares_receiver(
        // Public data
        &encrypted_value,
        &features,
        &script,
        &covenant,
        minimum_value_promise,
        &public_sender_nonce,
        &public_sender_offset_key,
        &commitment,
        &public_receiver_nonce,
        // Private data
        &party_2_spending_key_share,
        &party_2_nonce_b_private_key,
    );

    // 2.4.3 Receiver party 3 signature shares
    let party_3_signature_share = create_signature_shares_receiver(
        // Public data
        &encrypted_value,
        &features,
        &script,
        &covenant,
        minimum_value_promise,
        &public_sender_nonce,
        &public_sender_offset_key,
        &commitment,
        &public_receiver_nonce,
        // Private data
        &party_3_spending_key_share,
        &party_3_nonce_b_private_key,
    );

    // 2.5 Receiver aggregate signature shares (leader)
    let (receiver_metadata_signature, range_proof) = aggregate_signature_shares_receiver(
        &factories,
        value,
        &encrypted_value,
        &features,
        &script,
        &covenant,
        minimum_value_promise,
        &public_sender_nonce,
        &public_sender_offset_key,
        &commitment,
        &public_receiver_nonce,
        &receiver_nonce_a,
        vec![
            party_1_signature_share,
            party_2_signature_share,
            party_3_signature_share,
        ],
    );

    // 3 Sender

    // 3.1 Finalize UTXO
    let utxo = finalize_multi_party_utxo_sender(
        &encrypted_value,
        &factories,
        &features,
        &script,
        &covenant,
        minimum_value_promise,
        &private_sender_nonce,
        &public_sender_nonce,
        &private_sender_offset_key,
        &public_sender_offset_key,
        // Receiver values to the sender {C_i, range_proof, R_MRi, s_MRi=(a_MRi,b_MRi,R_MRi)}
        &commitment,
        &range_proof,
        &public_receiver_nonce,
        &receiver_metadata_signature,
    );
    utxo.verify_metadata_signature().unwrap();
    // TODO: Multi-party range proof
    // utxo.verify_range_proof(&factories.range_proof).unwrap();

    // ---------------------------------------------------------
    // Spend the multi-party UTXO
    // ---------------------------------------------------------

    // 1 Select threshold sender parties & reconstruct missing sender spending key share
    //   (party_1 + party_3 are available, use shards from party_2 to reconstruct)
    let party_2_spending_key_share_reconstructed =
        combine_shares(&[party_1_shard_from_party_2, party_3_shard_from_party_2])
            .unwrap()
            .unwrap();

    // 2 Sender parties create private-public nonce shares

    // 2.1 Sender party 1 private-public nonce shares
    let (party_1_private_nonce, party_1_public_nonce) = generate_sender_nonces_for_spending();

    // 2.2 Sender party 2 private-public nonce shares (Leader)
    let (party_2_private_nonce, party_2_public_nonce) = generate_sender_nonces_for_spending();

    // 2.3 Sender party 3 private-public nonce shares
    let (party_3_private_nonce, party_3_public_nonce) = generate_sender_nonces_for_spending();

    // 3 Sender leader creates private-public script key
    let (private_script_key, public_script_key) = generate_script_key_for_spending();

    // 4 Aggregate public nonce shares (leader)
    let (public_sender_nonce, sender_nonce_a) = generate_aggregate_public_share_for_spending(&factories, vec![
        party_1_public_nonce,
        party_2_public_nonce,
        party_3_public_nonce,
    ]);

    // 5 Sender parties create signature shares

    let input_data = ExecutionStack::new(vec![StackItem::PublicKey(public_script_key.clone())]);

    // 5.1 Sender party 1 signature share
    let party_1_sender_signature_share = create_signature_shares_sender_for_spending(
        // Public data
        &public_sender_nonce,
        &input_data,
        &utxo.script,
        &utxo.commitment,
        // Shared amongst parties
        &public_script_key,
        // Private data
        &party_1_spending_key_share,
        &party_1_private_nonce,
    );

    // 5.2 Sender party 2 signature share
    let party_2_sender_signature_share = create_signature_shares_sender_for_spending(
        // Public data
        &public_sender_nonce,
        &input_data,
        &utxo.script,
        &utxo.commitment,
        // Shared amongst parties
        &public_script_key,
        // Private data
        &PrivateKey::from_bytes(&party_2_spending_key_share_reconstructed[0..32]).unwrap(),
        &party_2_private_nonce,
    );

    // 5.3 Sender party 3 signature share
    let party_3_sender_signature_share = create_signature_shares_sender_for_spending(
        // Public data
        &public_sender_nonce,
        &input_data,
        &utxo.script,
        &utxo.commitment,
        // Shared amongst parties
        &public_script_key,
        // Private data
        &party_3_spending_key_share,
        &party_3_private_nonce,
    );

    // 6 Finalize transaction input (leader)
    let transaction_input = finalize_multi_party_transaction_input_sender(
        &factories,
        // Public data
        &public_sender_nonce,
        &input_data,
        &utxo.script,
        &utxo.commitment,
        &utxo.encrypted_value,
        &utxo.features,
        &utxo.covenant,
        utxo.minimum_value_promise,
        &utxo.sender_offset_public_key,
        // Shared amongst parties
        value,
        &public_script_key,
        vec![
            party_1_sender_signature_share,
            party_2_sender_signature_share,
            party_3_sender_signature_share,
        ],
        // Private data (Leader)
        &private_script_key,
        &sender_nonce_a,
    );
    let script_output_public_key = transaction_input.run_script(None).unwrap();
    transaction_input
        .validate_script_signature(&script_output_public_key, factories.commitment.as_ref())
        .unwrap();
}
