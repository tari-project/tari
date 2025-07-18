// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use utoipa::{
    openapi::{schema::SchemaType, Object, OneOf, Schema, Type},
    ToSchema,
};

use crate::transactions::transaction_components::TransactionOutput;

#[derive(Serialize, Deserialize, Validate)]
pub struct GetUtxosByBlockRequest {
    pub header_hash: Vec<u8>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct GetUtxosByBlockResponse {
    pub header_hash: Vec<u8>,
    pub height: u64,
    #[schema(schema_with = transaction_output_schema)]
    pub outputs: Vec<TransactionOutput>,
    pub mined_timestamp: u64,
}

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
