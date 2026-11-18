#[allow(dead_code)] // Seed skeleton for Wave 2 ui-overlays-data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationStep {
    Previous,
    Next,
    PageUp,
    PageDown,
}
