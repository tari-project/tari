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
//! Node-specific MCP prompts

use minotari_mcp_common::{
    McpResult,
    PromptRegistry,
    prompts::MessageRole,
    resource_message,
    simple_prompt,
    text_message,
};

/// Registry for node-specific MCP prompts
pub struct NodePromptRegistry;

impl NodePromptRegistry {
    /// Create a new node prompt registry with all available prompts
    #[allow(clippy::new_ret_no_self)] // Factory method for registry
    pub fn new() -> PromptRegistry {
        let mut registry = PromptRegistry::new();

        // Status check prompt
        registry.register(simple_prompt!(
            "status_check",
            "Complete node health assessment including chain state, network connectivity, and sync status",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping assess the health of a Tari base node. Provide a comprehensive status check."
                ),
                text_message(
                    MessageRole::User,
                    "Please provide a complete health assessment of this Tari base node. Check the following:

1. Chain metadata and current height
2. Network connectivity and peer status  
3. Synchronization progress
4. Mempool statistics
5. Any potential issues or warnings

Use the available resources to gather this information and provide a clear summary."
                ),
                resource_message(MessageRole::User, "chain_metadata"),
                resource_message(MessageRole::User, "network_status"),
                resource_message(MessageRole::User, "sync_progress"),
                resource_message(MessageRole::User, "mempool_stats"),
                resource_message(MessageRole::User, "peer_list"),
            ]
        ));

        // Mining setup prompt
        registry.register(simple_prompt!(
            "mining_setup",
            "Guidance for setting up mining operations with this node",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping set up mining operations for a Tari base node."
                ),
                text_message(
                    MessageRole::User,
                    "I want to set up mining with this Tari base node. Please provide:

1. Current network difficulty and mining information
2. Steps to generate a block template
3. Recommended mining configurations
4. How to submit blocks once mined

Use the available tools and resources to provide practical guidance."
                ),
                resource_message(MessageRole::User, "network_difficulty"),
                resource_message(MessageRole::User, "chain_metadata"),
            ]
        ));

        // Peer diagnostics prompt
        registry.register(simple_prompt!(
            "peer_diagnostics",
            "Network connectivity troubleshooting and peer management guidance",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping diagnose network connectivity issues for a Tari base node."
                ),
                text_message(
                    MessageRole::User,
                    "I'm having network connectivity issues with my Tari base node. Please help diagnose:

1. Current peer connection status
2. Network connectivity health
3. Any blocked or banned peers
4. Recommendations for improving connectivity

Analyze the current state and provide actionable recommendations."
                ),
                resource_message(MessageRole::User, "network_status"),
                resource_message(MessageRole::User, "peer_list"),
            ]
        ));

        // Sync troubleshooting prompt
        registry.register(simple_prompt!(
            "sync_troubleshooting",
            "Blockchain synchronization issue diagnosis and resolution",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping troubleshoot blockchain synchronization issues for a Tari base node."
                ),
                text_message(
                    MessageRole::User,
                    "My Tari base node seems to have synchronization issues. Please help troubleshoot:

1. Current chain height vs network height
2. Sync progress and status
3. Peer connectivity that might affect sync
4. Potential causes and solutions

Provide a diagnostic assessment and recommendations."
                ),
                resource_message(MessageRole::User, "chain_metadata"),
                resource_message(MessageRole::User, "sync_progress"),
                resource_message(MessageRole::User, "network_status"),
            ]
        ));

        registry
    }
}
