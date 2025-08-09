//! Authorization integration for gola-term
//!
//! This module provides integration between gola-term and the gola-core
//! authorization system, allowing the terminal client to handle tool
//! authorization requests.

use gola_ag_ui_types::{
    AuthorizationConfig, AuthorizationResponse, PendingAuthorization,
    ToolAuthorizationMode,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Response structure for pending authorizations endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingAuthorizationsResponse {
    pub status: String,
    pub pending_authorizations: Vec<PendingAuthorization>,
    pub count: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Response structure for authorization config endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthorizationConfigResponse {
    pub status: String,
    pub config: AuthorizationConfig,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Client for interacting with the server's authorization API
pub struct AuthorizationClient {
    client: Client,
    server_url: String,
    mode: Arc<Mutex<ToolAuthorizationMode>>,
    is_polling: Arc<Mutex<bool>>,
}

impl AuthorizationClient {
    pub fn new(server_url: String) -> Self {
        Self {
            client: Client::new(),
            server_url,
            mode: Arc::new(Mutex::new(ToolAuthorizationMode::Ask)),
            is_polling: Arc::new(Mutex::new(false)),
        }
    }

    /// Get the current authorization mode
    pub async fn get_mode(&self) -> ToolAuthorizationMode {
        *self.mode.lock().await
    }

    pub async fn set_mode(&self, mode: ToolAuthorizationMode) -> Result<(), String> {
        // Update local mode
        *self.mode.lock().await = mode;
        
        // Send to server
        let config = AuthorizationConfig::new(mode);
        self.update_server_config(config).await
    }

    /// Send an authorization response to the server
    pub async fn send_response(
        &self,
        tool_call_id: String,
        response: AuthorizationResponse,
    ) -> Result<(), String> {
        let auth_url = format!("{}/authorization", self.server_url);
        let auth_response = gola_ag_ui_types::ToolAuthorizationResponseEvent::new(
            tool_call_id,
            response,
        );

        match self.client
            .post(&auth_url)
            .header("Content-Type", "application/json")
            .json(&auth_response)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    log::debug!("Authorization response sent successfully");
                    Ok(())
                } else {
                    let error = format!("Failed to send authorization response: {}", resp.status());
                    log::error!("{}", error);
                    Err(error)
                }
            }
            Err(e) => {
                let error = format!("Failed to send authorization response: {}", e);
                log::error!("{}", error);
                Err(error)
            }
        }
    }

    /// Get pending authorization requests from the server
    pub async fn get_pending_authorizations(&self) -> Result<Vec<PendingAuthorization>, String> {
        let pending_url = format!("{}/authorization/pending", self.server_url);

        match self.client.get(&pending_url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    // First try to parse as the server's response format
                    match resp.json::<PendingAuthorizationsResponse>().await {
                        Ok(response) => {
                            if response.status == "success" {
                                Ok(response.pending_authorizations)
                            } else {
                                Err(format!("Server returned non-success status: {}", response.status))
                            }
                        }
                        Err(e) => {
                            // If that fails, log the error with more details
                            let error = format!("Failed to parse pending authorizations response: {}", e);
                            log::error!("{}", error);
                            log::debug!("Response parsing error details: {:?}", e);
                            Err(error)
                        }
                    }
                } else {
                    let error = format!(
                        "Failed to get pending authorizations: {}",
                        resp.status()
                    );
                    log::error!("{}", error);
                    Err(error)
                }
            }
            Err(e) => {
                let error = format!("Failed to get pending authorizations: {}", e);
                log::error!("{}", error);
                Err(error)
            }
        }
    }

    pub async fn cancel_authorization(&self, tool_call_id: String) -> Result<(), String> {
        let cancel_url = format!("{}/authorization/cancel", self.server_url);

        match self.client
            .post(&cancel_url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "tool_call_id": tool_call_id }))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    log::debug!("Authorization cancelled successfully");
                    Ok(())
                } else {
                    let error = format!("Failed to cancel authorization: {}", resp.status());
                    log::error!("{}", error);
                    Err(error)
                }
            }
            Err(e) => {
                let error = format!("Failed to cancel authorization: {}", e);
                log::error!("{}", error);
                Err(error)
            }
        }
    }

    /// Get the current authorization configuration from the server
    pub async fn get_server_config(&self) -> Result<AuthorizationConfig, String> {
        let config_url = format!("{}/authorization/config", self.server_url);

        match self.client.get(&config_url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    // Try to parse as the server's response format first
                    match resp.json::<AuthorizationConfigResponse>().await {
                        Ok(response) => {
                            if response.status == "success" {
                                // Update local mode to match server
                                *self.mode.lock().await = response.config.mode;
                                Ok(response.config)
                            } else {
                                Err(format!("Server returned non-success status: {}", response.status))
                            }
                        }
                        Err(e) => {
                            let error = format!("Failed to parse authorization config: {}", e);
                            log::error!("{}", error);
                            Err(error)
                        }
                    }
                } else {
                    let error = format!("Failed to get authorization config: {}", resp.status());
                    log::error!("{}", error);
                    Err(error)
                }
            }
            Err(e) => {
                let error = format!("Failed to get authorization config: {}", e);
                log::error!("{}", error);
                Err(error)
            }
        }
    }

    pub async fn update_server_config(&self, config: AuthorizationConfig) -> Result<(), String> {
        let config_url = format!("{}/authorization/config", self.server_url);

        match self.client
            .post(&config_url)
            .header("Content-Type", "application/json")
            .json(&config)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    log::debug!("Authorization config updated successfully");
                    Ok(())
                } else {
                    let error = format!("Failed to update authorization config: {}", resp.status());
                    log::error!("{}", error);
                    Err(error)
                }
            }
            Err(e) => {
                let error = format!("Failed to update authorization config: {}", e);
                log::error!("{}", error);
                Err(error)
            }
        }
    }

    pub async fn start_polling(&self, callback: impl Fn(Vec<PendingAuthorization>) + Send + Sync + 'static) {
        let is_polling = self.is_polling.clone();
        let client = self.clone();
        
        // Set polling flag
        *is_polling.lock().await = true;
        
        tokio::spawn(async move {
            while *is_polling.lock().await {
                // Poll for pending authorizations
                match client.get_pending_authorizations().await {
                    Ok(pending) => {
                        if !pending.is_empty() {
                            callback(pending);
                        }
                    }
                    Err(e) => {
                        log::error!("Error polling for pending authorizations: {}", e);
                    }
                }
                
                // Wait before polling again
                sleep(Duration::from_millis(500)).await;
            }
        });
    }

    /// Stop polling for pending authorizations
    pub async fn stop_polling(&self) {
        *self.is_polling.lock().await = false;
    }

    /// Sync local authorization mode with server
    pub async fn sync_with_server(&self) -> Result<(), String> {
        match self.get_server_config().await {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl std::fmt::Debug for AuthorizationClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationClient")
            .field("client", &"<reqwest::Client>")
            .field("server_url", &self.server_url)
            .field("mode", &self.mode) // Arc<Mutex<T>> is Debug if T is Debug
            .field("is_polling", &self.is_polling) // Arc<Mutex<bool>> is Debug
            .finish()
    }
}

impl Clone for AuthorizationClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            server_url: self.server_url.clone(),
            mode: self.mode.clone(),
            is_polling: self.is_polling.clone(),
        }
    }
}
