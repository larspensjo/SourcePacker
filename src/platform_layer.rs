#[cfg(target_os = "windows")]
pub mod app;
#[cfg(target_os = "windows")]
pub(crate) mod command_executor;
#[cfg(target_os = "windows")]
pub(crate) mod controls;
pub mod error;
pub(crate) mod styling_primitives;
#[cfg(not(target_os = "windows"))]
pub(crate) mod styling_stub;
#[cfg(target_os = "windows")]
pub(crate) mod styling_windows;
#[cfg(not(target_os = "windows"))]
pub(crate) use styling_stub as styling;
#[cfg(target_os = "windows")]
pub(crate) use styling_windows as styling;
pub mod types;
#[cfg(target_os = "windows")]
pub(crate) mod window_common;

#[cfg(target_os = "windows")]
pub use app::PlatformInterface;
pub use error::Result as PlatformResult;
pub use styling_primitives::{Color, ControlStyle, FontDescription, FontWeight, StyleId};
pub use types::{
    AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler,
    TreeItemDescriptor, TreeItemId, UiStateProvider, WindowConfig, WindowId,
};
