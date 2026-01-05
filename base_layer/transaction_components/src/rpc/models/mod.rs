// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use utoipa::openapi::{schema::SchemaType, Object, OneOf, Schema, Type};

mod fee_per_gram;
mod generate_kernel_merkle_proof;
mod get_header_by_height;
mod get_tip_info;
mod get_utxo;
mod get_utxos_by_block;
mod get_utxos_deleted_info;
mod get_utxos_mined_info;
mod sync_utxos_by_block;
mod transaction_query;
mod tx_submission_response;

pub use fee_per_gram::*;
pub use generate_kernel_merkle_proof::*;
pub use get_header_by_height::*;
pub use get_tip_info::*;
pub use get_utxo::*;
pub use get_utxos_by_block::*;
pub use get_utxos_deleted_info::*;
pub use get_utxos_mined_info::*;
pub use sync_utxos_by_block::*;
pub use transaction_query::*;
pub use tx_submission_response::*;

#[allow(clippy::too_many_lines)]
pub fn transaction_output_schema() -> Schema {
    Schema::Object(
        Object::builder()
            .property(
                "version",
                Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
            )
            .property(
                "features",
                Schema::Object(
                    Object::builder()
                        .property(
                            "version",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
                        )
                        .property(
                            "output_type",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
                        )
                        .property(
                            "maturity",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::Integer))),
                        )
                        .property(
                            "coinbase_extra",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
                        )
                        .property(
                            "sidechain_feature",
                            Schema::OneOf(
                                OneOf::builder()
                                    .item(Schema::Object(Object::with_type(SchemaType::Type(Type::String))))
                                    .item(Schema::Object(Object::with_type(SchemaType::Type(Type::Null))))
                                    .build(),
                            ),
                        )
                        .property(
                            "range_proof_type",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
                        )
                        .build(),
                ),
            )
            .property(
                "commitment",
                Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
            )
            .property(
                "proof",
                Schema::OneOf(
                    OneOf::builder()
                        .item(Schema::Object(Object::with_type(SchemaType::Type(Type::String))))
                        .item(Schema::Object(Object::with_type(SchemaType::Type(Type::Null))))
                        .build(),
                ),
            )
            .property(
                "script",
                Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
            )
            .property(
                "sender_offset_public_key",
                Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
            )
            .property(
                "metadata_signature",
                Schema::Object(
                    Object::builder()
                        .property(
                            "ephemeral_commitment",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
                        )
                        .property(
                            "ephemeral_pubkey",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
                        )
                        .property("u_a", Schema::Object(Object::with_type(SchemaType::Type(Type::String))))
                        .property("u_x", Schema::Object(Object::with_type(SchemaType::Type(Type::String))))
                        .property("u_y", Schema::Object(Object::with_type(SchemaType::Type(Type::String))))
                        .build(),
                ),
            )
            .property(
                "covenant",
                Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
            )
            .property(
                "encrypted_data",
                Schema::Object(
                    Object::builder()
                        .property(
                            "data",
                            Schema::Object(Object::with_type(SchemaType::Type(Type::String))),
                        )
                        .build(),
                ),
            )
            .property(
                "minimum_value_promise",
                Schema::Object(Object::with_type(SchemaType::Type(Type::Integer))),
            )
            .build(),
    )
}
