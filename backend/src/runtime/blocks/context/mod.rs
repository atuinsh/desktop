//! Context blocks for modifying execution context
//!
//! Context blocks are special blocks that modify the execution environment
//! for subsequent blocks in a runbook. They don't execute commands themselves,
//! but instead set up the context (working directory, environment variables, SSH connections, etc.)
//!
//! ## Adding New Context Blocks
//!
//! To add a new context block:
//!
//! 1. Create a new directory under `context/` (e.g., `context/my_block/`)
//! 2. Create `mod.rs` with:
//!    - A struct for your block data (with `TypedBuilder`, `Serialize`, `Deserialize`)
//!    - A handler struct implementing `ContextProvider`
//!    - A `from_document` method for JSON parsing
//!    - Comprehensive tests covering edge cases
//! 3. Add your module to this file's exports
//! 4. Update the main `Block` enum to include your new context block
//!
//! ## Example Structure
//!
//! ```rust
//! // context/my_block/mod.rs
//! use serde::{Deserialize, Serialize};
//! use typed_builder::TypedBuilder;
//! use uuid::Uuid;
//! use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
//!
//! #[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
//! #[serde(rename_all = "camelCase")]
//! pub struct MyBlock {
//!     #[builder(setter(into))]
//!     pub id: Uuid,
//!     
//!     #[builder(setter(into))]
//!     pub my_field: String,
//! }
//!
//! pub struct MyBlockHandler;
//!
//! impl ContextProvider for MyBlockHandler {
//!     type Block = MyBlock;
//!
//!     fn block_type(&self) -> &'static str {
//!         "my-block"
//!     }
//!
//!     fn apply_context(
//!         &self,
//!         block: &MyBlock,
//!         context: &mut ExecutionContext,
//!     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         // Modify context based on block data
//!         Ok(())
//!     }
//! }
//!
//! // Add comprehensive tests...
//! ```

pub mod directory;
pub mod environment;
pub mod host;
pub mod local_var;
pub mod ssh_connect;
pub mod var;

// Re-export all context blocks and handlers for easy access
#[allow(unused_imports)] // Available for external use
pub use directory::DirectoryHandler;
#[allow(unused_imports)] // Available for external use
pub use environment::EnvironmentHandler;
#[allow(unused_imports)] // Available for external use
pub use host::HostHandler;
#[allow(unused_imports)] // Available for external use
pub use local_var::LocalVarHandler;
#[allow(unused_imports)] // Available for external use
pub use ssh_connect::SshConnectHandler;
#[allow(unused_imports)] // Available for external use
pub use var::VarHandler;

// Re-export block types for external use when needed
#[allow(unused_imports)] // Available for external use
pub use directory::Directory;
#[allow(unused_imports)] // Available for external use
pub use environment::Environment;
#[allow(unused_imports)] // Available for external use
pub use host::Host;
#[allow(unused_imports)] // Available for external use
pub use local_var::LocalVar;
#[allow(unused_imports)] // Available for external use
pub use ssh_connect::SshConnect;
#[allow(unused_imports)] // Available for external use
pub use var::Var;
