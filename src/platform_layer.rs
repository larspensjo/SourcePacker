pub mod app;
pub(crate) mod command_executor;
pub(crate) mod controls;
pub(crate) mod dialog_handler;
pub mod error;
pub mod types;
pub(crate) mod window_common;

pub use app::PlatformInterface;
pub use error::Result as PlatformResult;
pub use types::{
    AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler,
    UiStateProvider, TreeItemDescriptor, TreeItemId, WindowConfig, WindowId,
};
