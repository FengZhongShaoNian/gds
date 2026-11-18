use zgui::prelude::*;

/// Focus anchor restored when a modal dialog closes.
#[derive(Clone, Copy, Default)]
pub struct DialogAnchor {
    pub trigger: NodeRef,
}

impl DialogAnchor {
    #[must_use]
    pub fn new(trigger: NodeRef) -> Self {
        Self { trigger }
    }

    pub fn restore(self) {
        self.trigger.focus();
    }
}
