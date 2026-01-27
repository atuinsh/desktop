# Inline Block Generation Implementation Plan

## Overview

Migrate inline block generation from server-based LLM calls to the local AI session infrastructure. This enables the AI to generate blocks based on context, with support for user-requested edits.

## User Flow

1. User presses Cmd/Ctrl+Enter on a text block (or Cmd/Ctrl+K to enter prompt manually)
2. Generator session is created, AI generates blocks via `submit_blocks` tool
3. Blocks are inserted into document, UI shows accept/cancel/edit options
4. User can:
   - **Accept (Tab)**: Blocks stay, session destroyed
   - **Cancel (Escape)**: Blocks removed, session destroyed
   - **Edit (E)**: User enters edit prompt, AI regenerates blocks

## Implementation Phases

### Phase 1: Backend - Tool & Event Infrastructure

#### 1.1 Add `submit_blocks` tool definition
**File:** `backend/src/ai/tools.rs`

```rust
pub fn submit_blocks() -> Tool {
    Tool::new("submit_blocks")
        .with_description(indoc! {"
            Submit the generated blocks to be inserted into the runbook.
            This tool should be called exactly once at the end of your response
            with all the blocks you want to insert. After calling this tool,
            the interaction ends unless the user requests edits.
        "})
        .with_schema(json!({
            "type": "object",
            "properties": {
                "blocks": {
                    "type": "array",
                    "description": "Array of block objects to insert. Each must have 'type' and 'props'.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string" },
                            "props": { "type": "object" }
                        },
                        "required": ["type", "props"]
                    }
                }
            },
            "required": ["blocks"]
        }))
}
```

#### 1.2 Add tool to InlineBlockGeneration session
**File:** `backend/src/ai/types.rs`

In `SessionKind::tools()`, uncomment/add submit_blocks:
```rust
SessionKind::InlineBlockGeneration { block_infos, .. } => {
    // ...
    vec![
        AITools::get_runboook_document(),
        AITools::get_block_docs(&block_types),
        AITools::get_default_shell(),
        AITools::submit_blocks(),  // Add this
    ]
}
```

#### 1.3 Add `BlocksGenerated` session event
**File:** `backend/src/ai/session.rs`

```rust
pub enum SessionEvent {
    // ... existing variants ...

    /// Blocks were generated via submit_blocks tool (for InlineBlockGeneration sessions)
    BlocksGenerated {
        blocks: Vec<serde_json::Value>,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
    },
}
```

#### 1.4 Intercept `submit_blocks` in effect execution
**File:** `backend/src/ai/session.rs`

In `execute_effect`, modify `Effect::ExecuteTools` handling:
```rust
Effect::ExecuteTools { calls } => {
    // Check for submit_blocks tool call
    if let Some(submit_call) = calls.iter().find(|c| c.fn_name == "submit_blocks") {
        if let Some(blocks) = submit_call.fn_arguments.get("blocks") {
            let blocks_vec = blocks.as_array().cloned().unwrap_or_default();
            let _ = self.output_tx.send(SessionEvent::BlocksGenerated {
                blocks: blocks_vec,
                tool_call_id: submit_call.call_id.clone(),
            }).await;
        }

        // Don't emit ToolsRequested for submit_blocks - frontend handles it specially
        // Filter out submit_blocks from the calls sent to frontend
        let other_calls: Vec<AIToolCall> = calls
            .into_iter()
            .filter(|c| c.fn_name != "submit_blocks")
            .map(|c| c.into())
            .collect();

        if !other_calls.is_empty() {
            let _ = self.output_tx.send(SessionEvent::ToolsRequested { calls: other_calls }).await;
        }

        // Save state when entering PendingTools
        self.save_if_persisted().await;
    } else {
        // Normal tool execution path
        let ai_calls: Vec<AIToolCall> = calls.into_iter().map(|c| c.into()).collect();
        let _ = self.output_tx.send(SessionEvent::ToolsRequested { calls: ai_calls }).await;
        self.save_if_persisted().await;
    }
}
```

### Phase 2: Backend - Edit Flow Support

#### 2.1 Add `UpdateSystemPrompt` FSM event
**File:** `backend/src/ai/fsm.rs`

```rust
pub enum Event {
    // ... existing variants ...

    /// Update the system prompt (replaces conversation[0])
    UpdateSystemPrompt(ChatMessage),
}
```

In `Agent::handle()`:
```rust
// Handle in any state - just updates the prompt
(_, Event::UpdateSystemPrompt(msg)) => {
    if !self.context.conversation.is_empty() {
        self.context.conversation[0] = msg;
    }
    Transition::none()
}
```

#### 2.2 Add `send_edit_request` to SessionHandle
**File:** `backend/src/ai/session.rs`

