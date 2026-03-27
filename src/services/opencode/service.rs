use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::client::{OpenCodeClient, SseResult};
use super::types::*;
use super::{session, prompt_builder, OpenCodeServer};
use crate::channels::types::InboundMessage;
use crate::config::types::AgentConfig;

/// Result of AI reply generation.
///
/// The caller (ThreadManager) uses this to decide whether to send a fallback reply.
#[derive(Debug)]
pub struct GenerateReplyResult {
    /// Whether the reply was already sent by the MCP tool
    pub reply_sent_by_tool: bool,
    /// Accumulated text from the AI (for fallback direct send if tool wasn't used)
    pub reply_text: Option<String>,
    /// Model used for generation
    pub model_id: Option<String>,
    /// Provider used
    pub provider_id: Option<String>,
}

/// Encapsulates all OpenCode AI interaction logic.
///
/// Owns: server lifecycle, sessions, prompts, SSE streaming, error recovery.
/// Does NOT own: message storage, outbound sending — those stay in the thread manager.
pub struct OpenCodeService {
    server: Arc<OpenCodeServer>,
    agent_config: Arc<AgentConfig>,
    workdir: PathBuf,
}

impl OpenCodeService {
    pub fn new(
        server: Arc<OpenCodeServer>,
        agent_config: Arc<AgentConfig>,
        workdir: PathBuf,
    ) -> Self {
        Self {
            server,
            agent_config,
            workdir,
        }
    }

    /// Generate a reply for an inbound message.
    ///
    /// Handles the full lifecycle:
    /// 1. Ensure OpenCode server is running
    /// 2. Setup per-thread opencode.json
    /// 3. Get or create session
    /// 4. Build prompts (system + user + reply_context)
    /// 5. Send via SSE streaming (with timeout, tool detection, error recovery)
    /// 6. Return result for the caller to handle sending
    pub async fn generate_reply(
        &self,
        message: &InboundMessage,
        thread_name: &str,
        thread_path: &Path,
        message_dir: &str,
    ) -> Result<GenerateReplyResult> {
        // 1. Ensure OpenCode server is running
        let base_url = self.server.base_url().await?;
        let client = OpenCodeClient::new(&base_url);

        // 2. Ensure thread has opencode.json (and detect config changes)
        let config_changed = session::ensure_thread_opencode_setup(
            thread_path,
            &self.agent_config,
            &self.workdir,
        ).await?;

        if config_changed {
            tracing::info!(thread = %thread_name, "opencode.json changed");
        }

        // 3. Get or create session
        // Always create a fresh session to avoid stale session issues
        // across server restarts. The session ID is cheap to create.
        session::delete_session(thread_path).await?;
        let session_id = session::create_new_session(&client, thread_path).await?;

        // 4. Clean up stale signal file
        session::cleanup_signal_file(thread_path).await;

        // 5. Build prompts
        let include_history = self.agent_config
            .opencode
            .as_ref()
            .map(|o| o.include_thread_history)
            .unwrap_or(true);

        let system_prompt = prompt_builder::build_system_prompt(
            thread_path,
            self.agent_config.opencode.as_ref().and_then(|o| o.system_prompt.as_deref()),
        ).await;

        let user_prompt = prompt_builder::build_prompt(
            message,
            thread_path,
            message_dir,
            include_history,
        ).await?;

        // 6. Check for mode override (plan/build)
        let mode_override = session::read_mode_override(thread_path).await;
        let agent_mode = if mode_override.as_deref() == Some("plan") {
            Some("plan".to_string())
        } else {
            None
        };

        let mode_label = agent_mode.as_deref().unwrap_or("build").to_string();

        let request = PromptRequest {
            system: system_prompt,
            agent: agent_mode,
            parts: vec![PromptPart::Text { text: user_prompt }],
        };

        // 7. Send prompt via SSE streaming
        tracing::info!(
            thread = %thread_name,
            session_id = %session_id,
            mode = %mode_label,
            "Sending prompt to OpenCode..."
        );

        let sse_result = client
            .prompt_with_sse(&session_id, thread_path, &request, None)
            .await;

        // 8. Handle result
        let result = match sse_result {
            Ok(result) => {
                self.handle_sse_result(
                    result,
                    thread_name,
                    thread_path,
                    &client,
                    &session_id,
                    &request,
                ).await?
            }
            Err(e) => {
                tracing::error!(
                    thread = %thread_name,
                    error = %e,
                    "SSE streaming failed, trying blocking fallback"
                );

                let blocking_result = client
                    .prompt_blocking(&session_id, thread_path, &request)
                    .await?;

                self.handle_blocking_result(
                    blocking_result,
                    thread_name,
                    thread_path,
                    &client,
                    &session_id,
                    &request,
                ).await?
            }
        };

        // 9. Update session timestamp
        session::update_session_timestamp(thread_path).await.ok();

        Ok(result)
    }

