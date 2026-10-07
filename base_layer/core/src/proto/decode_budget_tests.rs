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
    use tari_comms::protocol::rpc::{RpcError, decode_guard::check_decode_budget};

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
    const BODY_MAX_ITEMS: usize = 524_288;

    // The most of each that fits in a mainnet block (weight 90,000) on its own
    const MAX_INPUTS: usize = 11_242;
    const MAX_OUTPUTS: usize = 1_666;
    const MAX_KERNELS: usize = 8_994;

    /// Deterministic pseudo-random bytes. Hashes, keys and signatures are random-looking, and the guard's best-effort
    /// recursion counts whatever happens to parse as protobuf inside them, so the test must not use tidy patterns.
    struct Bytes(u64);

    impl Bytes {
        fn take(&mut self, len: usize) -> Vec<u8> {
            (0..len)
                .map(|_| {
                    self.0 ^= self.0 << 13;
                    self.0 ^= self.0 >> 7;
                    self.0 ^= self.0 << 17;
                    self.0.to_le_bytes()[0]
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

        /// Output features with every field set, including the largest sidechain feature (a template registration)
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
                coinbase_extra: self.take(16),
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

        /// An input with every field set (43 LEN items)
        fn input(&mut self) -> TransactionInput {
            TransactionInput {
                features: Some(self.features()),
                commitment: Some(Commitment { data: self.take(32) }),
                script: self.take(35),
                input_data: self.take(35),
                script_signature: Some(self.com_and_pub_signature()),
                sender_offset_public_key: self.take(32),
                output_hash: self.take(32),
                covenant: self.take(4),
                version: 1,
                encrypted_data: self.take(80),
                minimum_value_promise: 1,
                metadata_signature: Some(self.com_and_pub_signature()),
                rangeproof_hash: self.take(32),
            }
        }

        /// An output with every field set (36 LEN items)
        fn output(&mut self) -> TransactionOutput {
            TransactionOutput {
                features: Some(self.features()),
                commitment: Some(Commitment { data: self.take(32) }),
                range_proof: Some(RangeProof {
                    proof_bytes: self.take(672),
                }),
                script: self.take(35),
                sender_offset_public_key: self.take(32),
                metadata_signature: Some(self.com_and_pub_signature()),
                covenant: self.take(4),
                version: 1,
                encrypted_data: self.take(80),
                minimum_value_promise: 1,
            }
        }

        /// A kernel with every field set (8 LEN items)
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
    }

    fn body(inputs: usize, outputs: usize, kernels: usize) -> proto::types::AggregateBody {
        let mut bytes = Bytes(0x9e37_79b9_7f4a_7c15);
        proto::types::AggregateBody {
            inputs: (0..inputs).map(|_| bytes.input()).collect(),
            outputs: (0..outputs).map(|_| bytes.output()).collect(),
            kernels: (0..kernels).map(|_| bytes.kernel()).collect(),
        }
    }

    /// The number of LEN items the guard counts in `payload`: the smallest budget it accepts.
    fn counted_items(payload: &[u8]) -> usize {
        let (mut low, mut high) = (0usize, BODY_MAX_ITEMS.saturating_mul(2));
        while low < high {
            let mid = low + (high - low) / 2;
            if check_decode_budget(payload, mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        low
    }

    /// Accepted under the block-body budget, with room to spare on the bytes-per-item ratio.
    fn assert_accepted(payload: &[u8], min_items: usize) {
        check_decode_budget(payload, BODY_MAX_ITEMS).unwrap();
        let items = counted_items(payload);
        assert!(items >= min_items, "counted {items}, expected at least {min_items}");
        assert!(items <= BODY_MAX_ITEMS, "counted {items}");
        // The guard allows one item per 8 bytes; legitimate payloads need at most half that
        assert!(payload.len() / items >= 16, "{} bytes for {items} items", payload.len());
    }

    fn block_body_response(body: proto::types::AggregateBody) -> Vec<u8> {
        prost::Message::encode_to_vec(&proto::base_node::BlockBodyResponse {
            hash: vec![0x11; 32],
            body: Some(body),
        })
    }

    fn transaction(body: proto::types::AggregateBody) -> Vec<u8> {
        prost::Message::encode_to_vec(&proto::types::Transaction {
            offset: Some(PrivateKey { data: vec![0x22; 32] }),
            body: Some(body),
            script_offset: Some(PrivateKey { data: vec![0x33; 32] }),
        })
    }

    #[test]
    fn a_max_size_block_body_of_inputs_is_accepted() {
        let payload = block_body_response(body(MAX_INPUTS, 0, 0));
        assert_accepted(&payload, MAX_INPUTS * 43);
    }

    #[test]
    fn a_max_size_block_body_of_outputs_or_kernels_is_accepted() {
        assert_accepted(&block_body_response(body(0, MAX_OUTPUTS, 0)), MAX_OUTPUTS * 36);
        assert_accepted(&block_body_response(body(0, 0, MAX_KERNELS)), MAX_KERNELS * 8);
    }

    #[test]
    fn a_max_size_transaction_is_accepted() {
        let payload = transaction(body(MAX_INPUTS, 1, 1));
        assert_accepted(&payload, MAX_INPUTS * 43);
    }

    #[test]
    fn a_transaction_of_empty_inputs_is_rejected() {
        // Just under the 6 MiB request cap: two bytes per input on the wire, a whole struct each once decoded
        let payload = transaction(proto::types::AggregateBody {
            inputs: vec![TransactionInput::default(); 3_000_000],
            outputs: vec![],
            kernels: vec![],
        });
        assert!(payload.len() < 6 * 1024 * 1024);
        let err = check_decode_budget(&payload, BODY_MAX_ITEMS).unwrap_err();
        assert!(matches!(err, RpcError::DecodeBudgetExceeded { .. }), "{err:?}");
    }
}
