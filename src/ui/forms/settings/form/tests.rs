use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use zgui::prelude::*;
use zgui::reactive::{Mounted, RwSignal, install};

use super::editor::field_matches_query;
use super::*;
use crate::app::Gds;
use crate::i18n::Language;
use crate::service::task_manager::Scope;
use crate::ui::forms::field::placeholder::PlaceholderProps;
use crate::ui::forms::field::{
    FieldLabelProps, InputFieldProps, SearchChoiceFieldProps, SearchOption, TextareaFieldProps,
};

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::ConfigEditor)]
#[serde(default)]
#[config(validate = "Self::validate")]
struct RichSettings {
    #[setting(group = "form-font-family", label = "form-font-family", kind = "font")]
    font_family: Option<String>,
    #[setting(
        group = "form-font-family",
        label = "form-font-size",
        min = 8.0,
        max = 32.0,
        step = 0.5,
        unit = " px"
    )]
    font_size: f32,
    #[setting(group = "form-language", label = "form-language")]
    language: Language,
    #[setting(group = "form-language", label = "form-time-format")]
    mode_opt: Option<Mode>,
    #[setting(label = "form-search")]
    query_note: Option<String>,
    #[setting(label = "form-reset", kind = "string_list")]
    args: Vec<String>,
    #[setting(label = "form-default")]
    enabled: bool,
}

impl Default for RichSettings {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: 14.0,
            language: Language::English,
            mode_opt: None,
            query_note: None,
            args: vec!["--flag".to_owned()],
            enabled: true,
        }
    }
}

impl RichSettings {
    fn validate(&self) -> Result<(), String> {
        if self.font_size < 8.0 {
            return Err("font_size too small".into());
        }
        Ok(())
    }
}

#[test]
fn derived_model_and_choices_work_without_gpui() {
    install().ok();
    let root = Mounted::new();
    root.with(|| {
        let gds = Gds::install(Scope::default());
        gds.i18n().install(Language::English).expect("en-US");
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
        let observed: Rc<RefCell<Vec<SampleSettings>>> = Rc::new(RefCell::new(Vec::new()));
        let observed_cb = Rc::clone(&observed);
        let filter = RwSignal::new_local(String::new());
        let focus_target = NodeRef::new();
        let _editor = SampleSettingsEditor::new(sig)
            .disabled(false)
            .filter(filter)
            .reset_focus(focus_target)
            .on_change(move |changed| observed_cb.borrow_mut().push(changed.value))
            .view();
        assert_eq!(sig.get_untracked(), SampleSettings::default());
        assert!(observed.borrow().is_empty());
    });
    root.unmount();
}

#[test]
fn rich_settings_editor_builds_all_control_kinds_and_supports_filtering() {
    install().ok();
    let root = Mounted::new();
    root.with(|| {
        let gds = Gds::install(Scope::default());
        gds.i18n().install(Language::English).expect("en-US");

        let fields = RichSettings::fields();
        assert_eq!(fields.len(), 7);

        let sig = RwSignal::new_local(RichSettings::default());
        let disabled = RwSignal::new_local(false);
        let filter = RwSignal::new_local(String::new());

        let _editor = RichSettingsEditor::new(sig)
            .disabled(disabled)
            .filter(filter)
            .view();

        let font_field = fields
            .iter()
            .find(|f| f.id == "font_family")
            .expect("font_family field");
        assert!(field_matches_query(font_field, ""));
        assert!(field_matches_query(font_field, "font"));
        assert!(field_matches_query(font_field, "font_family"));
        assert!(!field_matches_query(font_field, "language"));

        // Missing custom font appears in choices formatted via i18n `form-font-missing`.
        let custom_font = RichSettings {
            font_family: Some("Missing Custom Sans".to_owned()),
            ..RichSettings::default()
        };
        let font_choices = font_field.choices(&custom_font);
        assert_eq!(font_choices[0].value, Value::Null);
        assert_eq!(font_choices[0].title, "System default");
        assert!(
            font_choices
                .iter()
                .any(|c| c.title == "Missing Custom Sans (not installed)")
        );

        // Language choices carry search terms matching locale codes & native names.
        let lang_field = fields
            .iter()
            .find(|f| f.id == "language")
            .expect("language field");
        let lang_choices = lang_field.choices(&custom_font);
        assert!(lang_choices.iter().any(|c| !c.terms().is_empty()));

        // Standalone SearchOption & field wrappers match hidden keywords cleanly.
        let zh_opt = SearchOption::new("chinese", "简体中文").with_keywords(["zh-CN", "Chinese"]);
        assert!(zh_opt.matches_query(""));
        assert!(zh_opt.matches_query("zh-cn"));
        assert!(zh_opt.matches_query("chinese 简体"));
        assert!(!zh_opt.matches_query("french"));

        let choice_sig = RwSignal::new_local("chinese".to_owned());
        let opts_sig = RwSignal::new_local(vec![zh_opt]);
        let _fields_view = view! {
            column {
                FieldLabel { "Search" }
                SearchChoiceField(value = choice_sig, options = opts_sig)
                InputField(placeholder = "Name")
                TextareaField(placeholder = "Args")
                Placeholder(visible = true, text = "Empty")
            }
        };

        filter.set("language".to_owned());
        disabled.set(true);
    });
    root.unmount();
}

