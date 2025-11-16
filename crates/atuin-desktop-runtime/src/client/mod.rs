//! Client communication and messaging interfaces
//!
//! This module provides abstractions for communicating with the desktop application
//! client, including:
//!
//! - Message channels for sending execution output and events
//! - Client prompts for user interaction during block execution
//! - Local value providers for accessing client-side data

mod bridge;
pub(crate) mod local;
mod message_channel;

pub use bridge::{
    ClientPrompt, ClientPromptResult, DocumentBridgeMessage, PromptIcon, PromptInput, PromptOption,
    PromptOptionColor, PromptOptionVariant,
};
pub use local::LocalValueProvider;
pub use message_channel::MessageChannel;
