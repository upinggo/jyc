use anyhow::Result;
use base64::Engine;
use std::path::Path;

use crate::channels::types::InboundMessage;
use crate::core::email_parser;
use crate::utils::constants::*;

/// Build the system prompt for OpenCode.
///
/// Includes:
/// - Configured system prompt (from agent config)
/// - Security: directory boundary rules
/// - Reply instructions (use jiny_reply_reply_message tool)
/// - Optional thread-specific system.md
pub async fn build_system_prompt(
    thread_path: &Path,
    config_system_prompt: Option<&str>,
) -> String {
    let mut prompt = String::new();

    // Config-level system prompt
    if let Some(sp) = config_system_prompt {
        prompt.push_str(sp);
        prompt.push_str("\n\n");
    }

    // Security: directory boundaries
    prompt.push_str(&format!(
        r#"Your working directory is "{}". You MUST only read, write, and access files within this directory. Do NOT access files outside this directory.

## Security: Directory Boundaries
- NEVER use `..` or any relative path that resolves outside your working directory.
- Do NOT access, read, write, list, or reference any parent directories or sibling workspaces.
- Do NOT use absolute paths outside your working directory.
- If a task requires files outside this directory, refuse and explain you cannot access them.

## Important: Focus on the Current Message
You will see a "Conversation history" section and an "Incoming Message" section in the user prompt.
The conversation history is for CONTEXT ONLY — do NOT act on previous messages.
You MUST only respond to the CURRENT "Incoming Message". Do NOT continue work from previous messages.
After you have replied to the current message, STOP. Do not do anything else.

## Reply Instructions
When replying to a message, use the jiny_reply_reply_message tool:
- `token`: Pass the opaque token from the <reply_context> block exactly as-is (do not decode or modify it)
CRITICAL: DO NOT decode, modify, re-encode, or add any formatting (backticks, quotes, spaces, newlines) to the token.
Any change—even a single character—will break the reply.
- `message`: Your reply text
- `attachments`: Optional filenames to attach from the working directory
After a successful reply, STOP immediately. Do NOT call any other tools or perform further actions.
CRITICAL: Always use jiny_reply_reply_message tool.
"#,
        thread_path.display()
    ));

    // Thread-specific system.md
    let system_md_path = thread_path.join("system.md");
    if let Ok(content) = tokio::fs::read_to_string(&system_md_path).await {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            prompt.push('\n');
            prompt.push_str(trimmed);
            prompt.push('\n');
        }
    }

    prompt
}

/// Build the user prompt for a single inbound message.
///
/// Includes:
/// - Conversation history from stored messages (stripped, truncated)
/// - Incoming message body (stripped, truncated)
/// - Reply context token (base64-encoded metadata)
pub async fn build_prompt(
    message: &InboundMessage,
    thread_path: &Path,
    message_dir: &str,
    include_history: bool,
) -> Result<String> {
    let mut prompt = String::new();

    // Conversation history
    if include_history {
        let history = build_conversation_history(thread_path, message_dir).await;
        if !history.is_empty() {
            prompt.push_str("## Conversation history (most recent messages):\n");
            prompt.push_str(&history);
            prompt.push('\n');
        }
    }

    // Incoming message
    prompt.push_str("## Incoming Message\n");
    prompt.push_str(&format!(
        "**From:** {} <{}>\n",
        message.sender, message.sender_address
    ));
    prompt.push_str(&format!("**Subject:** {}\n", message.topic));
    prompt.push_str(&format!(
        "**Date:** {}\n\n",
        message.timestamp.to_rfc3339()
    ));

    // Body (stripped + truncated)
    let body = message
        .content
        .text
        .as_deref()
        .or(message.content.markdown.as_deref())
        .unwrap_or("[no text content]");
    let stripped = email_parser::strip_quoted_history(body);
    let truncated = email_parser::truncate_text(&stripped, MAX_BODY_IN_PROMPT);
    prompt.push_str("**Body:**\n");
    prompt.push_str(&truncated);
    prompt.push('\n');

    // Reply context token
    let context_token = serialize_reply_context(message, thread_path, message_dir);
    prompt.push_str(&format!("\n<reply_context>{context_token}</reply_context>\n"));

    Ok(prompt)
}