#[test]
fn numeric_bounds_validate_and_localize_in_english_and_chinese() {
    install().ok();
    let root = Mounted::new();
    root.with(|| {
        let gds = Gds::install(Scope::default());
        gds.i18n().install(Language::English).expect("en-US");

        let integer_variants = [
            (Integer::I8, -128.0, 127.0, 200.0),
            (Integer::I16, -32768.0, 32767.0, 40000.0),
            (Integer::I32, -1000.0, 1000.0, 3_000_000_000.0),
            (Integer::I64, -100.0, 100.0, 1e25),
            (Integer::Isize, -50.0, 50.0, 1e25),
            (Integer::U8, 0.0, 255.0, 256.0),
            (Integer::U16, 0.0, 65535.0, -1.0),
            (Integer::U32, 0.0, 1000.0, -5.0),
            (Integer::U64, 0.0, 100.0, -1.0),
            (Integer::Usize, 0.0, 100.0, -1.0),
        ];

        for (kind, min, max, bad) in integer_variants {
            let num = Number {
                min,
                max,
                step: 1.0,
                scale: 1.0,
                unit: "",
                integer: kind,
            };
            assert!(num.value(min).is_ok(), "{kind:?} min failed");
            assert!(num.value(max).is_ok(), "{kind:?} max failed");
            let err_en = num.value(bad).expect_err("out-of-range expected");
            assert_eq!(err_en, "Number is outside the field's representable range");
            let nan_en = num.value(f64::NAN).expect_err("NaN expected");
            assert_eq!(nan_en, "Number must be finite");
        }

        let float_num = Number {
            min: 0.0,
            max: 1.0,
            step: 0.1,
            scale: 100.0,
            unit: "%",
            integer: Integer::Float,
        };
        assert_eq!(float_num.value(50.0), Ok(json!(0.5)));
        assert_eq!(float_num.label(50.0), "50%");
        assert!(float_num.value(f64::INFINITY).is_err());

        gds.i18n()
            .install(Language::Chinese)
            .expect("switch to Chinese");
        let u8_num = Number {
            min: 0.0,
            max: 255.0,
            step: 1.0,
            scale: 1.0,
            unit: "",
            integer: Integer::U8,
        };
        assert_eq!(
            u8_num.value(300.0).expect_err("overflow expected"),
            "数值超出此字段可表示的范围"
        );
        assert_eq!(
            u8_num.value(f64::NAN).expect_err("NaN expected"),
            "数值必须为有限数"
        );
    });
    root.unmount();
}

#[test]
fn literal_string_list_lines_preserve_whitespace_and_modified_tracking() {
    let mut model = RichSettings::default();
    let defaults = RichSettings::default();
    let fields = RichSettings::fields();
    let args_field = fields.iter().find(|f| f.id == "args").expect("args field");
    assert!(!args_field.is_modified(&model, &defaults));

    let literal_text = "  leading and trailing  \n\n--opt\n";
    let list: Vec<Value> = literal_text
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| Value::String(line.to_owned()))
        .collect();
    (args_field.set)(&mut model, Value::Array(list)).expect("set args");
    assert_eq!(
        model.args,
        vec!["  leading and trailing  ".to_owned(), "--opt".to_owned()]
    );
    assert!(args_field.is_modified(&model, &defaults));
}

