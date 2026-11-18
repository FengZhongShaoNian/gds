mod navigation;
mod tasks;

use super::{OpenLogs, Options, is_enabled};

#[test]
fn log_window_types_and_options_default() {
    let options = Options::default();
    assert_eq!(options.app_name, "GDS");
    let _ = (OpenLogs, is_enabled());
}
