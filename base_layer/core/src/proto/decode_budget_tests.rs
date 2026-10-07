// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Tests for what decoding RPC payloads of transactions and block bodies can cost.

// Overflow in test code panics, which is the desired failure mode for a test.
#![allow(clippy::arithmetic_side_effects)]

use std::mem::size_of;

use crate::proto;

/// prost allocates one struct per decoded input/output, so their inline size is what the decode budget multiplies.
/// The large optional sub-messages are boxed in build.rs to keep it small (inputs were 776 bytes, outputs 608).
#[test]
fn boxed_sub_messages_keep_inputs_and_outputs_small() {
    assert!(
        size_of::<proto::types::TransactionInput>() <= 256,
        "TransactionInput is {} bytes",
        size_of::<proto::types::TransactionInput>()
    );
    assert!(
        size_of::<proto::types::TransactionOutput>() <= 192,
        "TransactionOutput is {} bytes",
        size_of::<proto::types::TransactionOutput>()
    );
}

mod max_size_payloads {
    use tari_comms::decode_budget::{DecodeBudget, check_decode_budget};

    use crate::proto::{
        self,
        types::{
            BuildInfo,
            ComAndPubSignature,
            Commitment,
            OutputFeatures,
            PrivateKey,
            RangeProof,
            SideChainFeature,
            SidechainId,
            Signature,
            TemplateRegistration,
            TemplateType,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
            WasmInfo,
            side_chain_feature,
            template_type,
        },
    };

    /// The decode budget of the methods that carry a block body or transaction (sync_blocks, get_state,
    /// submit_transaction)
    const BODY_MAX_ITEMS: usize = 262_144;

    // The most of each that fits in a mainnet block (weight 90,000) on its own
    const MAX_INPUTS: usize = 11_242;
    const MAX_OUTPUTS: usize = 1_666;
    const MAX_KERNELS: usize = 8_994;

    // Message instances in each fully populated element below, counting the element itself
    const INPUT_MESSAGES: usize = 13;
    const OUTPUT_MESSAGES: usize = 13;
    const KERNEL_MESSAGES: usize = 4;

    /// Fills `bytes` fields: with deterministic pseudo-random data like real hashes, keys and ciphertexts, or entirely
    /// with `0a 00`, which reads as a run of empty embedded messages to anything that enters bytes fields.
    struct Bytes {
        state: u64,
        fake_messages: bool,
    }

    impl Bytes {
        fn random() -> Self {
            Self {
                state: 0x9e37_79b9_7f4a_7c15,
                fake_messages: false,
            }
        }

        fn fake_messages() -> Self {
            Self {
                state: 0,
                fake_messages: true,
            }
        }

        fn take(&mut self, len: usize) -> Vec<u8> {
            if self.fake_messages {
                return [0x0a, 0x00].repeat(len / 2);
            }
            (0..len)
                .map(|_| {
                    self.state ^= self.state << 13;
                    self.state ^= self.state >> 7;
                    self.state ^= self.state << 17;
                    self.state.to_le_bytes()[0]
                })
                .collect()
        }

        fn signature(&mut self) -> Signature {
            Signature {
                public_nonce: self.take(32),
                signature: self.take(32),
            }
        }

        fn com_and_pub_signature(&mut self) -> Box<ComAndPubSignature> {
            Box::new(ComAndPubSignature {
                ephemeral_commitment: self.take(32),
                ephemeral_pubkey: self.take(32),
                u_a: self.take(32),
                u_x: self.take(32),
                u_y: self.take(32),
            })
        }

        /// Output features with every field set, including the largest sidechain feature (a template registration):
        /// 9 message instances
        fn features(&mut self) -> Box<OutputFeatures> {
            let template = TemplateRegistration {
                author_public_key: self.take(32),
                author_signature: Some(self.signature()),
                template_name: "a_template_name".to_string(),
                template_version: 1,
                template_type: Some(TemplateType {
                    template_type: Some(template_type::TemplateType::Wasm(WasmInfo { abi_version: 1 })),
                }),
                build_info: Some(BuildInfo {
                    repo_url: "https://github.com/tari-project/a-template".to_string(),
                    commit_hash: self.take(20),
                }),
                binary_sha: self.take(32),
                binary_url: "https://example.com/a-template.wasm".to_string(),
            };
            Box::new(OutputFeatures {
                version: 1,
                output_type: 1,
                maturity: 1_000,
                coinbase_extra: self.take(64),
                sidechain_feature: Some(Box::new(SideChainFeature {
                    side_chain_feature: Some(side_chain_feature::SideChainFeature::TemplateRegistration(template)),
                    sidechain_id: Some(SidechainId {
                        public_key: self.take(32),
                        knowledge_proof: Some(self.signature()),
                    }),
                })),
                range_proof_type: 1,
            })
        }

        /// An input with every field set
        fn input(&mut self) -> TransactionInput {
            TransactionInput {
                features: Some(self.features()),
                commitment: Some(Commitment { data: self.take(32) }),
                script: self.take(70),
                input_data: self.take(70),
                script_signature: Some(self.com_and_pub_signature()),
                sender_offset_public_key: self.take(32),
                output_hash: self.take(32),
                covenant: self.take(4),
                version: 1,
                encrypted_data: self.take(256),
                minimum_value_promise: 1,
                metadata_signature: Some(self.com_and_pub_signature()),
                rangeproof_hash: self.take(32),
            }
        }

        /// An output with every field set
        fn output(&mut self) -> TransactionOutput {
            TransactionOutput {
                features: Some(self.features()),
                commitment: Some(Commitment { data: self.take(32) }),
                range_proof: Some(RangeProof {
                    proof_bytes: self.take(672),
                }),
                script: self.take(70),
                sender_offset_public_key: self.take(32),
                metadata_signature: Some(self.com_and_pub_signature()),
                covenant: self.take(4),
                version: 1,
                encrypted_data: self.take(256),
                minimum_value_promise: 1,
            }
        }

