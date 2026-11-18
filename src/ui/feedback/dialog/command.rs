/// Keyboard confirmation/cancellation commands routed to active dialogs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DialogCommand {
    Confirm,
    Cancel,
}
