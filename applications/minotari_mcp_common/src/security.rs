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
//! Security and permission management for MCP operations

use std::{collections::HashMap, net::IpAddr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{McpError, McpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    /// Read-only operations that don't modify state
    ReadOnly,
    /// Operations that modify state but are generally safe
    Control,
    /// Dangerous operations that require explicit user consent
    Privileged,
}

#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Whether control operations are globally enabled
    control_enabled: bool,
    /// Rate limiting tracking
    rate_limiter: RateLimiter,
    /// Audit logger
    audit_logger: AuditLogger,
    /// Session tracking
    sessions: HashMap<String, Session>,
}

#[derive(Debug, Clone)]
struct Session {
    id: Uuid,
    client_ip: IpAddr,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    #[allow(dead_code)]
    request_count: u32,
}

#[derive(Debug, Clone)]
struct RateLimiter {
    requests_per_minute: u32,
    client_requests: HashMap<IpAddr, Vec<DateTime<Utc>>>,
}

#[derive(Debug, Clone)]
pub struct AuditLogger {
    enabled: bool,
    log_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    timestamp: DateTime<Utc>,
    session_id: Uuid,
    client_ip: IpAddr,
    operation: String,
    permission_level: PermissionLevel,
    success: bool,
    error_message: Option<String>,
    request_data: serde_json::Value,
}

impl SecurityContext {
    pub fn new(control_enabled: bool, rate_limit_per_minute: u32, audit_log_path: Option<String>) -> Self {
        Self {
            control_enabled,
            rate_limiter: RateLimiter::new(rate_limit_per_minute),
            audit_logger: AuditLogger::new(audit_log_path),
            sessions: HashMap::new(),
        }
    }

    /// Check if a client can perform an operation with the given permission level
    pub fn check_permission(
        &mut self,
        client_ip: IpAddr,
        operation: &str,
        permission_level: PermissionLevel,
        request_data: serde_json::Value,
    ) -> McpResult<Uuid> {
        // Enforce local-only access
        if !client_ip.is_loopback() {
            let error =
                McpError::permission_denied("Remote access not allowed - MCP server only accepts local connections");
            self.audit_logger.log_operation(
                Uuid::new_v4(),
                client_ip,
                operation,
                permission_level,
                false,
                Some(error.to_string()),
                request_data,
            );
            return Err(error);
        }

        // Check rate limiting
        if !self.rate_limiter.check_rate_limit(client_ip) {
            let error = McpError::RateLimitExceeded;
            self.audit_logger.log_operation(
                Uuid::new_v4(),
                client_ip,
                operation,
                permission_level,
                false,
                Some(error.to_string()),
                request_data.clone(),
            );
            return Err(error);
        }

        // Check permission level
        match permission_level {
            PermissionLevel::ReadOnly => {
                // Always allowed
            },
            PermissionLevel::Control | PermissionLevel::Privileged => {
                if !self.control_enabled {
                    let error = McpError::permission_denied(
                        "Control operations disabled - use --mcp-control-enabled flag to enable",
                    );
                    self.audit_logger.log_operation(
                        Uuid::new_v4(),
                        client_ip,
                        operation,
                        permission_level,
                        false,
                        Some(error.to_string()),
                        request_data,
                    );
                    return Err(error);
                }
            },
        }

        // Create or update session
        let session_id = self.create_or_update_session(client_ip);

        // Log successful permission check
        self.audit_logger.log_operation(
            session_id,
            client_ip,
            operation,
            permission_level,
            true,
            None,
            request_data,
        );

        Ok(session_id)
    }

    fn create_or_update_session(&mut self, client_ip: IpAddr) -> Uuid {
        let now = Utc::now();

        // Find existing session for this IP
        let session_id = self
            .sessions
            .values()
            .find(|s| s.client_ip == client_ip)
            .map(|s| s.id)
            .unwrap_or_else(Uuid::new_v4);

        let session = Session {
            id: session_id,
            client_ip,
            created_at: now,
            last_activity: now,
            request_count: 1,
        };

        self.sessions.insert(session_id.to_string(), session);
        session_id
    }

    pub fn cleanup_expired_sessions(&mut self) {
        // On the (impossible in practice) overflow, keep every session rather than dropping them all
        let cutoff = Utc::now()
            .checked_sub_signed(chrono::Duration::hours(1))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        self.sessions.retain(|_, session| session.last_activity > cutoff);
    }
}

impl RateLimiter {
    fn new(requests_per_minute: u32) -> Self {
        Self {
            requests_per_minute,
            client_requests: HashMap::new(),
        }
    }

    fn check_rate_limit(&mut self, client_ip: IpAddr) -> bool {
        let now = Utc::now();
        // On the (impossible in practice) overflow, keep every request, i.e. rate limit more strictly
        let cutoff = now
            .checked_sub_signed(chrono::Duration::minutes(1))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);

        let requests = self.client_requests.entry(client_ip).or_default();

        // Remove old requests
        requests.retain(|&timestamp| timestamp > cutoff);

        // Check if under limit
        if requests.len() < self.requests_per_minute as usize {
            requests.push(now);
            true
        } else {
            false
        }
    }
}

impl AuditLogger {
    fn new(log_path: Option<String>) -> Self {
        Self {
            enabled: log_path.is_some(),
            log_path,
        }
    }

    fn log_operation(
        &self,
        session_id: Uuid,
        client_ip: IpAddr,
        operation: &str,
        permission_level: PermissionLevel,
        success: bool,
        error_message: Option<String>,
        request_data: serde_json::Value,
    ) {
        if !self.enabled {
            return;
        }

        let _entry = AuditEntry {
            timestamp: Utc::now(),
            session_id,
            client_ip,
            operation: operation.to_string(),
            permission_level,
            success,
            error_message: error_message.clone(),
            request_data,
        };

        // Log to structured logger
        if success {
            log::info!("MCP Operation: {operation} by {client_ip} (session: {session_id})");
        } else {
            log::warn!(
                "MCP Operation Failed: {} by {} (session: {}) - {}",
                operation,
                client_ip,
                session_id,
                error_message.unwrap_or_else(|| "Unknown error".to_string())
            );
        }

        // TODO: Also write to structured audit log file if path is configured
        if let Some(_path) = &self.log_path {
            // Write JSON entry to audit log file
            // This would be implemented based on Tari's logging infrastructure
        }
    }
}
