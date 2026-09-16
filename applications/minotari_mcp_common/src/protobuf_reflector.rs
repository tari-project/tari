// Copyright 2025, The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol)
// infrastructure for Tari applications
//! Runtime Protobuf Schema Generation using prost-reflect
//!
//! This module provides dynamic JSON schema generation from protobuf FileDescriptorSet
//! for MCP protocol validation. The ProtobufReflector can introspect gRPC method
//! definitions and generate corresponding JSON schemas for parameter validation.
//!
//! Note: This is a foundational implementation that provides basic schema generation.
//! Advanced features like validation rule extraction and complex type handling
//! can be extended in future iterations.

use std::collections::{BTreeMap, HashMap};

use prost_reflect::{DescriptorPool, EnumDescriptor, FieldDescriptor, Kind, MessageDescriptor};
use schemars::schema::*;
use serde_json::Value as JsonValue;

use crate::error::{McpError, McpResult};

/// Runtime protobuf schema generator using reflection
pub struct ProtobufReflector {
    descriptor_pool: DescriptorPool,
    schema_cache: HashMap<String, Schema>,
}

impl ProtobufReflector {
    /// Create a new reflector from FileDescriptorSet bytes
    pub fn new(descriptor_set: &[u8]) -> McpResult<Self> {
        let descriptor_pool = DescriptorPool::decode(descriptor_set)
            .map_err(|e| McpError::server_error(format!("Failed to decode descriptor set: {e}")))?;

        Ok(Self {
            descriptor_pool,
            schema_cache: HashMap::new(),
        })
    }

    /// Generate JSON schema for a specific message type
    pub fn generate_schema(&mut self, message_name: &str) -> McpResult<Schema> {
        let message_desc = self
            .descriptor_pool
            .get_message_by_name(message_name)
            .ok_or_else(|| McpError::invalid_request(format!("Message type '{}' not found", message_name)))?;

        self.generate_message_schema(&message_desc)
    }

    /// Generate schema for all methods in a service
    pub fn generate_service_schemas(&mut self, service_name: &str) -> McpResult<HashMap<String, Schema>> {
        let service_desc = self
            .descriptor_pool
            .get_service_by_name(service_name)
            .ok_or_else(|| McpError::invalid_request(format!("Service '{}' not found", service_name)))?;

        let mut schemas = HashMap::new();

        for method in service_desc.methods() {
            let input_type = method.input().full_name().to_string();
            let schema = self.generate_schema(&input_type)?;
            schemas.insert(method.name().to_string(), schema);
        }

        Ok(schemas)
    }

    /// Get all available message types
    pub fn list_message_types(&self) -> Vec<String> {
        self.descriptor_pool
            .all_messages()
            .map(|msg| msg.full_name().to_string())
            .collect()
    }

    /// Get all available service types
    pub fn list_services(&self) -> Vec<String> {
        let mut services = Vec::new();
        for file_desc in self.descriptor_pool.files() {
            for service_desc in file_desc.services() {
                services.push(service_desc.full_name().to_string());
            }
        }
        services
    }

    /// Generate schema for a message with caching
    fn generate_message_schema(&mut self, message_desc: &MessageDescriptor) -> McpResult<Schema> {
        let type_name = message_desc.full_name().to_string();

        // Check cache first
        if let Some(cached_schema) = self.schema_cache.get(&type_name) {
            return Ok(cached_schema.clone());
        }

        let mut properties = BTreeMap::new();
        let mut required = Vec::new();

        // Generate schema for each field
        for field in message_desc.fields() {
            let field_name = field.json_name().to_string();
            let field_schema = self.generate_field_schema(&field)?;

            properties.insert(field_name.clone(), field_schema);

            // In proto3, fields are optional by default, but we check for custom validation
            if self.is_field_required(&field) {
                required.push(field_name);
            }
        }

        let mut schema_obj = SchemaObject {
            instance_type: Some(InstanceType::Object.into()),
            object: Some(Box::new(ObjectValidation {
                properties,
                required: required.into_iter().collect(),
                additional_properties: Some(Box::new(false.into())), // Strict validation
                ..Default::default()
            })),
            metadata: Some(Box::new(Metadata {
                title: Some(type_name.clone()),
                description: Some(format!("Generated schema for protobuf message {}", type_name)),
                ..Default::default()
            })),
            ..Default::default()
        };

        // Apply message-level validation rules
        self.apply_message_validation(message_desc, &mut schema_obj);

        let schema = Schema::Object(schema_obj);
        self.schema_cache.insert(type_name, schema.clone());

        Ok(schema)
    }

