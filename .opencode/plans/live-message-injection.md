# Feature Design: Live Message Injection

## Overview

When a user sends a second message to a thread while the AI is still processing the first message, the second message should be injected into the ongoing AI session — allowing the AI to adjust its work based on the new input. The user receives one combined reply.

## Current Behavior

```
Message 1 → enqueue → worker processes → AI session → reply 1
Message 2 → enqueue → waits in queue → worker processes → AI session → reply 2
```

Sequential FIFO. Two separate replies. User must wait for Message 1 to complete before Message 2 starts.

## Proposed Behavior

```
Message 1 → enqueue → worker processes → AI session starts
Message 2 → arrives while AI busy:
  → store received.md
  → process commands
  → inject into SAME AI session (via prompt_async)
  → AI adjusts work based on Message 2
  → update reply-context.json with Message 2's dir
  → user gets ONE combined reply

Message 2 → arrives AFTER AI finished:
  → normal sequential processing → separate reply
```

## OpenCode API Support

`POST /session/:id/prompt_async` can be called while a session is busy. OpenCode queues the message and the AI receives it as a follow-up in the same conversation. This is the same mechanism the OpenCode TUI uses when users type messages during AI processing.

## Architecture

### Component Responsibilities (following design principles)

| Component | Responsibility | Change |
|-----------|---------------|--------|
| **ThreadManager** | Queue management, message dispatch. Passes queue receiver to agent for live monitoring. | Pass `rx` to agent during processing |
| **AgentService** | AI interaction. Monitors queue for new messages during SSE streaming. Injects them into the session. | Accept `rx`, inject messages during SSE |
| **OpenCodeService** | Session management, SSE streaming. Sends additional prompts to busy sessions. | Accept `rx`, send prompt_async for injected messages |
| **SSE Client** | Event streaming. Adds queue monitoring to select loop. | New arm in tokio::select for queue messages |
| **OutboundAdapter** | Reply lifecycle (format + send + store). No change — receives final reply text. | No change |
| **MessageStorage** | Store received.md. Each injected message gets its own directory. | No change |
| **MCP Reply Tool** | Reply via MCP. Uses latest reply-context.json. | No change |
| **CommandRegistry** | Process commands from injected messages. | Called by ThreadManager for each injected message |

### Data Flow

```
┌─ ThreadManager ──────────────────────────────────────────────────────┐
│                                                                       │
│  Worker receives Message 1 from queue                                 │
│  │                                                                    │
│  ├── 1. STORE Message 1 → received.md                                │
│  ├── 2. COMMAND PROCESS → parse/execute/strip                        │
│  ├── 3. REPLY COMMAND RESULTS (if commands found)                    │
│  ├── 4. CHECK BODY → if empty stop                                   │
│  ├── 5. DISPATCH TO AGENT                                            │
│  │       Pass queue receiver (rx) to agent.process()                  │
│  │       │                                                            │
│  │  ┌─ AgentService (OpenCodeService) ──────────────────────────┐   │
│  │  │                                                            │   │
│  │  │  Save reply-context.json                                   │   │
│  │  │  Build prompt, send via SSE                                │   │
│  │  │                                                            │   │
│  │  │  SSE Loop:                                                 │   │
│  │  │  tokio::select! {                                          │   │
│  │  │    event = sse.next() => { handle SSE event }              │   │
│  │  │                                                            │   │
│  │  │    new_msg = rx.recv() => {                                │   │
│  │  │      // NEW: Message injection                             │   │
│  │  │      1. Store new_msg → received.md (new dir)              │   │
│  │  │      2. Process commands from new_msg                      │   │
│  │  │      3. Update reply-context.json (new message_dir)        │   │
│  │  │      4. Build injection prompt (new message body)          │   │
│  │  │      5. Send prompt_async to same session                  │   │
│  │  │      // AI receives it as follow-up in conversation        │   │
│  │  │    }                                                       │   │
│  │  │                                                            │   │
│  │  │    _ = check_interval.tick() => { timeout/progress }       │   │
│  │  │    _ = cancel.cancelled() => break                         │   │
│  │  │  }                                                         │   │
│  │  │                                                            │   │
│  │  │  Return AgentResult { reply_text, reply_sent_by_tool }     │   │
│  │  └────────────────────────────────────────────────────────────┘   │
│  │                                                                    │
│  ├── 6. HANDLE RESULT (same as before)                               │
│  │       If tool sent → done                                          │
│  │       If fallback → outbound.send_reply()                          │
│  │                                                                    │
│  └── Worker picks next message from queue                             │
└──────────────────────────────────────────────────────────────────────┘
```

