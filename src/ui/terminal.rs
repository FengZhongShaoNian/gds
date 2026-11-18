//! Reusable single-terminal PTY controller and custom `zgui` view.

mod input;
mod instance;
mod preferences;
mod view;

pub use crate::service::terminal::{Shutdown, Status};
pub use instance::Terminal;
pub use preferences::{Preferences, PreferencesEditor};
pub use view::TerminalView;

pub const INPUT_CONTEXT: &str = "GdsTerminal";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Copy;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Paste;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScrollPageUp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScrollPageDown;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScrollTop;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScrollBottom;

/// Observable terminal session events decoupled from view mounts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Changed,
    TitleChanged(String),
    Bell,
    Closed(Option<i32>),
}
