use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::channels::types::{OutboundAdapter, OutboundAttachment, SendResult};
use crate::core::message_storage::MessageStorage;
use super::client::GithubClient;
use super::config::GithubConfig;

/// GitHub outbound adapter — posts comments on issues/PRs.
pub struct GithubOutboundAdapter {
    config: GithubConfig,
    storage: Arc<MessageStorage>,
    client: GithubClient,
}

impl GithubOutboundAdapter {
    pub fn new(config: GithubConfig, storage: Arc<MessageStorage>) -> Result<Self> {
        let client = GithubClient::new(&config)
            .context("Failed to create GitHub client for outbound")?;
        Ok(Self { config, storage, client })
    }
}

#[async_trait]
impl OutboundAdapter for GithubOutboundAdapter {
    fn channel_type(&self) -> &str {
        "github"
    }

    async fn connect(&self) -> Result<()> {
        tracing::info!(
            owner = %self.config.owner,
            repo = %self.config.repo,
            "GitHub outbound adapter connected"
        );
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        tracing::info!("GitHub outbound adapter disconnected");
        Ok(())
    }

    fn clean_body(&self, raw_body: &str) -> String {
        raw_body.to_string()
    }

    async fn send_reply(
        &self,
        original: &crate::channels::types::InboundMessage,
        reply_text: &str,
        thread_path: &Path,
        message_dir: &str,
        _attachments: Option<&[OutboundAttachment]>,
    ) -> Result<SendResult> {
        let number = original
            .metadata
            .get("github_number")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // Get role from metadata (set by message_router from pattern config)
        let role = original
            .metadata
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Build comment body with role prefix
        let comment_body = if role.is_empty() {
            reply_text.to_string()
        } else {
            format!("[{}] {}", role, reply_text)
        };

        // Post comment via GitHub API
        let comment_id = self.client.create_comment(number, &comment_body).await
            .with_context(|| format!("Failed to post comment on #{}", number))?;

        tracing::info!(
            number = number,
            comment_id = comment_id,
            role = role,
            reply_len = reply_text.len(),
            "GitHub comment posted"
        );

        // Store reply to chat log
        self.storage
            .store_reply(thread_path, reply_text, message_dir)
            .await?;

        Ok(SendResult {
            message_id: format!("github-comment-{}", comment_id),
        })
    }

    async fn send_alert(
        &self,
        _recipient: &str,
        _subject: &str,
        _body: &str,
    ) -> Result<SendResult> {
        tracing::debug!("GitHub send_alert: not implemented");
        Ok(SendResult {
            message_id: "github-alert-noop".to_string(),
        })
    }

    async fn send_heartbeat(
        &self,
        _original: &crate::channels::types::InboundMessage,
        _message: &str,
    ) -> Result<SendResult> {
        // GitHub doesn't need heartbeats — comments are discrete
        Ok(SendResult {
            message_id: "github-heartbeat-noop".to_string(),
        })
    }
}
