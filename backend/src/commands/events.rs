use async_trait::async_trait;
use tauri::{ipc::Channel, AppHandle, Manager};
use tokio::sync::mpsc;

use crate::runtime::events::{EventBus, GCEvent};
use crate::state::AtuinState;

/// Channel-based event bus implementation that forwards events to Tauri channels
pub struct ChannelEventBus {
    sender: mpsc::UnboundedSender<GCEvent>,
}

impl ChannelEventBus {
    pub fn new(sender: mpsc::UnboundedSender<GCEvent>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl EventBus for ChannelEventBus {
    async fn emit(&self, event: GCEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sender
            .send(event)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
}

/// Subscribe to the Grand Central event stream
#[tauri::command]
pub async fn subscribe_to_events(
    app_handle: AppHandle,
    event_channel: Channel<GCEvent>,
) -> Result<(), String> {
    let state = app_handle
        .try_state::<AtuinState>()
        .ok_or("State not available")?;

    // Get the event receiver from state
    let mut receiver = state
        .event_receiver
        .lock()
        .await
        .take()
        .ok_or("Event receiver already taken or not available")?;

    // Spawn task to forward events to the channel
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if let Err(e) = event_channel.send(event) {
                eprintln!("Failed to send event to frontend: {}", e);
                break;
            }
        }
    });

    Ok(())
}