```rust
impl SessionHandle {
    /// Send an edit request for InlineBlockGeneration sessions.
    /// This updates the system prompt, responds to the pending submit_blocks call,
    /// and sends the user's edit prompt.
    pub async fn send_edit_request(
        &self,
        edit_prompt: String,
        tool_call_id: String,
    ) -> Result<(), AISessionError> {
        // 1. Update kind to is_initial_generation: false
        {
            let mut kind = self.kind.write().await;
            if let SessionKind::InlineBlockGeneration { is_initial_generation, .. } = &mut *kind {
                *is_initial_generation = false;
            }
        }

        // 2. Generate new system prompt and send UpdateSystemPrompt event
        let new_system_prompt = {
            let kind = self.kind.read().await;
            kind.system_prompt().map_err(AISessionError::SystemPromptError)?
        };
        let mut system_msg = ChatMessage::system(new_system_prompt);
        system_msg.options = Some(MessageOptions {
            cache_control: Some(genai::chat::CacheControl::Ephemeral),
        });
        self.event_tx
            .send(Event::UpdateSystemPrompt(system_msg))
            .await
            .map_err(|_| AISessionError::ChannelClosed)?;

        // 3. Send tool result for submit_blocks (success - blocks were shown to user)
        self.event_tx
            .send(Event::ToolResult(ToolResult {
                call_id: tool_call_id,
                output: ToolOutput::Success("Blocks shown to user. User has requested changes.".to_string()),
            }))
            .await
            .map_err(|_| AISessionError::ChannelClosed)?;

        // 4. Send user message with edit prompt
        let user_msg = ChatMessage::user(edit_prompt);
        self.event_tx
            .send(Event::UserMessage(user_msg))
            .await
            .map_err(|_| AISessionError::ChannelClosed)?;

        Ok(())
    }
}
```

#### 2.3 Add Tauri command for edit request
**File:** `backend/src/commands/ai.rs` (or wherever AI commands live)

```rust
#[tauri::command]
pub async fn ai_send_edit_request(
    session_id: Uuid,
    edit_prompt: String,
    tool_call_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let handle = state.ai_manager
        .get_handle(session_id)
        .await
        .ok_or("Session not found")?;

    handle.send_edit_request(edit_prompt, tool_call_id)
        .await
        .map_err(|e| e.to_string())
}
```

### Phase 3: Frontend - Session Integration

#### 3.1 Update useAIInlineGeneration hook state
**File:** `src/components/runbooks/editor/hooks/useAIInlineGeneration.ts`

Add new state fields:
```typescript
export type InlineGenerationState =
  | { status: "idle" }
  | { status: "generating"; promptBlockId: string; originalPrompt: string; sessionId: string }
  | { status: "cancelled" }
  | { status: "postGeneration"; generatedBlockIds: string[]; sessionId: string; toolCallId: string }
  | { status: "editing"; generatedBlockIds: string[]; editPrompt: string; sessionId: string; toolCallId: string }
  | { status: "submittingEdit"; generatedBlockIds: string[]; editPrompt: string; sessionId: string; toolCallId: string };
```

#### 3.2 Create generator session on Cmd/Ctrl+Enter
**File:** `src/components/runbooks/editor/hooks/useAIInlineGeneration.ts`

Replace `generateBlocks()` call with session creation:
```typescript
const handleInlineGenerate = useCallback(async (block: any) => {
  const prompt = getBlockText(block);
  if (!prompt.trim() || !editor) return;

  // Cancel any existing session
  if (stateRef.current.status !== "idle" && "sessionId" in stateRef.current) {
    await invoke("ai_destroy_session", { sessionId: stateRef.current.sessionId });
  }

  dispatch({ type: "START_GENERATE", promptBlockId: block.id, originalPrompt: prompt });

  try {
    // Get context
    const context = await getEditorContext();
    const document = editor.document;

    // Create generator session
    const sessionId = await invoke<string>("ai_create_generator_session", {
      runbookId,
      blockInfos: getBlockInfos(), // Need to implement or get from context
      currentDocument: document,
      insertAfter: block.id,
      config: {
        model: getModelSelection(),
        desktopUsername: username,
        chargeTarget,
      },
    });

    // Update state with session ID
    dispatch({ type: "SESSION_CREATED", sessionId });

    // Subscribe to session events
    const channel = new Channel<SessionEvent>();
    channel.onmessage = (event) => handleSessionEvent(event, block.id);
    await invoke("ai_subscribe_session", { sessionId, channel });

    // Send the prompt as user message
    await invoke("ai_send_message", { sessionId, content: prompt });

  } catch (error) {
    dispatch({ type: "GENERATION_ERROR" });
    // Show error toast...
  }
}, [editor, runbookId, username, chargeTarget, getEditorContext]);
```

#### 3.3 Handle session events
**File:** `src/components/runbooks/editor/hooks/useAIInlineGeneration.ts`

