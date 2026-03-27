use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::channels::email::outbound::EmailOutboundAdapter;
use crate::channels::types::{AttachmentConfig, InboundMessage, PatternMatch};
use crate::config::types::AgentConfig;
use crate::core::email_parser;
use crate::core::message_storage::{MessageStorage, StoreResult};
use crate::services::opencode::service::OpenCodeService;

/// An item in a thread's message queue.
struct QueueItem {
    message: InboundMessage,
    pattern_match: PatternMatch,
    attachment_config: Option<AttachmentConfig>,
}

/// Per-thread queue stats.
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub active_workers: usize,
    pub total_threads: usize,
    pub pending_messages: usize,
}

/// Manages per-thread message queues with bounded concurrency.
///
/// Responsible for: queue management, concurrency control, dispatching to the right agent mode.
/// NOT responsible for: AI logic, sessions, prompts — those live in `OpenCodeService`.
pub struct ThreadManager {
    thread_queues: Mutex<HashMap<String, mpsc::Sender<QueueItem>>>,
    semaphore: Arc<Semaphore>,
    max_queue_size: usize,

    // Shared dependencies
    storage: Arc<MessageStorage>,
    outbound: Arc<EmailOutboundAdapter>,
    agent_config: Arc<AgentConfig>,
    opencode_service: Arc<OpenCodeService>,

    cancel: CancellationToken,
    worker_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl ThreadManager {
    pub fn new(
        max_concurrent: usize,
        max_queue_size: usize,
        storage: Arc<MessageStorage>,
        outbound: Arc<EmailOutboundAdapter>,
        agent_config: Arc<AgentConfig>,
        opencode_service: Arc<OpenCodeService>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            thread_queues: Mutex::new(HashMap::new()),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_queue_size,
            storage,
            outbound,
            agent_config,
            opencode_service,
            cancel,
            worker_handles: Mutex::new(Vec::new()),
        }
    }

    /// Enqueue a message for processing in the given thread.
    pub async fn enqueue(
        &self,
        message: InboundMessage,
        thread_name: String,
        pattern_match: PatternMatch,
        attachment_config: Option<AttachmentConfig>,
    ) {
        let mut queues = self.thread_queues.lock().await;

        let item = QueueItem {
            message,
            pattern_match,
            attachment_config,
        };

        if let Some(sender) = queues.get(&thread_name) {
            match sender.try_send(item) {
                Ok(()) => {
                    tracing::debug!(thread = %thread_name, "Message enqueued");
                    return;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(thread = %thread_name, "Queue full, dropping message");
                    return;
                }
                Err(mpsc::error::TrySendError::Closed(item)) => {
                    queues.remove(&thread_name);
                    self.create_and_enqueue(&mut queues, thread_name, item).await;
                    return;
                }
            }
        }

        self.create_and_enqueue(&mut queues, thread_name, item).await;
    }

    async fn create_and_enqueue(
        &self,
        queues: &mut HashMap<String, mpsc::Sender<QueueItem>>,
        thread_name: String,
        item: QueueItem,
    ) {
        let (tx, rx) = mpsc::channel(self.max_queue_size);
        let _ = tx.try_send(item);
        queues.insert(thread_name.clone(), tx);

        let handle = self.spawn_worker(thread_name, rx);
        self.worker_handles.lock().await.push(handle);
    }

    fn spawn_worker(
        &self,
        thread_name: String,
        mut rx: mpsc::Receiver<QueueItem>,
    ) -> JoinHandle<()> {
        let semaphore = self.semaphore.clone();
        let cancel = self.cancel.clone();
        let storage = self.storage.clone();
        let outbound = self.outbound.clone();
        let agent_config = self.agent_config.clone();
        let opencode_service = self.opencode_service.clone();

        tokio::spawn(async move {
            let _permit = tokio::select! {
                permit = semaphore.acquire_owned() => match permit {
                    Ok(p) => p,
                    Err(_) => return,
                },
                _ = cancel.cancelled() => return,
            };

            tracing::info!(thread = %thread_name, "Worker started");

            loop {
                let item = tokio::select! {
                    item = rx.recv() => match item {
                        Some(item) => item,
                        None => break,
                    },
                    _ = cancel.cancelled() => {
                        tracing::info!(thread = %thread_name, "Worker cancelled");
                        break;
                    }
                };

                if let Err(e) = process_message(
                    &item,
                    &thread_name,
                    &storage,
                    &outbound,
                    &agent_config,
                    &opencode_service,
                ).await {
                    tracing::error!(
                        thread = %thread_name,
                        error = %e,
                        "Failed to process message"
                    );
                }
            }

            tracing::info!(thread = %thread_name, "Worker finished");
        })
    }

