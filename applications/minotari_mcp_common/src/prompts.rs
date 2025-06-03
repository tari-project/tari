//! MCP prompt definitions and registry

use crate::error::{McpError, McpResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// MCP prompt trait that all prompts must implement
pub trait McpPrompt: Send + Sync {
    /// Get the prompt name
    fn name(&self) -> &str;
    
    /// Get the prompt description
    fn description(&self) -> &str;
    
    /// Get the arguments schema for this prompt
    fn arguments_schema(&self) -> Option<Value>;
    
    /// Generate the prompt content with the given arguments
    fn generate(&self, args: Option<Value>) -> McpResult<PromptContent>;
}

/// Content of a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptContent {
    pub messages: Vec<PromptMessage>,
}

/// A message in a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: MessageRole,
    pub content: MessageContent,
}

/// Role of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Resource { 
        #[serde(rename = "type")]
        content_type: String,
        #[serde(rename = "resource")]
        resource_uri: String,
    },
}

/// Prompt information for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInfo {
    pub name: String,
    pub description: String,
    pub arguments: Option<Value>,
}

/// Registry for managing MCP prompts
#[derive(Default)]
pub struct PromptRegistry {
    prompts: HashMap<String, Box<dyn McpPrompt>>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self {
            prompts: HashMap::new(),
        }
    }

    /// Register a new prompt
    pub fn register(&mut self, prompt: Box<dyn McpPrompt>) {
        let name = prompt.name().to_string();
        self.prompts.insert(name, prompt);
    }

    /// Get a prompt by name
    pub fn get(&self, name: &str) -> Option<&dyn McpPrompt> {
        self.prompts.get(name).map(|p| p.as_ref())
    }

    /// List all available prompts
    pub fn list_prompts(&self) -> Vec<PromptInfo> {
        self.prompts
            .values()
            .map(|prompt| PromptInfo {
                name: prompt.name().to_string(),
                description: prompt.description().to_string(),
                arguments: prompt.arguments_schema(),
            })
            .collect()
    }

    /// Get a prompt by name and generate its content
    pub fn get_prompt(&self, name: &str, args: Option<Value>) -> McpResult<PromptContent> {
        let prompt = self.get(name)
            .ok_or_else(|| McpError::PromptNotFound(name.to_string()))?;

        prompt.generate(args)
    }
}

/// Helper function to create a simple text message
pub fn text_message(role: MessageRole, content: impl Into<String>) -> PromptMessage {
    PromptMessage {
        role,
        content: MessageContent::Text(content.into()),
    }
}

/// Helper function to create a resource message
pub fn resource_message(role: MessageRole, resource_uri: impl Into<String>) -> PromptMessage {
    PromptMessage {
        role,
        content: MessageContent::Resource {
            content_type: "resource".to_string(),
            resource_uri: resource_uri.into(),
        },
    }
}

/// Macro to create a simple prompt
#[macro_export]
macro_rules! simple_prompt {
    ($name:expr, $description:expr, $messages:expr) => {
        {
            use crate::prompts::{McpPrompt, PromptContent};
            
            struct SimplePrompt {
                name: String,
                description: String,
                messages: Vec<crate::prompts::PromptMessage>,
            }
            
            impl McpPrompt for SimplePrompt {
                fn name(&self) -> &str { &self.name }
                fn description(&self) -> &str { &self.description }
                fn arguments_schema(&self) -> Option<serde_json::Value> { None }
                
                fn generate(&self, _args: Option<serde_json::Value>) -> McpResult<PromptContent> {
                    Ok(PromptContent {
                        messages: self.messages.clone(),
                    })
                }
            }
            
            Box::new(SimplePrompt {
                name: $name.to_string(),
                description: $description.to_string(),
                messages: $messages,
            })
        }
    };
}
