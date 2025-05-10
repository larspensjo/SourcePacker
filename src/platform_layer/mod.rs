pub mod app;
pub(crate) mod control_treeview;
pub mod error;
pub mod types;
pub(crate) mod window_common;
pub use app::PlatformInterface;
pub use error::{PlatformError, Result as PlatformResult};
pub use types::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowConfig, WindowId,
};