        /// A kernel with every field set
        fn kernel(&mut self) -> TransactionKernel {
            TransactionKernel {
                features: 1,
                fee: 1_000,
                lock_height: 1,
                excess: Some(Commitment { data: self.take(32) }),
                excess_sig: Some(self.signature()),
                version: 1,
                burn_commitment: Some(Commitment { data: self.take(32) }),
            }
        }

        fn body(&mut self, inputs: usize, outputs: usize, kernels: usize) -> proto::types::AggregateBody {
            proto::types::AggregateBody {
                inputs: (0..inputs).map(|_| self.input()).collect(),
                outputs: (0..outputs).map(|_| self.output()).collect(),
                kernels: (0..kernels).map(|_| self.kernel()).collect(),
            }
        }
    }

    fn block_body_response(body: proto::types::AggregateBody) -> Vec<u8> {
        prost::Message::encode_to_vec(&proto::base_node::BlockBodyResponse {
            hash: vec![0x11; 32],
            body: Some(body),
        })
    }

    fn transaction(body: proto::types::AggregateBody) -> proto::types::Transaction {
        proto::types::Transaction {
            offset: Some(PrivateKey { data: vec![0x22; 32] }),
            body: Some(body),
            script_offset: Some(PrivateKey { data: vec![0x33; 32] }),
        }
    }

    fn count<T: DecodeBudget>(payload: &[u8]) -> usize {
        check_decode_budget::<T>(payload, BODY_MAX_ITEMS).unwrap()
    }

    #[test]
    fn a_max_size_block_body_is_accepted() {
        let inputs = block_body_response(Bytes::random().body(MAX_INPUTS, 0, 0));
        // The body plus every element and its sub-messages
        assert_eq!(
            count::<proto::base_node::BlockBodyResponse>(&inputs),
            1 + MAX_INPUTS * INPUT_MESSAGES
        );

        let outputs = block_body_response(Bytes::random().body(0, MAX_OUTPUTS, 0));
        assert_eq!(
            count::<proto::base_node::BlockBodyResponse>(&outputs),
            1 + MAX_OUTPUTS * OUTPUT_MESSAGES
        );

        let kernels = block_body_response(Bytes::random().body(0, 0, MAX_KERNELS));
        assert_eq!(
            count::<proto::base_node::BlockBodyResponse>(&kernels),
            1 + MAX_KERNELS * KERNEL_MESSAGES
        );
    }

    #[test]
    fn a_max_size_transaction_is_accepted() {
        let tx = prost::Message::encode_to_vec(&transaction(Bytes::random().body(MAX_INPUTS, 1, 1)));
        // Offset, script offset and body, plus every element and its sub-messages
        assert_eq!(
            count::<proto::types::Transaction>(&tx),
            3 + MAX_INPUTS * INPUT_MESSAGES + OUTPUT_MESSAGES + KERNEL_MESSAGES
        );
    }

    /// Ciphertexts, script data and coinbase extra are attacker-chosen bytes in consensus-valid blocks. Filled with
    /// bytes that look like embedded messages they must not change the count, or a valid mined block could be made
    /// impossible to sync.
    #[test]
    fn bytes_fields_that_look_like_messages_are_not_counted() {
        let mut bytes = Bytes::fake_messages();
        let body = bytes.body(MAX_INPUTS, MAX_OUTPUTS, MAX_KERNELS);
        let payload = block_body_response(body.clone());
        assert_eq!(
            count::<proto::base_node::BlockBodyResponse>(&payload),
            1 + MAX_INPUTS * INPUT_MESSAGES + MAX_OUTPUTS * OUTPUT_MESSAGES + MAX_KERNELS * KERNEL_MESSAGES
        );

        let tx = prost::Message::encode_to_vec(&transaction(body));
        check_decode_budget::<proto::types::Transaction>(&tx, BODY_MAX_ITEMS).unwrap();
    }

    #[test]
    fn a_transaction_of_empty_inputs_is_rejected() {
        // Just under the 6 MiB request cap: two bytes per input on the wire, a whole struct each once decoded
        let tx = transaction(proto::types::AggregateBody {
            inputs: vec![TransactionInput::default(); 3_000_000],
            outputs: vec![],
            kernels: vec![],
        });
        let payload = prost::Message::encode_to_vec(&tx);
        assert!(payload.len() < 6 * 1024 * 1024);
        let err = check_decode_budget::<proto::types::Transaction>(&payload, BODY_MAX_ITEMS).unwrap_err();
        assert_eq!(err.max, BODY_MAX_ITEMS);
    }

    /// The flood sits three messages deep (mempool state -> transaction -> body -> inputs), spread over transactions
    /// that are each well within the budget.
    #[test]
    fn a_flood_deep_in_the_mempool_state_is_rejected() {
        let tx = transaction(proto::types::AggregateBody {
            inputs: vec![TransactionInput::default(); 20_000],
            outputs: vec![],
            kernels: vec![],
        });
        check_decode_budget::<proto::types::Transaction>(&prost::Message::encode_to_vec(&tx), BODY_MAX_ITEMS).unwrap();

        let state = proto::mempool::StateResponse {
            unconfirmed_pool: vec![tx; 20],
            reorg_pool: vec![],
        };
        let payload = prost::Message::encode_to_vec(&state);
        check_decode_budget::<proto::mempool::StateResponse>(&payload, BODY_MAX_ITEMS).unwrap_err();
    }
}