    /// Handle the result from SSE streaming.
    async fn handle_sse_result(
        &self,
        result: SseResult,
        thread_name: &str,
        thread_path: &Path,
        client: &OpenCodeClient,
        session_id: &str,
        request: &PromptRequest,
    ) -> Result<GenerateReplyResult> {
        // Check for ContextOverflow error
        if let Some(ref error) = result.error {
            if error.contains("ContextOverflow") {
                tracing::warn!(
                    thread = %thread_name,
                    "ContextOverflow — creating new session and retrying"
                );
                session::delete_session(thread_path).await?;
                let new_session_id = session::create_new_session(client, thread_path).await?;

                let retry_result = client
                    .prompt_blocking(&new_session_id, thread_path, request)
                    .await?;

                return self.handle_blocking_result(
                    retry_result, thread_name, thread_path,
                    client, &new_session_id, request,
                ).await;
            }
        }

        // Check if reply was sent by tool
        let reply_sent_by_tool = result.reply_sent_by_tool
            || session::check_signal_file(thread_path).await;

        if reply_sent_by_tool {
            tracing::info!(
                thread = %thread_name,
                model = ?result.model_id,
                "Reply sent by MCP tool"
            );
            return Ok(GenerateReplyResult {
                reply_sent_by_tool: true,
                reply_text: None,
                model_id: result.model_id,
                provider_id: result.provider_id,
            });
        }

        // Stale session detection
        let tool_reported_in_sse = result.parts.iter().any(|p| {
            p.part_type == "tool"
                && p.tool.as_deref().map(|t| t.contains("reply_message")).unwrap_or(false)
                && p.state.as_ref().is_some_and(|s| s.status == "completed")
        });

        if tool_reported_in_sse && !session::check_signal_file(thread_path).await {
            tracing::warn!(
                thread = %thread_name,
                "Stale session detected — tool reported success but signal file missing"
            );
            session::delete_session(thread_path).await?;
            let new_session_id = session::create_new_session(client, thread_path).await?;
            session::cleanup_signal_file(thread_path).await;

            let retry_result = client
                .prompt_with_sse(&new_session_id, thread_path, request, None)
                .await?;

            let retry_sent = retry_result.reply_sent_by_tool
                || session::check_signal_file(thread_path).await;

            if retry_sent {
                tracing::info!(thread = %thread_name, "Reply sent after stale session retry");
                return Ok(GenerateReplyResult {
                    reply_sent_by_tool: true,
                    reply_text: None,
                    model_id: retry_result.model_id,
                    provider_id: retry_result.provider_id,
                });
            }

            return Ok(GenerateReplyResult {
                reply_sent_by_tool: false,
                reply_text: extract_text_from_parts(&retry_result.parts),
                model_id: retry_result.model_id,
                provider_id: retry_result.provider_id,
            });
        }

        // Timeout check
        if result.timed_out {
            if session::check_signal_file(thread_path).await {
                tracing::info!(thread = %thread_name, "Reply sent (detected via signal file after timeout)");
                return Ok(GenerateReplyResult {
                    reply_sent_by_tool: true,
                    reply_text: None,
                    model_id: result.model_id,
                    provider_id: result.provider_id,
                });
            }
            tracing::error!(thread = %thread_name, "Timed out with no reply");
            return Ok(GenerateReplyResult {
                reply_sent_by_tool: false,
                reply_text: None,
                model_id: result.model_id,
                provider_id: result.provider_id,
            });
        }

        // Fallback: extract text from AI response
        Ok(GenerateReplyResult {
            reply_sent_by_tool: false,
            reply_text: extract_text_from_parts(&result.parts),
            model_id: result.model_id,
            provider_id: result.provider_id,
        })
    }

    /// Handle the result from a blocking prompt.
    async fn handle_blocking_result(
        &self,
        result: PromptResponse,
        thread_name: &str,
        thread_path: &Path,
        _client: &OpenCodeClient,
        _session_id: &str,
        _request: &PromptRequest,
    ) -> Result<GenerateReplyResult> {
        // Check for error
        if let Some(ref data) = result.data {
            if let Some(ref info) = data.info {
                if let Some(ref error) = info.error {
                    tracing::error!(
                        thread = %thread_name,
                        error = %error.name,
                        "Blocking prompt error"
                    );
                }
            }
        }

        // Check signal file
        if session::check_signal_file(thread_path).await {
            tracing::info!(thread = %thread_name, "Reply sent by tool (blocking mode)");
            return Ok(GenerateReplyResult {
                reply_sent_by_tool: true,
                reply_text: None,
                model_id: None,
                provider_id: None,
            });
        }

        // Extract text parts for fallback
        let parts = result
            .data
            .map(|d| d.parts)
            .unwrap_or_default();

        Ok(GenerateReplyResult {
            reply_sent_by_tool: false,
            reply_text: extract_text_from_parts(&parts),
            model_id: None,
            provider_id: None,
        })
    }
}

/// Extract text content from accumulated response parts.
/// Strips prompt echoes that the AI may include when the reply tool fails.
fn extract_text_from_parts(parts: &[ResponsePart]) -> Option<String> {
    let text: String = parts
        .iter()
        .filter(|p| p.part_type == "text")
        .filter_map(|p| p.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");

    let cleaned = strip_prompt_echo(&text);

    if cleaned.trim().is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Strip prompt artifacts that the AI may echo back when the reply tool fails.
///
/// The AI sometimes copies the prompt structure into its text output:
/// - `## Incoming Message` section
/// - `<reply_context>...</reply_context>` block
/// - `## Conversation history` section
///
/// We cut at the first occurrence of any of these markers.
fn strip_prompt_echo(text: &str) -> String {
    let markers = [
        "## Incoming Message",
        "<reply_context>",
        "## Conversation history",
    ];

    let mut end = text.len();
    for marker in &markers {
        if let Some(pos) = text.find(marker) {
            if pos < end {
                end = pos;
            }
        }
    }

    text[..end].trim().to_string()
}
