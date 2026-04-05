//! Feishu outbound adapter implementation.
//! 
//! This module handles sending messages to Feishu via HTTP API.

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::channels::feishu::client::FeishuClient;
use crate::channels::feishu::config::FeishuConfig;
use crate::channels::types::{InboundMessage, OutboundAttachment, SendResult};

/// Feishu outbound adapter for sending messages via HTTP API.
pub struct FeishuOutboundAdapter {
    client: FeishuClient,
}

impl FeishuOutboundAdapter {
    /// Create a new Feishu outbound adapter.
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            client: FeishuClient::new(config),
        }
    }
}

#[async_trait]
impl crate::channels::types::OutboundAdapter for FeishuOutboundAdapter {
    fn channel_type(&self) -> &str {
        "feishu"
    }

    async fn connect(&self) -> Result<()> {
        // Client initialization is lazy and will happen on first use
        tracing::info!("Feishu outbound adapter connected");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        tracing::info!("Feishu outbound adapter disconnected");
        Ok(())
    }

    async fn send_reply(
        &self,
        original: &InboundMessage,
        reply_text: &str,
        _attachments: Option<&[OutboundAttachment]>,
    ) -> Result<SendResult> {
        // Ensure client is initialized
        self.client.initialize().await
            .context("Failed to initialize Feishu client before sending reply")?;
        
        // Extract chat ID from original message
        let chat_id = original.channel_uid.as_str();
        
        // Send message using Feishu client
        let result = self.client.send_text_message(chat_id, reply_text).await
            .context("Failed to send Feishu reply")?;
        
        tracing::info!("Feishu reply sent to {}: {}", chat_id, &reply_text[..reply_text.len().min(50)]);
        
        Ok(SendResult {
            message_id: result.message_id,
        })
    }

    async fn send_alert(
        &self,
        recipient: &str,
        subject: &str,
        body: &str,
    ) -> Result<SendResult> {
        // Ensure client is initialized
        self.client.initialize().await
            .context("Failed to initialize Feishu client before sending alert")?;
        
        // Format alert message
        let alert_text = format!("**{}**\n\n{}", subject, body);
        
        // Send alert using Feishu client
        let result = self.client.send_text_message(recipient, &alert_text).await
            .context("Failed to send Feishu alert")?;
        
        tracing::info!("Feishu alert sent to {}: {}", recipient, subject);
        
        Ok(SendResult {
            message_id: result.message_id,
        })
    }

    async fn send_progress_update(
        &self,
        original: &InboundMessage,
        elapsed_ms: u64,
        activity: &str,
    ) -> Result<SendResult> {
        // Ensure client is initialized
        self.client.initialize().await
            .context("Failed to initialize Feishu client before sending progress update")?;
        
        // Extract chat ID from original message
        let chat_id = original.channel_uid.as_str();
        
        // Format progress update message
        let progress_text = format!("⏳ {} ({}ms elapsed)", activity, elapsed_ms);
        
        // Send progress update using Feishu client
        let result = self.client.send_text_message(chat_id, &progress_text).await
            .context("Failed to send Feishu progress update")?;
        
        tracing::debug!("Feishu progress update sent to {}: {}", chat_id, activity);
        
        Ok(SendResult {
            message_id: result.message_id,
        })
    }
}