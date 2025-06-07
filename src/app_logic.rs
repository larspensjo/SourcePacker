/*
 * This module provides the application logic layer, primarily centered around
 * `MyAppLogic` which acts as the Presenter/Controller. It also now includes
 * `MainWindowUiState` for managing UI-specific state for the main window.
 * Unit tests for `MyAppLogic` are in `handler_tests.rs`.
 */
pub mod handler;
pub mod main_window_ui_state;
pub mod ui_constants;

#[cfg(test)]
mod handler_tests;

pub use main_window_ui_state::MainWindowUiState;
#[cfg(test)]
pub use ui_constants::*;
