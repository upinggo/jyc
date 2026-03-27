use anyhow::{bail, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Minimal reply context token — channel-agnostic routing + file location.
///
/// The token is intentionally minimal to reduce corruption risk from AI models.
/// All message metadata (sender, recipient, topic, threading headers) is read
/// from the stored received.md file by the reply tool — NOT from the token.
///
/// Token fields:
/// - `channel`: config channel name (routing key for outbound adapter)
/// - `threadName`: thread directory name (for logging)
/// - `incomingMessageDir`: message subdirectory name (to find received.md)
/// - `uid`: channel-specific message ID
/// - `_nonce`: integrity nonce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyContext {
    /// Config channel name (e.g., "jiny283") — routing key
    pub channel: String,
    /// Thread directory name (e.g., "weather") — for logging
    #[serde(rename = "threadName")]
    pub thread_name: String,
    /// Message subdirectory under messages/ (e.g., "2026-03-27_10-00-00")
    #[serde(rename = "incomingMessageDir")]
    pub incoming_message_dir: String,
    /// Channel-specific message ID (e.g., IMAP UID)
    pub uid: String,
    /// Integrity nonce
    #[serde(rename = "_nonce")]
    pub nonce: Option<String>,
}

/// Serialize a reply context token (struct → JSON → base64).
///
/// Uses standard base64 (with padding) to match jiny-m's format.
pub fn serialize_context(
    channel: &str,
    thread_name: &str,
    incoming_message_dir: &str,
    uid: &str,
) -> String {
    let nonce = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    let context = ReplyContext {
        channel: channel.to_string(),
        thread_name: thread_name.to_string(),
        incoming_message_dir: incoming_message_dir.to_string(),
        uid: uid.to_string(),
        nonce: Some(nonce),
    };

    let json = serde_json::to_string(&context).unwrap_or_default();
    base64::engine::general_purpose::STANDARD.encode(json)
}

/// Deserialize and validate a reply context token.
///
/// base64 → JSON → ReplyContext with integrity checks.
pub fn deserialize_context(encoded: &str) -> Result<ReplyContext> {
    // Try standard base64 first, then URL-safe (backward compat)
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded))
        .map_err(|e| anyhow::anyhow!("invalid base64 token: {e}"))?;

    let json =
        String::from_utf8(bytes).map_err(|e| anyhow::anyhow!("invalid UTF-8 in token: {e}"))?;

    // Check for tampering indicators
    if json.contains('`') || json.contains("\\n") || json.contains("\\\"") {
        bail!("token appears modified — DO NOT decode or modify the token");
    }

    let ctx: ReplyContext =
        serde_json::from_str(&json).map_err(|e| anyhow::anyhow!("invalid JSON in token: {e}"))?;

    // Validate required fields
    if ctx.channel.is_empty() {
        bail!("missing required field: channel");
    }
    if ctx.incoming_message_dir.is_empty() {
        bail!("missing required field: incomingMessageDir");
    }

    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_round_trip() {
        let token = serialize_context("jiny283", "weather", "2026-03-27_10-00-00", "42");
        let ctx = deserialize_context(&token).unwrap();
        assert_eq!(ctx.channel, "jiny283");
        assert_eq!(ctx.thread_name, "weather");
        assert_eq!(ctx.incoming_message_dir, "2026-03-27_10-00-00");
        assert_eq!(ctx.uid, "42");
        assert!(ctx.nonce.is_some());
    }

    #[test]
    fn test_deserialize_missing_channel() {
        let json = r#"{"channel":"","threadName":"t","incomingMessageDir":"d","uid":"1"}"#;
        let token = base64::engine::general_purpose::STANDARD.encode(json);
        assert!(deserialize_context(&token).is_err());
    }

    #[test]
    fn test_deserialize_missing_message_dir() {
        let json = r#"{"channel":"ch","threadName":"t","incomingMessageDir":"","uid":"1"}"#;
        let token = base64::engine::general_purpose::STANDARD.encode(json);
        assert!(deserialize_context(&token).is_err());
    }

    #[test]
    fn test_deserialize_invalid_base64() {
        assert!(deserialize_context("not-valid!!!").is_err());
    }

    #[test]
    fn test_deserialize_tampered_backticks() {
        let json = r#"`{"channel":"ch"}`"#;
        let token = base64::engine::general_purpose::STANDARD.encode(json);
        assert!(deserialize_context(&token).is_err());
    }

    #[test]
    fn test_minimal_token_is_short() {
        let token = serialize_context("jiny283", "weather", "2026-03-27_10-00-00", "42");
        // Minimal token should be well under 200 chars
        assert!(token.len() < 200, "token too long: {} chars", token.len());
    }
}
