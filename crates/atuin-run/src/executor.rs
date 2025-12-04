use std::sync::Arc;

use atuin_desktop_runtime::{
    blocks::Block,
    client::DocumentBridgeMessage,
    context::ContextResolver,
    document::{DocumentError, DocumentHandle},
    execution::BlockLifecycleEvent,
    pty::PtyStoreHandle,
    ssh::SshPoolHandle,
};
use crossterm::terminal;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    runbooks::Runbook,
    runtime::{
        ChannelDocumentBridge, NullDocumentBridge, NullEventBus, TempNullContextStorage,
        TempNullLocalValueProvider,
    },
    ui::{Renderer, StreamingRenderer, TerminalViewport, ViewportManager},
};

type Result<T> = std::result::Result<T, ExecutorError>;

#[derive(thiserror::Error, Debug)]
pub enum ExecutorError {
    #[error("Generic execution error: {0}")]
    GenericError(String),

    #[error("Runtime reported a document error: {0}")]
    RuntimeDocumentError(#[from] DocumentError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Block {0} failed: {0} (exited with code {2:?})")]
    BlockFailed(Uuid, String, Option<i32>),

    #[error("Block {0} cancelled")]
    BlockCancelled(Uuid),

    #[error("Block {0} error: {1}")]
    BlockError(Uuid, String),
}

pub struct Executor {
    runbook: Runbook,
    document: Arc<DocumentHandle>,
    interactive: bool,
    pty_store: PtyStoreHandle,
    ssh_pool: SshPoolHandle,
    renderer: Box<dyn Renderer>,
}

impl Executor {
    pub fn new(runbook: Runbook, interactive: bool) -> Self {
        let document = DocumentHandle::new(
            runbook.id.to_string(),
            Arc::new(NullEventBus),
            Arc::new(NullDocumentBridge),
            Some(Box::new(TempNullLocalValueProvider)),
            Some(Box::new(TempNullContextStorage)),
        );

        // Choose renderer based on interactive mode
        let renderer: Box<dyn Renderer> = if interactive {
            Box::new(ViewportManager::new())
        } else {
            Box::new(StreamingRenderer::new())
        };

        Self {
            runbook,
            document,
            interactive,
            pty_store: PtyStoreHandle::new(),
            ssh_pool: SshPoolHandle::new(),
            renderer,
        }
    }

    // TODO: find variables / inputs that need setting
    pub async fn execute(&mut self) -> Result<()> {
        self.document
            .put_document(self.runbook.content.clone())
            .await?;

        let blocks = self.document.blocks().await?;
        for block in blocks {
            let (sender, receiver) = mpsc::channel(16);
            let document_bridge = Arc::new(ChannelDocumentBridge::new(sender));
            self.document.update_bridge_channel(document_bridge).await?;

            if let Err(e) = self.execute_block(block, receiver).await {
                println!("{e}");
                std::process::exit(1);
            }
        }

        Ok(())
    }

    async fn execute_block(
        &mut self,
        block: Block,
        mut receiver: mpsc::Receiver<DocumentBridgeMessage>,
    ) -> Result<()> {
        let title = self.get_viewport_title(&block);
        let is_terminal = matches!(block, Block::Terminal(_));

        let viewport_height = match block {
            Block::Terminal(_) => 15,
            Block::Script(_) => 8,
            _ => 2,
        };

        let viewport = if is_terminal {
            // Get terminal width for sizing the viewport
            let term_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
            self.renderer
                .add_terminal_block(title, viewport_height, term_width)?
        } else {
            self.renderer.add_block(title, viewport_height)?
        };

        self.do_execute_block(block, viewport, &mut receiver, is_terminal, viewport_height)
            .await?;

        Ok(())
    }

