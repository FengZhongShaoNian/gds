use crate::service::logs::Level;

#[allow(dead_code)] // Seed skeleton for Wave 2 ui-overlays-data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterState {
    pub level: Option<Level>,
    pub query: String,
}
