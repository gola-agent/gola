//! Authorization types for tool execution guardrails.

use serde::{Deserialize, Serialize};

/// Authorization mode for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAuthorizationMode {
    /// Always allow tool execution without prompting.
    AlwaysAllow,
    /// Always deny tool execution.
    AlwaysDeny,
    /// Ask user for authorization before each tool execution.
    Ask,
}

impl Default for ToolAuthorizationMode {
    fn default() -> Self {
        Self::Ask
    }
}

/// User's response to a tool authorization request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationResponse {
    /// Approve this specific tool execution.
    Approve,
    /// Deny this specific tool execution.
    Deny,
    /// Approve this tool execution and switch to always allow mode.
    ApproveAndAllow,
}

/// Event requesting authorization for a tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAuthorizationRequestEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the tool call requiring authorization.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// The name of the tool being called.
    #[serde(rename = "toolCallName")]
    pub tool_call_name: String,
    /// The arguments for the tool call.
    #[serde(rename = "toolCallArgs")]
    pub tool_call_args: String,
    /// A human-readable description of what the tool will do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Event containing the user's authorization response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAuthorizationResponseEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the tool call being responded to.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// The user's authorization response.
    pub response: AuthorizationResponse,
}

impl ToolAuthorizationRequestEvent {
    /// Create a new tool authorization request event.
    pub fn new(tool_call_id: String, tool_call_name: String, tool_call_args: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
            tool_call_name,
            tool_call_args,
            description: None,
        }
    }

    /// Create a new tool authorization request event with description.
    pub fn with_description(
        tool_call_id: String,
        tool_call_name: String,
        tool_call_args: String,
        description: String,
    ) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
            tool_call_name,
            tool_call_args,
            description: Some(description),
        }
    }
}

impl ToolAuthorizationResponseEvent {
    /// Create a new tool authorization response event.
    pub fn new(tool_call_id: String, response: AuthorizationResponse) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
            response,
        }
    }
}

/// Authorization configuration for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationConfig {
    /// The current authorization mode.
    pub mode: ToolAuthorizationMode,
    /// Whether authorization is enabled.
    pub enabled: bool,
    /// Optional message to display to users when requesting authorization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_message: Option<String>,
    /// Timeout for authorization requests in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            mode: ToolAuthorizationMode::Ask,
            enabled: true,
            prompt_message: None,
            timeout_seconds: Some(30),
        }
    }
}

impl AuthorizationConfig {
    /// Create a new authorization configuration.
    pub fn new(mode: ToolAuthorizationMode) -> Self {
        Self {
            mode,
            enabled: true,
            prompt_message: None,
            timeout_seconds: Some(30),
        }
    }

    /// Create a disabled authorization configuration.
    pub fn disabled() -> Self {
        Self {
            mode: ToolAuthorizationMode::AlwaysAllow,
            enabled: false,
            prompt_message: None,
            timeout_seconds: None,
        }
    }

    /// Set the prompt message.
    pub fn with_prompt_message(mut self, message: String) -> Self {
        self.prompt_message = Some(message);
        self
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }

    /// Enable or disable authorization.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Status of a tool authorization request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationStatus {
    /// Authorization is pending user response.
    Pending,
    /// Authorization was approved.
    Approved,
    /// Authorization was denied.
    Denied,
    /// Authorization request timed out.
    TimedOut,
    /// Authorization was cancelled.
    Cancelled,
}

/// Information about a pending tool authorization request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingAuthorization {
    /// The ID of the tool call requiring authorization.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// The name of the tool being called.
    #[serde(rename = "toolCallName")]
    pub tool_call_name: String,
    /// The arguments for the tool call.
    #[serde(rename = "toolCallArgs")]
    pub tool_call_args: String,
    /// A human-readable description of what the tool will do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The current status of the authorization.
    pub status: AuthorizationStatus,
    /// Timestamp when the request was created.
    pub created_at: i64,
    /// Timestamp when the request expires (optional).
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl PendingAuthorization {
    /// Create a new pending authorization.
    pub fn new(tool_call_id: String, tool_call_name: String, tool_call_args: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            tool_call_id,
            tool_call_name,
            tool_call_args,
            description: None,
            status: AuthorizationStatus::Pending,
            created_at: now,
            expires_at: None,
        }
    }

    /// Create a new pending authorization with description.
    pub fn with_description(
        tool_call_id: String,
        tool_call_name: String,
        tool_call_args: String,
        description: String,
    ) -> Self {
        let mut auth = Self::new(tool_call_id, tool_call_name, tool_call_args);
        auth.description = Some(description);
        auth
    }

    /// Set the expiration time.
    pub fn with_expiration(mut self, expires_at: i64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Check if the authorization has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            chrono::Utc::now().timestamp() > expires_at
        } else {
            false
        }
    }

    /// Update the status.
    pub fn with_status(mut self, status: AuthorizationStatus) -> Self {
        self.status = status;
        self
    }
}

/// Event indicating that an authorization request has been updated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationStatusEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the tool call.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// The new status.
    pub status: AuthorizationStatus,
    /// Optional message explaining the status change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl AuthorizationStatusEvent {
    /// Create a new authorization status event.
    pub fn new(tool_call_id: String, status: AuthorizationStatus) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
            status,
            message: None,
        }
    }

    /// Create a new authorization status event with message.
    pub fn with_message(
        tool_call_id: String,
        status: AuthorizationStatus,
        message: String,
    ) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
            status,
            message: Some(message),
        }
    }
}