/// Build conversation history from stored messages in the thread.
///
/// Reads the last N message directories, strips quoted history,
/// and truncates per-file to fit the context budget.
async fn build_conversation_history(
    thread_path: &Path,
    current_message_dir: &str,
) -> String {
    let messages_dir = thread_path.join("messages");
    if !messages_dir.exists() {
        return String::new();
    }

    // Read and sort message directories (newest first)
    let mut dirs: Vec<String> = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&messages_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(ft) = entry.file_type().await {
                if ft.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    // Skip current message dir (it's the one being processed)
                    if name != current_message_dir {
                        dirs.push(name);
                    }
                }
            }
        }
    }
    dirs.sort();
    dirs.reverse(); // Newest first

    // Take last N
    let dirs: Vec<_> = dirs.into_iter().take(MAX_FILES_IN_CONTEXT).collect();

    let mut history = String::new();
    let mut total_chars = 0;

    // Build history (oldest first for chronological reading)
    for dir_name in dirs.into_iter().rev() {
        let dir_path = messages_dir.join(&dir_name);

        // Reply (AI response) — comes after received in chronological order
        let reply_path = dir_path.join("reply.md");
        if let Ok(content) = tokio::fs::read_to_string(&reply_path).await {
            let reply_text = email_parser::parse_stored_reply(&content);
            let truncated = email_parser::truncate_text(&reply_text, MAX_PER_FILE);
            if total_chars + truncated.len() <= MAX_TOTAL_CONTEXT {
                history.push_str(&format!("### AI Assistant ({})\n", dir_name));
                history.push_str(&truncated);
                history.push_str("\n\n");
                total_chars += truncated.len();
            }
        }

        // Received message
        let received_path = dir_path.join("received.md");
        if let Ok(content) = tokio::fs::read_to_string(&received_path).await {
            let parsed = email_parser::parse_stored_message(&content);
            let stripped = email_parser::strip_quoted_history(&parsed.body);
            let truncated = email_parser::truncate_text(&stripped, MAX_PER_FILE);
            if total_chars + truncated.len() <= MAX_TOTAL_CONTEXT {
                let sender = parsed.sender.as_deref().unwrap_or("Unknown");
                let ts = parsed.timestamp.as_deref().unwrap_or(&dir_name);
                history.push_str(&format!("### {sender} ({ts})\n"));
                history.push_str(&truncated);
                history.push_str("\n\n");
                total_chars += truncated.len();
            }
        }
    }

    history
}

/// Serialize a reply context token (metadata → JSON → base64).
///
/// The token is opaque — the AI passes it through unchanged to the reply tool.
fn serialize_reply_context(
    message: &InboundMessage,
    thread_path: &Path,
    message_dir: &str,
) -> String {
    let thread_name = thread_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Generate nonce for integrity
    let nonce = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    let context = serde_json::json!({
        "channel": message.channel,
        "threadName": thread_name,
        "sender": message.sender,
        "recipient": message.sender_address,
        "topic": message.topic,
        "timestamp": message.timestamp.to_rfc3339(),
        "incomingMessageDir": message_dir,
        "externalId": message.external_id,
        "threadRefs": message.thread_refs,
        "uid": message.channel_uid,
        "_nonce": nonce,
        "channelMetadata": message.metadata,
    });

    let json = serde_json::to_string(&context).unwrap_or_default();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::types::{MessageContent, InboundMessage};
    use std::collections::HashMap;

    fn test_message() -> InboundMessage {
        InboundMessage {
            id: "test".to_string(),
            channel: "email".to_string(),
            channel_uid: "42".to_string(),
            sender: "John".to_string(),
            sender_address: "john@example.com".to_string(),
            recipients: vec![],
            topic: "Test Subject".to_string(),
            content: MessageContent {
                text: Some("Hello, help me.".to_string()),
                html: None,
                markdown: None,
            },
            timestamp: chrono::Utc::now(),
            thread_refs: None,
            reply_to_id: None,
            external_id: Some("<msg123@example.com>".to_string()),
            attachments: vec![],
            metadata: HashMap::new(),
            matched_pattern: None,
        }
    }

    #[tokio::test]
    async fn test_build_system_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = build_system_prompt(
            tmp.path(),
            Some("Be helpful."),
        ).await;

        assert!(prompt.contains("Be helpful."));
        assert!(prompt.contains("jiny_reply_reply_message"));
        assert!(prompt.contains("Directory Boundaries"));
    }

    #[tokio::test]
    async fn test_build_system_prompt_with_system_md() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(
            tmp.path().join("system.md"),
            "You are a code reviewer.",
        ).await.unwrap();

        let prompt = build_system_prompt(tmp.path(), None).await;
        assert!(prompt.contains("You are a code reviewer."));
    }

    #[tokio::test]
    async fn test_build_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let msg = test_message();

        let prompt = build_prompt(&msg, tmp.path(), "2026-03-27_10-00-00", false)
            .await
            .unwrap();

        assert!(prompt.contains("## Incoming Message"));
        assert!(prompt.contains("John"));
        assert!(prompt.contains("john@example.com"));
        assert!(prompt.contains("Hello, help me."));
        assert!(prompt.contains("<reply_context>"));
    }

    #[test]
    fn test_serialize_reply_context() {
        let msg = test_message();
        let tmp = tempfile::tempdir().unwrap();
        let token = serialize_reply_context(&msg, tmp.path(), "2026-03-27_10-00-00");

        // Should be valid base64
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&token)
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&decoded).unwrap();

        assert_eq!(json["channel"], "email");
        assert_eq!(json["recipient"], "john@example.com");
        assert_eq!(json["incomingMessageDir"], "2026-03-27_10-00-00");
        assert!(json["_nonce"].is_string());
    }
}
