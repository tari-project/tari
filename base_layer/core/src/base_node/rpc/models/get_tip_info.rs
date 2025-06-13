// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};
use utoipa::{
    openapi::{schema::SchemaType, Object, OneOf, Schema, Type},
    ToSchema,
};

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct TipInfoResponse {
    #[schema(schema_with = optional_chain_metadata_schema)]
    pub metadata: Option<tari_common_types::chain_metadata::ChainMetadata>,
    pub is_synced: bool,
}

pub fn optional_chain_metadata_schema() -> Schema {
    Schema::OneOf(
        OneOf::builder()
            .item(Schema::Object(
                Object::builder()
                    .property(
                        "best_block_height",
                        Schema::Object(Object::with_type(SchemaType::Type(Type::Integer))),
                    )
                    .property("best_block_hash", Schema::Object(Object::with_type(Type::Array)))
                    .property(
                        "pruning_horizon",
                        Schema::Object(Object::with_type(SchemaType::Type(Type::Integer))),
                    )
                    .property(
                        "pruned_height",
                        Schema::Object(Object::with_type(SchemaType::Type(Type::Integer))),
                    )
                    .property(
                        "accumulated_difficulty",
                        Schema::Object(Object::with_type(SchemaType::Type(Type::Integer))),
                    )
                    .property(
                        "timestamp",
                        Schema::Object(Object::with_type(SchemaType::Type(Type::Integer))),
                    )
                    .build(),
            ))
            .item(Schema::Object(Object::with_type(Type::Null)))
            .build(),
    )
}
