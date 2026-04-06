//! Feishu API client wrapper.
//!
//! This module provides a high-level client for Feishu API interactions
//! using the openlark SDK.

use anyhow::{Context, Result};
use open_lark::prelude::*;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

use super::config::FeishuConfig;

/// Feishu client errors.
#[derive(Debug, Error)]
pub enum FeishuError {
    /// Client not initialized
    #[error("Feishu client not initialized. Call initialize() first")]
    NotInitialized,

    /// Configuration error
    #[error("Feishu configuration error: {0}")]
    ConfigError(String),

    /// API error
    #[error("Feishu API error: {0}")]
    ApiError(String),

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthError(String),
}

/// Feishu API client wrapper.
///
/// Wraps the openlark `Client` and provides high-level methods for
/// sending messages and managing authentication.
pub struct FeishuClient {
    config: FeishuConfig,
    client: Arc<RwLock<Option<Client>>>,
}

impl FeishuClient {
    /// Create a new Feishu client.
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            config,
            client: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the openlark client (lazy — only called on first use).
    pub async fn initialize(&self) -> Result<()> {
        let mut client = self.client.write().await;
        if client.is_none() {
            let openlark_client = Client::builder()
                .app_id(&self.config.app_id)
                .app_secret(&self.config.app_secret)
                .base_url(&self.config.base_url)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build openlark client: {e}"))?;

            tracing::info!(
                app_id = %self.config.app_id,
                "Feishu client initialized"
            );
            *client = Some(openlark_client);
        }
        Ok(())
    }

    /// Get the internal openlark client.
    async fn get_client(&self) -> Result<Client> {
        let client_guard = self.client.read().await;
        client_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(FeishuError::NotInitialized))
    }

    /// Get the openlark core config (for use with IM APIs and AuthService).
    async fn get_core_config(&self) -> Result<open_lark::core::config::Config> {
        let client = self.get_client().await?;
        Ok(client.api_config().clone())
    }

    /// Get the current app access token.
    ///
    /// Uses the openlark AuthService to request a tenant_access_token.
    /// The token is managed internally by the SDK with caching.
    pub async fn get_token(&self) -> Result<String> {
        let core_config = self.get_core_config().await?;
        let auth = open_lark::auth::AuthService::new(core_config);
        let resp = auth
            .v3()
            .app_access_token_internal()
            .app_id(&self.config.app_id)
            .app_secret(&self.config.app_secret)
            .execute()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get Feishu app access token: {e}"))?;

        Ok(resp.data.app_access_token)
    }

    /// Send a text message to a chat.
    ///
    /// Uses the openlark IM API to send a message to the specified chat_id.
    /// For p2p (direct messages), `chat_id` should be the user's open_id
    /// and `receive_id_type` should be `OpenId`.
    pub async fn send_text_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<FeishuMessageResult> {
        let core_config = self.get_core_config().await?;

        use open_lark::communication::im::im::v1::message::create::{
            CreateMessageBody, CreateMessageRequest,
        };
        use open_lark::communication::im::im::v1::message::models::ReceiveIdType;

        let body = CreateMessageBody {
            receive_id: chat_id.to_string(),
            msg_type: "text".to_string(),
            content: serde_json::json!({"text": text}).to_string(),
            uuid: None,
        };

        let resp = CreateMessageRequest::new(core_config)
            .receive_id_type(ReceiveIdType::ChatId)
            .execute(body)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send Feishu message: {e}"))?;

        // Extract message_id from response JSON
        let message_id = resp
            .get("data")
            .and_then(|d| d.get("message_id"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        tracing::info!(
            chat_id = %chat_id,
            message_id = %message_id,
            text_len = text.len(),
            "Feishu message sent"
        );

        Ok(FeishuMessageResult { message_id })
    }
}

/// Result of sending a Feishu message.
#[derive(Debug, Clone)]
pub struct FeishuMessageResult {
    pub message_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::feishu::config::{FeishuConfig, WebSocketConfig};

    #[test]
    fn test_feishu_client_creation() {
        let config = FeishuConfig {
            app_id: "test_app_id".to_string(),
            app_secret: "test_app_secret".to_string(),
            base_url: "https://open.feishu.cn".to_string(),
            websocket: WebSocketConfig::default(),
            events: vec![],
            message_format: "markdown".to_string(),
            metadata: Default::default(),
        };

        let _client = FeishuClient::new(config);
    }

    #[test]
    fn test_feishu_message_result() {
        let result = FeishuMessageResult {
            message_id: "test_message_123".to_string(),
        };

        assert_eq!(result.message_id, "test_message_123");

        let cloned = result.clone();
        assert_eq!(cloned.message_id, result.message_id);
    }
}