    async fn do_execute_block(
        &mut self,
        block: Block,
        viewport: usize,
        receiver: &mut mpsc::Receiver<DocumentBridgeMessage>,
        is_terminal: bool,
        viewport_height: usize,
    ) -> Result<()> {
        let block_clone = block.clone();
        let block_id = block.id();

        let context = self
            .document
            .create_execution_context(
                block_id,
                Some(self.ssh_pool.clone()),
                Some(self.pty_store.clone()),
                None,
            )
            .await?;

        let resolver = context.context_resolver.clone();

        let execution_handle = block
            .execute(context)
            .await
            .map_err(|e| ExecutorError::GenericError(e.to_string()))?;

        // Enable raw mode for terminal blocks to capture keyboard input (only in interactive mode)
        if is_terminal && self.interactive {
            crossterm::terminal::enable_raw_mode()?;
        }

        let mut full_output = String::new();

        // Calculate PTY dimensions if this is a terminal block
        let (pty_rows, pty_cols) = if is_terminal {
            let term_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
            let inner_width = term_width.saturating_sub(6); // padding
            let content_width = inner_width.saturating_sub(2); // borders
            (viewport_height as u16, content_width as u16)
        } else {
            (0, 0)
        };

        let mut terminal_viewport = if is_terminal {
            // Create terminal viewport with calculated width
            Some(TerminalViewport::new(pty_rows, pty_cols, viewport_height))
        } else {
            None
        };
        let mut last_visible_lines: Option<Vec<String>> = None;

        // Create keyboard input channel for terminal blocks (only in interactive mode)
        let (mut key_rx, keyboard_task_handle) = if is_terminal && self.interactive {
            let (tx, rx) = tokio::sync::mpsc::channel::<crossterm::event::Event>(32);

            // Spawn task to read keyboard events
            let handle = tokio::spawn(async move {
                loop {
                    // Check if channel is still open first
                    if tx.is_closed() {
                        break;
                    }

                    if crossterm::event::poll(std::time::Duration::from_millis(100))
                        .unwrap_or(false)
                    {
                        if let Ok(event) = crossterm::event::read() {
                            if tx.send(event).await.is_err() {
                                break; // Channel closed, exit
                            }
                        }
                    }
                }
            });

            (Some(rx), Some(handle))
        } else {
            (None, None)
        };

        let result = if let Some(_handle) = execution_handle {
            self.execute_block_with_io(
                block_id,
                viewport,
                receiver,
                &mut key_rx,
                &mut terminal_viewport,
                &mut last_visible_lines,
                &mut full_output,
                is_terminal,
                viewport_height,
            )
            .await
        } else {
            let lines = self.get_output_lines(block_clone, &resolver);
            for line in lines {
                self.renderer.add_line(viewport, &line)?;
            }
            self.renderer.mark_complete(viewport)?;
            Ok(())
        };

        // Clean up: abort the keyboard task if it exists
        if let Some(handle) = keyboard_task_handle {
            handle.abort();
        }

        // Always disable raw mode if it was enabled
        if is_terminal && self.interactive {
            let _ = crossterm::terminal::disable_raw_mode();
        }

        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_block_with_io(
        &mut self,
        block_id: uuid::Uuid,
        viewport: usize,
        receiver: &mut mpsc::Receiver<DocumentBridgeMessage>,
        key_rx: &mut Option<tokio::sync::mpsc::Receiver<crossterm::event::Event>>,
        terminal_viewport: &mut Option<TerminalViewport>,
        last_visible_lines: &mut Option<Vec<String>>,
        full_output: &mut String,
        is_terminal: bool,
        viewport_height: usize,
    ) -> Result<()> {
        use crossterm::event::Event;

        // Throttle viewport updates to ~30 FPS (33ms between updates)
        let mut viewport_update_ticker =
            tokio::time::interval(std::time::Duration::from_millis(33));
        let mut pending_viewport_update = false;

        loop {
            tokio::select! {
                // Periodic timer to flush pending viewport updates (throttled rendering)
                // Always poll the ticker so it advances at regular intervals
                _ = viewport_update_ticker.tick() => {
                    // Only update if we have pending changes and this is a terminal block
                    if pending_viewport_update && is_terminal {
                        if let Some(ref term_viewport) = terminal_viewport {
                            let lines = term_viewport.get_visible_lines();

                            // Only update if lines actually changed
                            let should_update = match &last_visible_lines {
                                None => true,
                                Some(prev_lines) => prev_lines != &lines,
                            };

                            if should_update {
                                self.renderer.replace_lines(viewport, lines.clone())?;
                                *last_visible_lines = Some(lines);
                            }
                        }

                        pending_viewport_update = false;
                    }
                }

                // Handle keyboard input for terminal blocks
                Some(event) = async {
                    match key_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Event::Key(key_event) = event {
                        let bytes = key_event_to_bytes(key_event);
                        if let Err(e) = self.pty_store.write_pty(block_id, bytes).await {
                            eprintln!("Failed to send keyboard input to PTY: {e}");
                        }
                    }
                }

                // Handle PTY output messages
                Some(message) = receiver.recv() => {
                if let DocumentBridgeMessage::BlockOutput { output, .. } = message {
                    // Handle PTY metadata message - resize PTY when it's created
                    if is_terminal {
                        if let Some(ref obj) = output.object {
                            if obj.get("pty").is_some() {
                                let term_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
                                let inner_width = term_width.saturating_sub(6);
                                let content_width = inner_width.saturating_sub(2);
                                let resize_cols = content_width as u16;
                                let resize_rows = viewport_height as u16;

                                let _ = self.pty_store.resize_pty(block_id, resize_rows, resize_cols).await;
                            }
                        }
                    }

                    // Handle binary output for terminal blocks
                    if let Some(binary) = output.binary {
                        // Only process non-empty binary output
                        if !binary.is_empty() {
                            if let Some(ref mut term_viewport) = terminal_viewport {
                                // Always feed raw PTY bytes to terminal parser (keep internal state up-to-date)
                                term_viewport.process_output(&binary);

                                // Mark that we have pending changes to render
                                pending_viewport_update = true;
                            }
                        }
                    }

                    // Handle stdout/stderr output for non-terminal blocks
                    if let Some(stdout) = output.stdout {
                        full_output.push_str(&stdout);
                        if terminal_viewport.is_none() {
                            self.renderer
                                .add_line(viewport, stdout.trim_end())?;
                        }
                    }
                    if let Some(stderr) = output.stderr {
                        full_output.push_str(&stderr);
                        if terminal_viewport.is_none() {
                            self.renderer
                                .add_line(viewport, stderr.trim_end())?;
                        }
                    }

                    // Handle lifecycle events - only break on these
                    if let Some(lifecycle) = output.lifecycle {
                        match lifecycle {
                            BlockLifecycleEvent::Started(_) => {}
                            BlockLifecycleEvent::Finished(data) => {
                                // Flush any pending viewport updates before completing
                                if pending_viewport_update && is_terminal {
                                    if let Some(ref term_viewport) = terminal_viewport {
                                        let lines = term_viewport.get_visible_lines();
                                        if last_visible_lines.as_ref() != Some(&lines) {
                                            self.renderer.replace_lines(viewport, lines.clone())?;
                                            *last_visible_lines = Some(lines);
                                        }
                                    }
                                }

                                if let Some(exit_code) = data.exit_code {
                                    if exit_code == 0 {
                                        self.renderer.mark_complete(viewport)?;
                                        // Drop keyboard receiver to unblock the spawned task
                                        break;
                                    } else {
                                        return Err(ExecutorError::BlockFailed(block_id, full_output.clone(), Some(exit_code)));
                                    }
                                } else {
                                    self.renderer.mark_complete(viewport)?;
                                    // Drop keyboard receiver to unblock the spawned task
                                    break;
                                }
                            }
                            BlockLifecycleEvent::Cancelled => {
                                // Flush any pending viewport updates before cancelling
                                if pending_viewport_update && is_terminal {
                                    if let Some(ref term_viewport) = terminal_viewport {
                                        let lines = term_viewport.get_visible_lines();
                                        if last_visible_lines.as_ref() != Some(&lines) {
                                            let _ = self.renderer.replace_lines(viewport, lines.clone());
                                            *last_visible_lines = Some(lines);
                                        }
                                    }
                                }
                                return Err(ExecutorError::BlockCancelled(block_id));
                            }
                            BlockLifecycleEvent::Error(data) => {
                                // Flush any pending viewport updates before erroring
                                if pending_viewport_update && is_terminal {
                                    if let Some(ref term_viewport) = terminal_viewport {
                                        let lines = term_viewport.get_visible_lines();
                                        if last_visible_lines.as_ref() != Some(&lines) {
                                            let _ = self.renderer.replace_lines(viewport, lines.clone());
                                            *last_visible_lines = Some(lines);
                                        }
                                    }
                                }
                                return Err(ExecutorError::BlockError(block_id, data.message));
                            }
                        }
                    }
                }
                }
            }
        }

        Ok(())
    }

    fn get_output_lines(&self, block: Block, resolver: &ContextResolver) -> Vec<String> {
        match block {
            Block::Directory(dir) => {
                tracing::debug!("dir block: {dir:?}");
                vec![format!(
                    "Directory set to: {}",
                    resolver.resolve_template(&dir.path).unwrap_or_default()
                )]
            }
            Block::Var(var) => {
                vec![format!(
                    "Variable {}: {}",
                    var.name,
                    resolver.resolve_template(&var.value).unwrap_or_default()
                )]
            }
            _ => vec![],
        }
    }

    fn get_viewport_title(&self, block: &Block) -> String {
        let name = block.name();
        if name.is_empty() {
            self.get_block_type(block)
        } else {
            format!("{}: {}", self.get_block_type(block), name)
        }
    }

    fn get_block_type(&self, block: &Block) -> String {
        match block {
            Block::Terminal(_) => "Terminal".to_string(),
            Block::Script(_) => "Script".to_string(),
            Block::SQLite(_) => "SQLite".to_string(),
            Block::Postgres(_) => "Postgres".to_string(),
            Block::Http(_) => "HTTP".to_string(),
            Block::Prometheus(_) => "Prometheus".to_string(),
            Block::Clickhouse(_) => "Clickhouse".to_string(),
            Block::Mysql(_) => "MySQL".to_string(),
            Block::Editor(_) => "Editor".to_string(),
            Block::LocalVar(_) => "Local variable".to_string(),
            Block::Var(_) => "Variable".to_string(),
            Block::Environment(_) => "Environment".to_string(),
            Block::Directory(_) => "Directory".to_string(),
            Block::LocalDirectory(_) => "Local directory".to_string(),
            Block::SshConnect(_) => "SSH connect".to_string(),
            Block::Host(_) => "Host".to_string(),
            Block::VarDisplay(_) => "Variable display".to_string(),
            Block::MarkdownRender(_) => "Markdown render".to_string(),
            Block::Kubernetes(_) => "Kubernetes".to_string(),
            Block::Dropdown(_) => "Dropdown".to_string(),
        }
    }
}

/// Convert crossterm KeyEvent to bytes for PTY input
fn key_event_to_bytes(key_event: crossterm::event::KeyEvent) -> bytes::Bytes {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut output = Vec::new();

    match key_event.code {
        KeyCode::Char(c) => {
            // Handle Ctrl+ combinations
            if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'a'..='z' => {
                        // Ctrl+A = 0x01, Ctrl+B = 0x02, ..., Ctrl+Z = 0x1A
                        output.push((c as u8) - b'a' + 1);
                    }
                    'A'..='Z' => {
                        output.push((c as u8) - b'A' + 1);
                    }
                    _ => {
                        output.push(c as u8);
                    }
                }
            } else {
                // Regular character
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                output.extend_from_slice(s.as_bytes());
            }
        }
        KeyCode::Enter => output.extend_from_slice(b"\r"),
        KeyCode::Backspace => output.push(0x7F), // DEL character
        KeyCode::Tab => output.push(b'\t'),
        KeyCode::Esc => output.push(0x1B),
        KeyCode::Up => output.extend_from_slice(b"\x1b[A"),
        KeyCode::Down => output.extend_from_slice(b"\x1b[B"),
        KeyCode::Right => output.extend_from_slice(b"\x1b[C"),
        KeyCode::Left => output.extend_from_slice(b"\x1b[D"),
        KeyCode::Home => output.extend_from_slice(b"\x1b[H"),
        KeyCode::End => output.extend_from_slice(b"\x1b[F"),
        KeyCode::PageUp => output.extend_from_slice(b"\x1b[5~"),
        KeyCode::PageDown => output.extend_from_slice(b"\x1b[6~"),
        KeyCode::Delete => output.extend_from_slice(b"\x1b[3~"),
        KeyCode::Insert => output.extend_from_slice(b"\x1b[2~"),
        KeyCode::F(n) => {
            // F1-F12 escape sequences
            match n {
                1 => output.extend_from_slice(b"\x1bOP"),
                2 => output.extend_from_slice(b"\x1bOQ"),
                3 => output.extend_from_slice(b"\x1bOR"),
                4 => output.extend_from_slice(b"\x1bOS"),
                5 => output.extend_from_slice(b"\x1b[15~"),
                6 => output.extend_from_slice(b"\x1b[17~"),
                7 => output.extend_from_slice(b"\x1b[18~"),
                8 => output.extend_from_slice(b"\x1b[19~"),
                9 => output.extend_from_slice(b"\x1b[20~"),
                10 => output.extend_from_slice(b"\x1b[21~"),
                11 => output.extend_from_slice(b"\x1b[23~"),
                12 => output.extend_from_slice(b"\x1b[24~"),
                _ => {}
            }
        }
        _ => {}
    }

    bytes::Bytes::from(output)
}
