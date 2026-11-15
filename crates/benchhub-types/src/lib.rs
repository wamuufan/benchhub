//! BenchHub Shared Domain Types & Models
//!
//! Pure Serde models shared between native backend and WASM frontend
//! with zero OS, Tokio, or SQLite dependencies.

pub mod ipc;
pub mod models;
pub mod terminal;
pub mod theme;

pub use ipc::*;
pub use models::*;
pub use terminal::*;
pub use theme::*;
