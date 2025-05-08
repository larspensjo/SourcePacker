// Re-exports the public parts of the facade

pub mod app;
pub mod error;
pub mod window;

// Re-export the main facade components for easier use.
pub use app::App;
pub use error::{Result as UiResult, UiError}; // Renamed to avoid conflict with std::result::Result
pub use window::{Window, WindowBuilder};