### Injection Prompt Format

When Message 2 is injected, the prompt sent to the AI is just the message body — no metadata, no system prompt, no reply instructions:

```
## Follow-up Message

Please also add a chart to the PPT.
```

The AI already has the system prompt and reply instructions from the initial prompt. The follow-up is minimal — just the user's new content.

### Reply Context Update

When a message is injected:
- Update `.jyc/reply-context.json` with the new `incomingMessageDir`
- The reply tool (or fallback) will reference the LATEST message in the reply
- Previous messages' `received.md` files remain (for history)
- `reply.md` is only written in the LATEST message directory

### Command Processing for Injected Messages

If the injected message contains commands (e.g., `/model ark/deepseek-v3.2`):
1. Process commands first (same CommandRegistry flow)
2. Send command results as a direct reply (separate email)
3. If body remains after stripping → inject body into AI session
4. If body is empty → don't inject (command-only message during AI processing)

### API Changes

#### AgentService trait

```rust
#[async_trait]
pub trait AgentService: Send + Sync {
    async fn process(
        &self,
        message: &InboundMessage,
        thread_name: &str,
        thread_path: &Path,
        message_dir: &str,
        pending_rx: &mut mpsc::Receiver<QueueItem>,  // NEW
    ) -> Result<AgentResult>;
}
```

#### OpenCodeService

```rust
pub async fn generate_reply(
    &self,
    message: &InboundMessage,
    thread_name: &str,
    thread_path: &Path,
    message_dir: &str,
    pending_rx: &mut mpsc::Receiver<QueueItem>,  // NEW
) -> Result<GenerateReplyResult>
```

#### SSE Client

```rust
pub async fn prompt_with_sse(
    &self,
    session_id: &str,
    directory: &Path,
    request: &PromptRequest,
    mode_label: &str,
    pending_rx: &mut mpsc::Receiver<QueueItem>,  // NEW
) -> Result<SseResult>
```

### ThreadManager Changes

The worker loop passes `rx` through to `process_message`:

```rust
// Current:
process_message(&item, &thread_name, &storage, &outbound, agent.as_ref()).await

// Proposed:
process_message(&item, &thread_name, &storage, &outbound, agent.as_ref(), &mut rx).await
```

And `process_message` passes `rx` to `agent.process()`.

### Edge Cases

| Case | Behavior |
|------|----------|
| Command-only injected message (e.g., `/model reset`) | Process command, send result, don't inject into AI |
| Injected message with command + body | Process command, inject body into AI |
| Multiple injected messages | Each injected sequentially as they arrive |
| Message arrives right as AI finishes | Timing: if `session.idle` already received, message goes to next worker loop iteration (normal sequential processing) |
| Cancellation during injection | Worker cancelled → stop processing, don't inject |
| AI fails/errors during injection | Same error recovery as single message (ContextOverflow, stale session) |

### What Does NOT Change

- OutboundAdapter — still receives final reply text, builds footer + quoted history
- MCP Reply Tool — still reads reply-context.json from cwd
- MessageStorage — still stores received.md per message
- Signal file — still used for tool detection
- Session management — same session reuse logic
- IMAP Monitor — still delivers messages to thread queue

### Implementation Phases

1. **Phase A**: Pass `rx` through ThreadManager → AgentService → OpenCodeService → SSE Client
2. **Phase B**: Add `rx.recv()` arm to SSE select loop
3. **Phase C**: Implement injection logic (store, command process, prompt_async)
4. **Phase D**: Update reply-context.json on injection
5. **Phase E**: Test with real email flow

### StaticAgentService

The static agent returns immediately — no SSE loop to inject into. For static mode, injected messages are processed normally in the next worker loop iteration (no change needed). The `pending_rx` parameter is accepted but not used.

## References

- OpenCode Server API: https://opencode.ai/docs/server/
- `POST /session/:id/prompt_async` — async message to busy session
- `POST /session/:id/abort` — abort running session (confirms busy state concept)
- DESIGN.md: Responsibility Separation (ThreadManager vs AgentService vs OutboundAdapter)