    pub async fn get_stats(&self) -> QueueStats {
        let queues = self.thread_queues.lock().await;
        let total_threads = queues.len();
        let active_workers = self.semaphore.available_permits();
        QueueStats {
            active_workers: total_threads.saturating_sub(active_workers),
            total_threads,
            pending_messages: 0,
        }
    }

    pub async fn shutdown(&self) {
        self.cancel.cancel();
        {
            let mut queues = self.thread_queues.lock().await;
            queues.clear();
        }
        let mut handles = self.worker_handles.lock().await;
        for handle in handles.drain(..) {
            let _ = handle.await;
        }
        tracing::info!("All workers shut down");
    }
}

/// Process a single message within a worker.
///
/// 1. Store the inbound message
/// 2. Dispatch to the configured agent mode (static / opencode)
/// 3. If agent returns fallback text → send via outbound + store reply
async fn process_message(
    item: &QueueItem,
    thread_name: &str,
    storage: &MessageStorage,
    outbound: &EmailOutboundAdapter,
    agent_config: &AgentConfig,
    opencode_service: &OpenCodeService,
) -> Result<()> {
    let message = &item.message;

    // 1. Store the inbound message
    let store_result: StoreResult = storage
        .store(message, thread_name, item.attachment_config.as_ref())
        .await?;

    tracing::info!(
        thread = %thread_name,
        message_dir = %store_result.message_dir,
        sender = %message.sender_address,
        topic = %message.topic,
        "Message stored, processing..."
    );

    if !agent_config.enabled {
        tracing::info!(thread = %thread_name, "Agent disabled, skipping reply");
        return Ok(());
    }

    // 2. Dispatch to agent mode
    match agent_config.mode.as_str() {
        "static" => {
            let reply_text = agent_config
                .text
                .as_deref()
                .unwrap_or("Thank you for your message. We will get back to you soon.");

            let body_text = message
                .content
                .text
                .as_deref()
                .or(message.content.markdown.as_deref())
                .unwrap_or("");

            let full_reply = email_parser::build_full_reply_text(
                reply_text,
                &store_result.thread_path,
                &message.sender,
                &message.timestamp.to_rfc3339(),
                &message.topic,
                body_text,
                &store_result.message_dir,
            )
            .await;

            outbound.send_reply(message, &full_reply, None).await?;
            storage
                .store_reply(&store_result.thread_path, &full_reply, &store_result.message_dir)
                .await?;

            tracing::info!(thread = %thread_name, "Static reply sent");
        }

        "opencode" => {
            let result = opencode_service
                .generate_reply(
                    message,
                    thread_name,
                    &store_result.thread_path,
                    &store_result.message_dir,
                )
                .await?;

            // 3. If tool didn't send the reply, do fallback send
            if !result.reply_sent_by_tool {
                if let Some(ref text) = result.reply_text {
                    tracing::info!(
                        thread = %thread_name,
                        text_len = text.len(),
                        "Fallback: building full reply with quoted history"
                    );

                    let body_text = message
                        .content
                        .text
                        .as_deref()
                        .or(message.content.markdown.as_deref())
                        .unwrap_or("");

                    let full_reply = email_parser::build_full_reply_text(
                        text,
                        &store_result.thread_path,
                        &message.sender,
                        &message.timestamp.to_rfc3339(),
                        &message.topic,
                        body_text,
                        &store_result.message_dir,
                    )
                    .await;

                    outbound.send_reply(message, &full_reply, None).await?;
                    storage
                        .store_reply(
                            &store_result.thread_path,
                            &full_reply,
                            &store_result.message_dir,
                        )
                        .await?;

                    tracing::info!(thread = %thread_name, "Fallback reply sent");
                } else {
                    tracing::warn!(
                        thread = %thread_name,
                        "No reply text from AI, skipping fallback send"
                    );
                }
            }
        }

        other => {
            tracing::warn!(thread = %thread_name, mode = %other, "Unknown agent mode");
        }
    }

    Ok(())
}