#[test]
fn headless_settings_editor_wires_labelled_by_reset_and_filter() {
    use std::collections::HashMap;
    use std::sync::Arc;

    use zgui::platform::{AppHandler, PlatformError, Surface};
    use zgui::runtime::AppError;
    use zgui_platform_headless::Harness;

    #[derive(Debug, Default)]
    struct HeadlessSink {
        atlas: zgui::atlas::MemorySink,
        target: Option<zgui::render::RenderTarget>,
    }

    impl zgui::render::Renderer for HeadlessSink {
        fn capabilities(&self) -> zgui::render::RenderCapabilities {
            zgui::render::RenderCapabilities::MINIMAL
        }

        fn configure(&mut self, target: zgui::render::RenderTarget) {
            self.target = Some(target);
        }

        fn target(&self) -> Option<zgui::render::RenderTarget> {
            self.target
        }

        fn register_external(
            &mut self,
            _texture: zgui::render::ExternalTexture,
        ) -> zgui::render::TextureHandle {
            zgui::render::TextureHandle(0)
        }

        fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

        fn memory(&self) -> zgui::render::MemoryReport {
            zgui::render::MemoryReport::default()
        }

        fn draw(
            &mut self,
            _scene: &zgui::scene::Scene,
            _damage: &zgui::bits::DamageSet,
        ) -> zgui::render::FrameOutcome {
            zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
        }

        fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
            &mut self.atlas
        }
    }

    let sig_slot: Rc<RefCell<Option<RwSignal<SampleSettings, LocalStorage>>>> =
        Rc::new(RefCell::new(None));
    let filter_slot: Rc<RefCell<Option<RwSignal<String, LocalStorage>>>> =
        Rc::new(RefCell::new(None));
    let sig_slot_view = Rc::clone(&sig_slot);
    let filter_slot_view = Rc::clone(&filter_slot);
    let sig_slot_driver = Rc::clone(&sig_slot);
    let filter_slot_driver = Rc::clone(&filter_slot);

    let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = Harness::new(handler);
        harness.pump();
        if let Some(surface) = harness.platform().offscreens().first() {
            surface.set_a11y_listener(true);
        }
        harness.settle(4);

        let offscreen = harness.platform().offscreens()[0].clone();
        let mut nodes_by_id = HashMap::new();
        for update in offscreen.a11y_log() {
            for (id, node) in update.nodes {
                nodes_by_id.insert(id, node);
            }
        }

        let switch_node = nodes_by_id
            .values()
            .find(|n| format!("{:?}", n.role()) == "Switch")
            .expect("Switch rendered in Editor");
        assert!(
            !switch_node.labelled_by().is_empty(),
            "Switch must have labelled_by pointing to FieldLabel"
        );
        let switch_label_text = switch_node
            .labelled_by()
            .iter()
            .filter_map(|id| nodes_by_id.get(id))
            .find_map(|n| n.value().or_else(|| n.label()));
        assert_eq!(switch_label_text, Some("Language"));

        let slider_node = nodes_by_id
            .values()
            .find(|n| format!("{:?}", n.role()) == "Slider")
            .expect("Slider rendered in Editor");
        assert!(
            !slider_node.labelled_by().is_empty(),
            "Slider must have labelled_by pointing to FieldLabel"
        );

        // Mutating model exposes Reset to default IconButton cleanly.
        let sig = sig_slot_driver.borrow().expect("model signal");
        sig.update(|s| s.enabled = true);
        harness.settle(4);
        assert!(
            offscreen
                .a11y_log()
                .iter()
                .flat_map(|u| u.nodes.iter())
                .any(|(_, n)| n.label() == Some("Reset to default")),
            "expected Reset to default IconButton after modifying setting"
        );

        // Search filtering keeps matching row while removing non-matching ones.
        let filter = filter_slot_driver.borrow().expect("filter signal");
        filter.set("language".to_owned());
        harness.settle(4);
        let last = offscreen.last_a11y_update().expect("filter update");
        assert!(
            !last
                .nodes
                .iter()
                .any(|(_, n)| format!("{:?}", n.role()) == "Slider"),
            "non-matching Font size Slider should not remain in filtered update"
        );

        harness.shut_down();
        Ok(())
    };

    zgui::app()
        .with_title("settings-editor-test")
        .with_size(640.0, 480.0)
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = HeadlessSink::default();
            zgui::render::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
        }))
        .run_on(driver, move || {
            let gds = Gds::try_current().unwrap_or_else(|| Gds::install(Scope::default()));
            gds.i18n().install(Language::English).expect("en-US");
            let sig = RwSignal::new_local(SampleSettings::default());
            let filter = RwSignal::new_local(String::new());
            *sig_slot_view.borrow_mut() = Some(sig);
            *filter_slot_view.borrow_mut() = Some(filter);
            SampleSettingsEditor::new(sig).filter(filter).view()
        })
        .expect("headless settings editor test ran");
}