```typescript
const handleSessionEvent = useCallback((event: SessionEvent, promptBlockId: string) => {
  switch (event.type) {
    case "blocksGenerated":
      // Insert blocks into editor
      const insertedIds = insertBlocks(event.blocks, promptBlockId);
      dispatch({
        type: "GENERATION_SUCCESS",
        generatedBlockIds: insertedIds,
        toolCallId: event.toolCallId,
      });
      break;

    case "error":
      dispatch({ type: "GENERATION_ERROR" });
      // Show error toast
      break;

    case "cancelled":
      dispatch({ type: "GENERATION_CANCELLED" });
      break;

    // ... handle other events as needed
  }
}, [editor]);
```

#### 3.4 Handle edit submission
**File:** `src/components/runbooks/editor/hooks/useAIInlineGeneration.ts`

```typescript
const handleEditSubmit = useCallback(async () => {
  const currentState = stateRef.current;
  if (currentState.status !== "editing" || !currentState.editPrompt.trim()) return;

  const { sessionId, toolCallId, editPrompt, generatedBlockIds } = currentState;
  dispatch({ type: "SUBMIT_EDIT" });

  try {
    // Remove old blocks before AI generates new ones
    editor.removeBlocks(generatedBlockIds);

    // Send edit request to session
    await invoke("ai_send_edit_request", {
      sessionId,
      editPrompt,
      toolCallId,
    });

    // Session will emit new BlocksGenerated event
  } catch (error) {
    dispatch({ type: "EDIT_ERROR" });
    // Show error toast
  }
}, [editor]);
```

#### 3.5 Clean up on accept/cancel
**File:** `src/components/runbooks/editor/hooks/useAIInlineGeneration.ts`

```typescript
const handleAccept = useCallback(async () => {
  const currentState = stateRef.current;
  if (currentState.status !== "postGeneration") return;

  // Destroy session
  await invoke("ai_destroy_session", { sessionId: currentState.sessionId });
  dispatch({ type: "CLEAR" });
}, []);

const handleCancel = useCallback(async () => {
  const currentState = stateRef.current;
  if (currentState.status !== "postGeneration") return;

  // Remove blocks and destroy session
  editor.removeBlocks(currentState.generatedBlockIds);
  await invoke("ai_destroy_session", { sessionId: currentState.sessionId });
  dispatch({ type: "CLEAR" });
}, [editor]);
```

### Phase 4: Cleanup

#### 4.1 Remove stub code from block_generator.ts
**File:** `src/lib/ai/block_generator.ts`

Remove the early return with mock data (lines 40-46). Either:
- Remove the file entirely if unused
- Keep helper types/functions if useful elsewhere

#### 4.2 Remove or deprecate server API
**File:** `src/api/ai.ts`

Check if `generateOrEditBlock` is used elsewhere. If not, remove it along with related types:
- `AIBlockRequest`
- `AISingleBlockResponse`
- `AIMultiBlockResponse`
- `generateOrEditBlock()`

Keep error classes if they're reused:
- `AIFeatureDisabledError`
- `AIGenerationError`
- `AIQuotaExceededError`

### Phase 5: Testing & Polish

#### 5.1 Manual testing checklist
- [ ] Cmd/Ctrl+Enter triggers generation from text block
- [ ] Blocks are inserted after the prompt block
- [ ] Tab accepts and clears UI state
- [ ] Escape removes blocks and clears UI state
- [ ] E enters edit mode
- [ ] Edit prompt submission regenerates blocks
- [ ] Multiple edits work correctly
- [ ] Cancellation during generation works
- [ ] Error handling shows appropriate toasts
- [ ] Starting new generation while one is pending cancels the old one

#### 5.2 Edge cases to test
- [ ] Empty prompt block (should not trigger)
- [ ] Generation while document is being edited elsewhere
- [ ] Network errors during generation
- [ ] Very long prompts
- [ ] AI returns empty blocks array
- [ ] AI returns malformed blocks

## File Change Summary

| File | Type | Description |
|------|------|-------------|
| `backend/src/ai/tools.rs` | Modify | Add `submit_blocks()` tool |
| `backend/src/ai/types.rs` | Modify | Add submit_blocks to InlineBlockGeneration tools |
| `backend/src/ai/session.rs` | Modify | Add `BlocksGenerated` event, intercept submit_blocks, add `send_edit_request` |
| `backend/src/ai/fsm.rs` | Modify | Add `UpdateSystemPrompt` event |
| `backend/src/commands/ai.rs` | Modify | Add `ai_send_edit_request` command |
| `src/.../useAIInlineGeneration.ts` | Modify | Replace server API with session management |
| `src/lib/ai/block_generator.ts` | Remove/Modify | Remove stub code |
| `src/api/ai.ts` | Modify | Remove unused server API functions |

## Open Items / Future Considerations

1. **Cmd/Ctrl+K flow**: Same infrastructure, but need UI for entering prompt manually
2. **Block info retrieval**: Need to ensure `blockInfos` are available when creating generator session
3. **Model selection**: Need to get current model selection from settings/context
4. **Streaming feedback**: Currently no streaming for generator - consider showing "thinking" indicator
5. **Rate limiting**: Consider debouncing rapid Cmd/Ctrl+Enter presses
