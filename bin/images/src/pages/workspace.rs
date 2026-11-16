//! Workspace page owning active image viewing, editing, filmstrip, inspector, and settings.

mod ui;

use std::{path::PathBuf, sync::Arc};

pub use ui::WorkspacePage;

/// Initial launch mode mapped from CLI arguments or stdin without coupling `pages` to `app`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    EmptySmartState,
    OpenFile(PathBuf),
    Clipboard,
    Stdin(Arc<[u8]>),
}

/// Optional capture workflow post-save/copy behavior passed in by window creation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureWorkflow {
    pub output: Option<PathBuf>,
    pub copy_on_save: bool,
    pub early_exit: bool,
}

/// Startup parameters for [`WorkspacePage`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchOptions {
    pub mode: LaunchMode,
    pub workflow: CaptureWorkflow,
    pub is_capture: bool,
}
