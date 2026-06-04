//! Terminal UI — Ratatui-based interactive client with
//! WebSocket connection, chat view, and input handling.

pub mod app;
pub mod chat_view;
pub mod input;
pub mod status;
pub mod theme;
pub mod ws_client;

pub use app::TuiApp;
