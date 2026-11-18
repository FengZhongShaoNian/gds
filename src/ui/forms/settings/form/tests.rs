use serde::{Deserialize, Serialize};
use zgui::reactive::{Mounted, RwSignal, install, prelude::*};

use super::*;
use crate::app::Gds;
use crate::service::task_manager::Scope;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, crate::ConfigEditor,
)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    #[setting(label = "form-default")]
    Auto,
    #[setting(label = "form-time-absolute")]
    Manual,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, crate::ConfigEditor)]
#[serde(default)]
#[config(validate = "Self::validate")]
struct SampleSettings {
    #[setting(label = "form-language")]
    enabled: bool,
    #[setting(label = "form-time-format")]
    mode: Mode,
    #[setting(label = "form-font-size", min = 10, max = 30, step = 2, unit = " px")]
    count: u32,
}

impl SampleSettings {
    fn validate(&self) -> Result<(), String> {
        if self.count > 30 {
            return Err("too large".into());
        }
        Ok(())
    }
}

#[test]
fn derived_model_and_choices_work_without_gpui() {
    install().ok();
    let root = Mounted::new();
    root.with(|| {
        let _gds = Gds::install(Scope::default());
        let fields = SampleSettings::fields();
        assert_eq!(fields.len(), 3);
        let choices = Mode::choices();
        assert_eq!(choices.len(), 2);
        assert!(
            SampleSettings {
                enabled: true,
                mode: Mode::Manual,
                count: 16
            }
            .validate()
            .is_ok()
        );
        assert!(
            SampleSettings {
                enabled: true,
                mode: Mode::Manual,
                count: 40
            }
            .validate()
            .is_err()
        );

        let sig = RwSignal::new_local(SampleSettings::default());
        let _editor = SampleSettingsEditor::new(sig).disabled(false);
        assert_eq!(sig.get_untracked(), SampleSettings::default());
    });
    root.unmount();
}
