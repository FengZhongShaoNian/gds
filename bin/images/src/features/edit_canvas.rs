//! Interactive canvas editing: reversible commands, bounded history stacks, and tool state machine.

mod command;
mod history;
mod tool_fsm;

#[cfg(test)]
mod tests;

pub use command::EditCommand;
pub use history::{EditHistory, MAX_UNDO_DEPTH};
pub use tool_fsm::{DragPreview, FinishOutcome, ToolFsm, ToolMode, ToolSettings};
