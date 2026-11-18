//! Picker keyboard command definitions (`Back`, `Forward`, `Up`, `Refresh`, `EditLocation`, `Filter`).

#[allow(dead_code)] // Seed skeleton for Wave 2 ui-overlays-data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PickerCommand {
    Back,
    Forward,
    Up,
    Refresh,
    EditLocation,
    Filter,
}
