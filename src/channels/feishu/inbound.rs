//! Feishu inbound adapter implementation.
//! 
//! This module handles receiving messages from Feishu via WebSocket connections.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::channels::types::{ChannelPattern, InboundAdapterOptions, InboundMessage, PatternMatch};

use super::websocket::FeishuWebSocket;
use super::config::FeishuConfig;

/// Feishu inbound adapter for receiving messages via WebSocket.
pub struct FeishuInboundAdapter {
    websocket: Arc<Mutex<FeishuWebSocket>>,
    config: FeishuConfig,
}

impl FeishuInboundAdapter {
    /// Create a new Feishu inbound adapter.
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            websocket: Arc::new(Mutex::new(FeishuWebSocket::new(&config))),
            config,
        }
    }
}

#[async_trait]
impl crate::channels::types::InboundAdapter for FeishuInboundAdapter {
    fn channel_type(&self) -> &str {
        "feishu"
    }

    fn derive_thread_name(
        &self,
        message: &InboundMessage,
        _pattern_match: Option<&PatternMatch>,
    ) -> String {
        // Use chat ID or user ID for thread naming
        // Feishu messages have metadata with chat/user info
        if let Some(chat_id) = message.metadata.get("chat_id").and_then(|v| v.as_str()) {
            format!("feishu_chat_{}", chat_id)
        } else if let Some(user_id) = message.metadata.get("user_id").and_then(|v| v.as_str()) {
            format!("feishu_user_{}", user_id)
        } else {
            format!("feishu_{}", message.channel_uid)
        }
    }

    fn match_message(
        &self,
        message: &InboundMessage,
        patterns: &[ChannelPattern],
    ) -> Option<PatternMatch> {
        // Extract Feishu-specific metadata for matching
        let chat_id = message.metadata.get("chat_id").and_then(|v| v.as_str());
        let user_id = message.metadata.get("user_id").and_then(|v| v.as_str());
        
        // For Feishu, we can match based on metadata in the message
        // Patterns might have rules that reference Feishu-specific fields
        
        // Simple matching based on sender for now
        // TODO: Implement more sophisticated Feishu-specific matching
        // based on chat_id, user_id, message_type, etc.
        
        None
    }

    async fn start(
        &self,
        _options: InboundAdapterOptions,
        cancel: CancellationToken,
    ) -> Result<()> {
        if !self.config.websocket.enabled {
            tracing::info!("Feishu WebSocket is disabled in configuration");
            return Ok(());
        }
        
        tracing::info!("Starting Feishu inbound adapter with WebSocket...");
        
        // Connect WebSocket
        let mut websocket = self.websocket.lock().await;
        websocket.connect().await
            .context("Failed to connect Feishu WebSocket")?;
        
        // Start listening in background task
        let websocket_clone = Arc::clone(&self.websocket);
        let cancel_clone = cancel.clone();
        
        tokio::spawn(async move {
            let mut ws = websocket_clone.lock().await;
            
            // Start listening loop
            match ws.start_listening().await {
                Ok(_) => {
                    tracing::info!("Feishu WebSocket listener stopped normally");
                }
                Err(e) => {
                    tracing::error!("Feishu WebSocket listener error: {}", e);
                }
            }
            
            // Cleanup on cancellation
            if cancel_clone.is_cancelled() {
                let _ = ws.disconnect().await;
            }
        });
        
        tracing::info!("Feishu inbound adapter started successfully");
        Ok(())
    }
}