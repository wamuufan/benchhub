pub mod engine_callbacks;
pub mod log_callbacks;
pub mod state_callbacks;
pub mod ui_callbacks;

use crate::state::AppState;
use crate::AppWindow;

/// Wires together all callback submodules for Slint UI events,
/// engine interactions, log handling, and application state updates.
pub fn register_callbacks(ui: &AppWindow, app_state: AppState) {
    ui_callbacks::register_ui_callbacks(ui, &app_state);
    engine_callbacks::register_engine_callbacks(ui, &app_state);
    log_callbacks::register_log_callbacks(ui, &app_state);
    state_callbacks::register_state_callbacks(ui, &app_state);
}
