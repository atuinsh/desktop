//! Client communication and messaging interfaces
//!
//! This module provides abstractions for communicating with the desktop application
//! client, including:
//!
//! - Message channels for sending execution output and events
//! - Client prompts for user interaction during block execution
//! - Local value providers for accessing client-side data
//! - Runbook content loaders for loading sub-runbooks

mod bridge;
pub mod hub;
pub(crate) mod local;
mod message_channel;
mod runbook_loader;

pub use bridge::{
    ClientPrompt, ClientPromptResult, DocumentBridgeMessage, PromptIcon, PromptInput, PromptOption,
    PromptOptionColor, PromptOptionVariant,
};
pub use hub::{
    load_runbook_from_id, load_runbook_from_uri, HubClient, HubError, HubRunbook, HubSnapshot,
    HubSnapshotMeta, ParsedUri,
};
pub use local::LocalValueProvider;
pub use message_channel::MessageChannel;
pub use runbook_loader::{LoadedRunbook, RunbookContentLoader, RunbookLoadError, SubRunbookRef};

#[cfg(test)]
pub use runbook_loader::MemoryRunbookContentLoader;
