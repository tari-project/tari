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
//! Wallet-specific MCP prompts

use minotari_mcp_common::{
    prompts::MessageRole,
    resource_message,
    simple_prompt,
    text_message,
    McpResult,
    PromptRegistry,
};

/// Registry for wallet-specific MCP prompts
pub struct WalletPromptRegistry;

impl WalletPromptRegistry {
    /// Create a new wallet prompt registry with all available prompts
    #[allow(clippy::new_ret_no_self)] // Factory method for registry
    pub fn new() -> PromptRegistry {
        let mut registry = PromptRegistry::new();

        // Balance check prompt
        registry.register(simple_prompt!(
            "balance_check",
            "Complete wallet balance and status overview including connectivity and transaction status",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping assess the status of a Tari wallet. Provide a comprehensive balance and status \
                     overview."
                ),
                text_message(
                    MessageRole::User,
                    "Please provide a complete wallet status check including:

1. Current balance breakdown (available, pending, time-locked)
2. Network connectivity status
3. Recent transaction activity
4. Address information for receiving funds
5. Any pending transactions or issues

Use the available resources to gather this information and provide a clear summary."
                ),
                resource_message(MessageRole::User, "balance"),
                resource_message(MessageRole::User, "network_status"),
                resource_message(MessageRole::User, "transaction_history"),
                resource_message(MessageRole::User, "addresses"),
                resource_message(MessageRole::User, "connected_peers"),
            ]
        ));

        // Send transaction prompt
        registry.register(simple_prompt!(
            "send_transaction",
            "Step-by-step guidance for sending Tari with transaction best practices",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping a user send Tari from their wallet. Provide step-by-step guidance with security \
                     best practices."
                ),
                text_message(
                    MessageRole::User,
                    "I want to send Tari from my wallet. Please provide guidance on:

1. How to check my available balance
2. Best practices for setting transaction amounts and fees
3. Address formats and validation
4. Transaction confirmation process
5. How to track the transaction after sending

Provide a complete guide with security considerations."
                ),
                resource_message(MessageRole::User, "balance"),
                resource_message(MessageRole::User, "addresses"),
                resource_message(MessageRole::User, "network_status"),
            ]
        ));

        // Transaction troubleshooting prompt
        registry.register(simple_prompt!(
            "transaction_troubleshooting",
            "Diagnosis and resolution guidance for transaction issues",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping troubleshoot transaction issues for a Tari wallet."
                ),
                text_message(
                    MessageRole::User,
                    "I'm having issues with a transaction. Please help troubleshoot:

1. Check transaction status and history
2. Identify pending or failed transactions
3. Network connectivity that might affect transactions
4. Balance and UTXO availability
5. Recommendations for resolving issues

Analyze the current wallet state and provide actionable recommendations."
                ),
                resource_message(MessageRole::User, "transaction_history"),
                resource_message(MessageRole::User, "balance"),
                resource_message(MessageRole::User, "network_status"),
                resource_message(MessageRole::User, "connected_peers"),
            ]
        ));

        // Wallet recovery prompt
        registry.register(simple_prompt!(
            "wallet_recovery",
            "Wallet recovery and validation procedures for backup and restore operations",
            vec![
                text_message(
                    MessageRole::System,
                    "You are helping with wallet recovery and validation procedures for a Tari wallet."
                ),
                text_message(
                    MessageRole::User,
                    "I need help with wallet recovery and validation. Please provide guidance on:

1. Current wallet status and balance verification
2. Address validation and backup procedures
3. Transaction history verification
4. Network connectivity requirements for recovery
5. Best practices for wallet backup and security

Provide comprehensive recovery guidance with security considerations."
                ),
                resource_message(MessageRole::User, "balance"),
                resource_message(MessageRole::User, "addresses"),
                resource_message(MessageRole::User, "transaction_history"),
                resource_message(MessageRole::User, "network_status"),
            ]
        ));

        registry
    }
}
