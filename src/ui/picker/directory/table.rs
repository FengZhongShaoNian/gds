use super::model::Entry;

/// Filtered rows projected for directory picker display.
#[allow(dead_code)] // Seed skeleton for Wave 2 ui-overlays-data.
#[derive(Clone, Default)]
pub struct DirectoryRows {
    pub entries: Vec<Entry>,
}