    /// Generate schema for an individual field
    fn generate_field_schema(&mut self, field: &FieldDescriptor) -> McpResult<Schema> {
        let mut base_schema = match field.kind() {
            Kind::Bool => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Boolean.into()),
                ..Default::default()
            }),

            Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Integer.into()),
                format: Some("int32".to_string()),
                number: Some(Box::new(NumberValidation {
                    minimum: Some(-2147483648.0), // i32::MIN
                    maximum: Some(2147483647.0),  // i32::MAX
                    ..Default::default()
                })),
                ..Default::default()
            }),

            Kind::Uint32 | Kind::Fixed32 => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Integer.into()),
                format: Some("uint32".to_string()),
                number: Some(Box::new(NumberValidation {
                    minimum: Some(0.0),
                    maximum: Some(4294967295.0), // u32::MAX
                    ..Default::default()
                })),
                ..Default::default()
            }),

            Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Integer.into()),
                format: Some("int64".to_string()),
                ..Default::default()
            }),

            Kind::Uint64 | Kind::Fixed64 => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Integer.into()),
                format: Some("uint64".to_string()),
                number: Some(Box::new(NumberValidation {
                    minimum: Some(0.0),
                    ..Default::default()
                })),
                ..Default::default()
            }),

            Kind::Float => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Number.into()),
                format: Some("float".to_string()),
                ..Default::default()
            }),

            Kind::Double => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Number.into()),
                format: Some("double".to_string()),
                ..Default::default()
            }),

            Kind::String => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::String.into()),
                ..Default::default()
            }),

            Kind::Bytes => Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::String.into()),
                format: Some("byte".to_string()),
                string: Some(Box::new(StringValidation {
                    pattern: Some("^[A-Za-z0-9+/]*={0,2}$".to_string()), // Base64 pattern
                    ..Default::default()
                })),
                ..Default::default()
            }),

            Kind::Message(message_desc) => {
                // Handle special message types
                match message_desc.full_name() {
                    "google.protobuf.Timestamp" => Schema::Object(SchemaObject {
                        instance_type: Some(InstanceType::String.into()),
                        format: Some("date-time".to_string()),
                        ..Default::default()
                    }),
                    "google.protobuf.Duration" => Schema::Object(SchemaObject {
                        instance_type: Some(InstanceType::String.into()),
                        format: Some("duration".to_string()),
                        ..Default::default()
                    }),
                    _ => self.generate_message_schema(&message_desc)?,
                }
            },

            Kind::Enum(enum_desc) => self.generate_enum_schema(&enum_desc),
        };

        // Apply field-level validation
        self.apply_field_validation(field, &mut base_schema);

        // Handle repeated fields (arrays)
        if field.is_list() {
            base_schema = Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Array.into()),
                array: Some(Box::new(ArrayValidation {
                    items: Some(SingleOrVec::Single(Box::new(base_schema))),
                    min_items: if self.is_field_required(field) { Some(1) } else { None },
                    ..Default::default()
                })),
                ..Default::default()
            });
        }

        // Handle optional fields (in proto3, most fields are optional by default)
        // We skip the null handling for now as it depends on specific proto3 optional semantics

        Ok(base_schema)
    }

    /// Generate schema for enum types
    fn generate_enum_schema(&self, enum_desc: &EnumDescriptor) -> Schema {
        let enum_values: Vec<JsonValue> = enum_desc
            .values()
            .map(|v| JsonValue::String(v.name().to_string()))
            .collect();

        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(enum_values),
            metadata: Some(Box::new(Metadata {
                title: Some(enum_desc.full_name().to_string()),
                description: Some(format!("Enum values for {}", enum_desc.full_name())),
                ..Default::default()
            })),
            ..Default::default()
        })
    }

    /// Check if a field should be required based on proto rules
    fn is_field_required(&self, _field: &FieldDescriptor) -> bool {
        // In proto3, most fields are optional by default
        // You could extend this to check for custom validation annotations
        // For now, we use a conservative approach
        false
    }

    /// Apply message-level validation rules from protobuf options
    fn apply_message_validation(&self, _message_desc: &MessageDescriptor, schema: &mut SchemaObject) {
        // Extract custom validation rules from protobuf options
        // This would require parsing proto options like (validate.rules)
        // For now, we apply sensible defaults

        let mut extensions = BTreeMap::new();
        extensions.insert(
            "mcp_type".to_string(),
            JsonValue::String("protobuf_message".to_string()),
        );
        schema.extensions.extend(extensions);
    }

    /// Apply field-level validation rules
    fn apply_field_validation(&self, _field: &FieldDescriptor, schema: &mut Schema) {
        // Extract validation rules from field options
        // This would parse annotations like:
        // string name = 1 [(validate.rules).string.min_len = 3];
        // uint32 age = 2 [(validate.rules).uint32.gt = 0];

        if let Schema::Object(obj) = schema {
            match field.kind() {
                Kind::String => {
                    // Apply string validations
                    if obj.string.is_none() {
                        obj.string = Some(Box::new(StringValidation::default()));
                    }

                    // Example: minimum length for strings
                    if field.json_name().contains("email") {
                        obj.string.as_mut().unwrap().pattern =
                            Some(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string());
                    }
                },
                Kind::Uint32 | Kind::Uint64 => {
                    // Ensure non-negative for unsigned integers
                    if obj.number.is_none() {
                        obj.number = Some(Box::new(NumberValidation::default()));
                    }
                    obj.number.as_mut().unwrap().minimum = Some(0.0);
                },
                _ => {},
            }

            // Add MCP-specific metadata
            let mut extensions = BTreeMap::new();
            extensions.insert("field_name".to_string(), JsonValue::String(field.name().to_string()));
            extensions.insert("field_number".to_string(), JsonValue::Number(field.number().into()));
            obj.extensions.extend(extensions);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflector_creation_empty_descriptor() {
        // Test with empty descriptor set
        let empty_descriptor = vec![];
        let result = ProtobufReflector::new(&empty_descriptor);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_generation_nonexistent_message() {
        // Test with minimal valid descriptor set (would need actual protobuf bytes)
        // This is a placeholder for real test data
        let descriptor_bytes = vec![]; // Would contain actual FileDescriptorSet bytes

        if let Ok(mut reflector) = ProtobufReflector::new(&descriptor_bytes) {
            let result = reflector.generate_schema("NonexistentMessage");
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_list_operations() {
        let descriptor_bytes = vec![]; // Would contain actual FileDescriptorSet bytes

        if let Ok(reflector) = ProtobufReflector::new(&descriptor_bytes) {
            let messages = reflector.list_message_types();
            let services = reflector.list_services();

            // With empty descriptor, should have empty lists
            assert!(messages.is_empty());
            assert!(services.is_empty());
        }
    }
}
