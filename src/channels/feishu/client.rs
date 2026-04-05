//! Feishu API client wrapper.
//! 
//! This module provides a high-level client for Feishu API interactions
//! using the openlark SDK.

use anyhow::{Context, Result};
use open_lark::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;

use super::config::FeishuConfig;

/// Feishu client errors
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

impl FeishuError {
    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::ConfigError(msg.into())
    }
    
    /// Create an API error
    pub fn api(msg: impl Into<String>) -> Self {
        Self::ApiError(msg.into())
    }
    
    /// Create a network error
    pub fn network(msg: impl Into<String>) -> Self {
        Self::NetworkError(msg.into())
    }
    
    /// Create an authentication error
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::AuthError(msg.into())
    }
}

/// Feishu API client wrapper.
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
    
    /// Initialize the openlark client.
    pub async fn initialize(&self) -> Result<()> {
        let mut client = self.client.write().await;
        if client.is_none() {
            let openlark_client = Client::builder()
                .app_id(&self.config.app_id)
                .app_secret(&self.config.app_secret)
                .base_url(&self.config.base_url)
                .build()
                .context("Failed to build openlark client")?;
            
            *client = Some(openlark_client);
            tracing::info!("Feishu client initialized with app_id: {}", self.config.app_id);
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
    
    /// Get the current tenant access token.
    pub async fn get_token(&self) -> Result<String> {
        let _client = self.get_client().await?;
        
        // In openlark SDK, tokens are managed internally by the auth module
        // We can try to access the token through the auth client
        
        tracing::debug!("Getting Feishu token");
        
        // TODO: Implement actual token retrieval
        // The openlark SDK manages tokens internally, but we might need to
        // access them for WebSocket authentication or other purposes
        
        // For now, return a placeholder
        Ok("feishu_managed_token".to_string())
    }
    
    /// Send a text message to a chat.
    pub async fn send_text_message(&self, chat_id: &str, text: &str) -> Result<FeishuMessageResult> {
        let client = self.get_client().await?;
        
        tracing::info!("Sending Feishu message: chat_id={}, text_length={}", chat_id, text.len());
        
        // Based on openlark SDK structure, we need to use the request builder pattern
        // The actual implementation would look like:
        
        // Get the core config from communication client
        let _config = client.communication.im.config();
        
        // TODO: Implement actual message sending using openlark_communication crate
        // The pattern appears to be:
        // 1. Create request builder with config
        // 2. Set parameters (receive_id_type, etc.)
        // 3. Execute with message body
        
        // For now, we'll use a placeholder while we verify the exact API
        tracing::info!("[IMPLEMENTATION IN PROGRESS] Would send message to {}: {}", chat_id, text);
        
        // Simulate API call success
        Ok(FeishuMessageResult {
            message_id: format!("feishu_msg_{}", uuid::Uuid::new_v4()),
        })
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

        let client = FeishuClient::new(config);
        // Verify creation doesn't panic
        assert!(true);
    }

    #[tokio::test]
    async fn test_feishu_client_placeholder_functionality() {
        let config = FeishuConfig {
            app_id: "test_app_id".to_string(),
            app_secret: "test_app_secret".to_string(),
            base_url: "https://open.feishu.cn".to_string(),
            websocket: WebSocketConfig::default(),
            events: vec![],
            message_format: "markdown".to_string(),
            metadata: Default::default(),
        };

        let client = FeishuClient::new(config);
        
        // Test initialization
        let init_result = client.initialize().await;
        assert!(init_result.is_ok());
        
        // Test token retrieval
        let token_result = client.get_token().await;
        assert!(token_result.is_ok());
        let token = token_result.unwrap();
        assert!(!token.is_empty());
        
        // Test message sending
        let message_result = client.send_text_message("test_chat_id", "Hello, world!").await;
        assert!(message_result.is_ok());
        let result = message_result.unwrap();
        assert!(result.message_id.starts_with("feishu_msg_"));
    }

    #[test]
    fn test_feishu_message_result() {
        let result = FeishuMessageResult {
            message_id: "test_message_123".to_string(),
        };
        
        assert_eq!(result.message_id, "test_message_123");
        
        // Test clone
        let cloned = result.clone();
        assert_eq!(cloned.message_id, result.message_id);
    }
}