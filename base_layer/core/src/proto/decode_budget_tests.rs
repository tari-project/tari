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
    use tari_comms::{
        decode_budget::{DecodeBudget, check_decode_budget},
        protocol::rpc::RPC_MAX_REQUEST_SIZE,
    };

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
    const BODY_MAX_ITEMS: usize = proto::BODY_MAX_DECODE_ITEMS;

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
    pub(super) struct Bytes {
        state: u64,
        fake_messages: bool,
    }

    impl Bytes {
        pub(super) fn random() -> Self {
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

        pub(super) fn take(&mut self, len: usize) -> Vec<u8> {
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

        pub(super) fn signature(&mut self) -> Signature {
            Signature {
                public_nonce: self.take(32),
                signature: self.take(32),
            }
        }

        pub(super) fn com_and_pub_signature(&mut self) -> Box<ComAndPubSignature> {
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
        pub(super) fn input(&mut self) -> TransactionInput {
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
        pub(super) fn output(&mut self) -> TransactionOutput {
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
        pub(super) fn kernel(&mut self) -> TransactionKernel {
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

    pub(super) fn transaction(body: proto::types::AggregateBody) -> proto::types::Transaction {
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

    /// The wire bytes of field `tag` holding `contents`. Flood tests build their payloads from these rather than from
    /// decoded structs, which would cost hundreds of MB just to encode.
    fn len_field(tag: u32, contents: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(contents.len() + 16);
        prost::encoding::encode_key(tag, prost::encoding::WireType::LengthDelimited, &mut buf);
        prost::encoding::encode_varint(contents.len() as u64, &mut buf);
        buf.extend_from_slice(contents);
        buf
    }

    /// `count` empty elements of the repeated field `tag`: two bytes each on the wire
    fn empty_elements(tag: u32, count: usize) -> Vec<u8> {
        len_field(tag, &[]).repeat(count)
    }

    /// Elements that fill a request up to the 6 MiB request cap, leaving room for the enclosing headers
    const FLOOD_ELEMENTS: usize = RPC_MAX_REQUEST_SIZE / 2 - 8;

    #[test]
    fn a_transaction_of_empty_inputs_is_rejected() {
        // A full 6 MiB request: two bytes per input on the wire, a whole struct each once decoded. Transaction.body
        // is tag 2 and AggregateBody.inputs tag 1.
        let payload = len_field(2, &empty_elements(1, FLOOD_ELEMENTS));
        assert!(payload.len() <= RPC_MAX_REQUEST_SIZE);
        let err = check_decode_budget::<proto::types::Transaction>(&payload, BODY_MAX_ITEMS).unwrap_err();
        assert_eq!(err.max, BODY_MAX_ITEMS);
    }

    /// A max-size transaction, and a max-size `NewBlock` (1,000 fully populated coinbase outputs and a kernel excess
    /// signature for every kernel that fits), pass the messaging decode budget.
    #[test]
    fn max_size_messages_pass_the_messaging_budget() {
        use tari_p2p::tari_message::TariMessageType;

        use crate::test_helpers::create_peer_message;

        let tx = prost::Message::encode_to_vec(&transaction(Bytes::random().body(MAX_INPUTS, 1, 1)));
        let msg = create_peer_message(TariMessageType::NewTransaction, tx);
        msg.decode_message_with_max_items::<proto::types::Transaction>(proto::MESSAGE_MAX_DECODE_ITEMS)
            .unwrap();

        let mut bytes = Bytes::random();
        let block = proto::core::NewBlock {
            header: Some(proto::core::BlockHeader::default()),
            coinbase_kernels: vec![bytes.kernel()],
            coinbase_outputs: (0..1_000).map(|_| bytes.output()).collect(),
            kernel_excess_sigs: (0..MAX_KERNELS).map(|_| bytes.take(32)).collect(),
        };
        let msg = create_peer_message(TariMessageType::NewBlock, prost::Message::encode_to_vec(&block));
        let decoded = msg
            .decode_message_with_max_items::<proto::core::NewBlock>(proto::MESSAGE_MAX_DECODE_ITEMS)
            .unwrap();
        assert_eq!(decoded.kernel_excess_sigs.len(), MAX_KERNELS);
    }

    /// A max-size transaction wrapped as a mempool-sync `TransactionItem` passes the budget the sync protocol applies
    /// to every frame it reads.
    #[test]
    fn a_max_size_mempool_sync_transaction_item_passes_the_messaging_budget() {
        let item = crate::mempool::proto::TransactionItem {
            transaction: Some(transaction(Bytes::random().body(MAX_INPUTS, 1, 1))),
        };
        let frame = prost::Message::encode_to_vec(&item);
        let items =
            check_decode_budget::<crate::mempool::proto::TransactionItem>(&frame, proto::MESSAGE_MAX_DECODE_ITEMS)
                .unwrap();
        assert!(items > MAX_INPUTS);
    }

    /// The budget of the hash-batch query methods (fetch_matching_utxos, utxo_query, query_deleted, find_chain_split)
    const QUERY_MAX_ITEMS: usize = 65_536;

    #[test]
    fn a_flood_of_empty_hashes_is_rejected() {
        use tari_transaction_components::rpc::MAX_ALLOWED_QUERY_SIZE;

        // The largest legitimate request: one item per hash
        let request = proto::base_node::FetchMatchingUtxos {
            output_hashes: vec![vec![0xaa; 32]; MAX_ALLOWED_QUERY_SIZE],
        };
        let payload = prost::Message::encode_to_vec(&request);
        assert_eq!(
            check_decode_budget::<proto::base_node::FetchMatchingUtxos>(&payload, QUERY_MAX_ITEMS).unwrap(),
            MAX_ALLOWED_QUERY_SIZE
        );

        // A full 6 MiB request: two bytes per hash on the wire, a whole `Vec` each once decoded. output_hashes is tag
        // 1.
        let payload = empty_elements(1, FLOOD_ELEMENTS);
        assert!(payload.len() <= RPC_MAX_REQUEST_SIZE);
        check_decode_budget::<proto::base_node::FetchMatchingUtxos>(&payload, QUERY_MAX_ITEMS).unwrap_err();
    }

    /// The flood sits three messages deep (mempool state -> transaction -> body -> inputs), spread over transactions
    /// that are each well within the budget.
    #[test]
    fn a_flood_deep_in_the_mempool_state_is_rejected() {
        // Transaction.body (tag 2) holding 20,000 empty AggregateBody.inputs (tag 1)
        let tx = len_field(2, &empty_elements(1, 20_000));
        assert_eq!(
            check_decode_budget::<proto::types::Transaction>(&tx, BODY_MAX_ITEMS).unwrap(),
            20_001
        );

        // StateResponse.unconfirmed_pool is tag 1
        let payload = len_field(1, &tx).repeat(20);
        check_decode_budget::<proto::mempool::StateResponse>(&payload, BODY_MAX_ITEMS).unwrap_err();
    }
}

/// The block-body decode budget must hold on every network, for every consensus epoch.
mod per_network {
    use std::collections::BTreeSet;

    use tari_common::configuration::Network;
    use tari_comms::decode_budget::check_decode_budget;
    use tari_p2p::tari_message::TariMessageType;
    use tari_transaction_components::consensus::ConsensusConstants;

    use super::max_size_payloads::{Bytes, transaction};
    use crate::{proto, test_helpers::create_peer_message};

    const NETWORKS: [Network; 6] = [
        Network::MainNet,
        Network::StageNet,
        Network::NextNet,
        Network::LocalNet,
        Network::Igor,
        Network::Esmeralda,
    ];

    /// The message instances the walker counts for one element, encoded in an otherwise empty `AggregateBody`
    fn instances(body: proto::types::AggregateBody) -> u64 {
        let count =
            check_decode_budget::<proto::types::AggregateBody>(&prost::Message::encode_to_vec(&body), usize::MAX)
                .unwrap();
        count as u64
    }

    fn body() -> proto::types::AggregateBody {
        proto::types::AggregateBody::default()
    }

    /// The most of each element a max-weight block can hold, for one set of consensus constants
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct Limits {
        /// Inputs, after the one kernel and one output a transaction needs
        inputs: u64,
        /// Kernels, if the block held nothing else (an upper bound on its kernel excess signatures)
        kernels: u64,
        coinbases: u64,
    }

    fn limits(constants: &ConsensusConstants) -> Limits {
        let weights = constants.transaction_weight_params().params();
        let max_weight = constants.max_block_transaction_weight();
        Limits {
            inputs: max_weight
                .saturating_sub(weights.kernel_weight)
                .saturating_sub(weights.output_weight) /
                weights.input_weight,
            kernels: max_weight / weights.kernel_weight,
            coinbases: constants.max_block_coinbase_count(),
        }
    }

    fn all_limits() -> Vec<(Network, ConsensusConstants)> {
        NETWORKS
            .into_iter()
            .flat_map(|network| {
                ConsensusConstants::for_network(network)
                    .into_iter()
                    .map(move |constants| (network, constants))
            })
            .collect()
    }

    #[test]
    fn the_messaging_budget_is_the_body_budget() {
        assert_eq!(proto::MESSAGE_MAX_DECODE_ITEMS, proto::BODY_MAX_DECODE_ITEMS);
    }

    #[test]
    fn a_max_weight_block_fits_the_body_budget_on_every_network() {
        // Measured from fully populated elements, so a proto shape change moves them
        let mut bytes = Bytes::random();
        let hydrated_input = instances(proto::types::AggregateBody {
            inputs: vec![bytes.input()],
            ..body()
        });
        // A compact input, as sync_blocks serves them: the output hash, input data and script signature
        let compact_input = instances(proto::types::AggregateBody {
            inputs: vec![proto::types::TransactionInput {
                output_hash: bytes.take(32),
                input_data: bytes.take(70),
                script_signature: Some(bytes.com_and_pub_signature()),
                version: 1,
                ..Default::default()
            }],
            ..body()
        });
        let output = instances(proto::types::AggregateBody {
            outputs: vec![bytes.output()],
            ..body()
        });
        let kernel = instances(proto::types::AggregateBody {
            kernels: vec![bytes.kernel()],
            ..body()
        });
        println!(
            "instances: hydrated input {hydrated_input}, compact input {compact_input}, output {output}, kernel \
             {kernel}"
        );

        let budget = proto::BODY_MAX_DECODE_ITEMS as u64;
        for (network, constants) in all_limits() {
            let Limits { inputs, coinbases, .. } = limits(&constants);
            // The body, the coinbase outputs, the transaction's kernel and the coinbase kernel
            let fixed = 1 + coinbases * output + 2 * kernel;
            let hydrated = fixed + inputs * hydrated_input;
            let compact = fixed + inputs * compact_input;
            println!(
                "{network} (from height {}): {inputs} inputs; hydrated {hydrated} instances ({:.2}x headroom), \
                 compact {compact} ({:.2}x)",
                constants.effective_from_height(),
                budget as f64 / hydrated as f64,
                budget as f64 / compact as f64,
            );
            assert!(
                hydrated < budget,
                "{network}: a max-weight block of hydrated inputs is {hydrated} message instances, over the {budget} \
                 decode budget"
            );
        }
    }

    /// The largest transaction and `NewBlock` each network allows pass the messaging decode budget
    #[test]
    fn max_size_messages_pass_the_messaging_budget_on_every_network() {
        // Distinct limits only: building a max-size transaction is not free
        let distinct = all_limits()
            .iter()
            .map(|(_, constants)| limits(constants))
            .collect::<BTreeSet<_>>();
        for limits in distinct {
            let mut bytes = Bytes::random();
            let tx = transaction(proto::types::AggregateBody {
                inputs: (0..limits.inputs).map(|_| bytes.input()).collect(),
                outputs: vec![bytes.output()],
                kernels: vec![bytes.kernel()],
            });
            let msg = create_peer_message(TariMessageType::NewTransaction, prost::Message::encode_to_vec(&tx));
            msg.decode_message_with_max_items::<proto::types::Transaction>(proto::MESSAGE_MAX_DECODE_ITEMS)
                .unwrap_or_else(|err| panic!("{limits:?}: {err}"));

            let block = proto::core::NewBlock {
                header: Some(proto::core::BlockHeader::default()),
                coinbase_kernels: vec![bytes.kernel()],
                coinbase_outputs: (0..limits.coinbases).map(|_| bytes.output()).collect(),
                kernel_excess_sigs: (0..limits.kernels).map(|_| bytes.take(32)).collect(),
            };
            let msg = create_peer_message(TariMessageType::NewBlock, prost::Message::encode_to_vec(&block));
            msg.decode_message_with_max_items::<proto::core::NewBlock>(proto::MESSAGE_MAX_DECODE_ITEMS)
                .unwrap_or_else(|err| panic!("{limits:?}: {err}"));
        }
    }
}
