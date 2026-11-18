/// Outcome produced when a dialog button or shortcut completes a prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DialogDecision {
    Confirm,
    Cancel,
    Dismiss,
}
