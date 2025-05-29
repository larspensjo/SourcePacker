pub mod app;
pub(crate) mod control_treeview;
pub mod error;
pub mod types;
pub(crate) mod window_common;
// Add command_executor to the public module exports if needed, or keep it crate-internal
// For now, keep it crate-internal as it's an implementation detail of the platform_layer.
pub(crate) mod command_executor; // Made crate-visible

pub use app::PlatformInterface;
pub use error::{PlatformError, Result as PlatformResult};
pub use types::{
    AppEvent, CheckState, DockStyle, LayoutRule, MessageSeverity, PlatformCommand,
    PlatformEventHandler, TreeItemDescriptor, TreeItemId, WindowConfig, WindowId,
};
