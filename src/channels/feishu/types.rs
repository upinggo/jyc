//! Feishu-specific types and conversions.
//!
//! This module contains types for converting between Feishu API formats
//! and JYC internal formats.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Feishu message content in various formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuMessageContent {
    /// Text content
    pub text: Option<String>,
    /// Markdown content
    pub markdown: Option<String>,
    /// Image key (for image messages)
    pub image_key: Option<String>,
    /// File key (for file messages)
    pub file_key: Option<String>,
}

/// Feishu user information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuUser {
    /// User ID
    pub user_id: String,
    /// Display name
    pub name: String,
    /// Email address
    pub email: Option<String>,
}

/// Feishu chat information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuChat {
    /// Chat ID
    pub chat_id: String,
    /// Chat type: "p2p" or "group"
    pub chat_type: String,
    /// Chat name
    pub name: String,
}

/// Feishu message event from WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuMessageEvent {
    /// Message ID
    pub message_id: String,
    /// Chat information
    pub chat: FeishuChat,
    /// Sender information
    pub sender: FeishuUser,
    /// Message content
    pub content: FeishuMessageContent,
    /// Create timestamp (milliseconds)
    pub create_time: i64,
    /// Parent message ID (for replies)
    pub parent_id: Option<String>,
    /// Mentioned users
    pub mentions: Vec<FeishuUser>,
    /// Additional metadata
    pub extra: HashMap<String, serde_json::Value>,
}

/// Feishu event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FeishuEvent {
    /// Message received event
    #[serde(rename = "im.message.receive_v1")]
    MessageReceived {
        /// Event data
        event: FeishuMessageEvent,
    },
    /// Bot added to chat event
    #[serde(rename = "im.chat.member.bot.added_v1")]
    BotAddedToChat {
        /// Chat ID
        chat_id: String,
        /// Operator user ID
        operator_id: String,
    },
    // TODO: Add more event types
}
