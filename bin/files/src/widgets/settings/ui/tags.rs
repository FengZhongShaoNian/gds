use gds::ui::settings::{
    SettingActionsProps, SettingControlProps, SettingLabelProps, SettingRowProps,
};
use gds::{app::Gds, i18n};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, ReadSignal, RenderEffect, RwSignal, on_cleanup_local};
use zgui::view;
use zgui::view::install_stylesheet;
use zgui_ui::{ButtonProps, ButtonSize, ButtonVariant, SwitchProps, SwitchSize};
use zgui_ui_primitives::Binding;

use super::{Preferences, View};
use crate::entities::tags;

const TAGS_SHEET: &str = "gds-files-settings-tags";
const TAGS_CSS: &str = r#"
.gds-settings-tags {
    display: flex;
    flex-direction: column;
}

.gds-settings-tags__title {
    font-size: 18px;
    font-weight: 600;
    margin-bottom: 16px;
}

.gds-settings-tags__probe {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 12px;
    padding: 12px 0;
}

.gds-settings-tags__status {
    flex: 1 1 0%;
    min-width: 0;
    color: var(--gds-muted-foreground);
}
"#;

#[derive(Clone)]
enum State {
    Checking,
    Ready,
    Failed(String),
}

#[derive(Clone, Copy)]
pub(super) struct Probe {
    state: RwSignal<State, LocalStorage>,
    refresh: RwSignal<u64, LocalStorage>,
}

impl Probe {
    pub(super) fn new(preferences: ReadSignal<Preferences, LocalStorage>) -> Self {
        let state = RwSignal::new_local(State::Checking);
        let refresh = RwSignal::new_local(0_u64);
        let fx = RenderEffect::new(move |_| {
            let _ = refresh.get();
            let commands = preferences.with(|p| p.tags.commands.clone());
            state.set(State::Checking);
            Gds::current().tasks().spawn_ui(
                "Check tag backend",
                async move { tags::connect(&commands).await },
                move |result| {
                    state.set(match result {
                        Ok(Ok(_)) => State::Ready,
                        Ok(Err(error)) => State::Failed(error),
                        Err(error) => State::Failed(error.to_string()),
                    });
                },
            );
        });
        on_cleanup_local(move || drop(fx));
        Self { state, refresh }
    }

    fn ready(self) -> bool {
        self.state.with(|state| matches!(state, State::Ready))
    }

    fn checking(self) -> bool {
        self.state.with(|state| matches!(state, State::Checking))
    }

    fn status(self) -> String {
        self.state.with(|state| match state {
            State::Checking => i18n::text("tags-checking"),
            State::Ready => i18n::text("tags-ready"),
            State::Failed(error) => error.clone(),
        })
    }
}

impl View {
    pub(super) fn extensions(&self) -> AnyView {
        install_stylesheet(TAGS_SHEET, TAGS_CSS);
        let probe = self.tags;
        let preferences = self.preferences;
        let view = self.clone();
        let enabled = Binding::controlled(
            Signal::derive_local(move || preferences.with(|p| p.tags.enabled)),
            move |enabled: bool| {
                if !enabled || probe.state.with_untracked(|s| matches!(s, State::Ready)) {
                    view.apply(|p| p.tags.enabled = enabled);
                }
            },
        );
        let switch_disabled =
            Signal::derive_local(move || !preferences.with(|p| p.tags.enabled) && !probe.ready());
        let reset = self.reset(
            "tags-enabled",
            |p| p.tags.enabled,
            |p, v| p.tags.enabled = v,
        );
        let commands = self
            .subform(|p| p.tags.commands.clone(), |p, v| p.tags.commands = v)
            .view();

        AnyView::new(view! {
            column(class = "gds-settings-tags") {
                text(class = "gds-settings-tags__title") {{move || i18n::text("tags-title")}}
                SettingRow {
                    SettingLabel {
                        text {{move || i18n::text("tags-enabled")}}
                    }
                    SettingActions {
                        {reset}
                        SettingControl {
                            Switch(
                                checked = enabled,
                                disabled = switch_disabled,
                                size = SwitchSize::Sm,
                            )
                        }
                    }
                }
                row(class = "gds-settings-tags__probe") {
                    text(class = "gds-settings-tags__status") {{move || probe.status()}}
                    Button(
                        variant = ButtonVariant::Outline,
                        size = ButtonSize::Sm,
                        disabled = Signal::derive_local(move || probe.checking()),
                        on:click = move |_| probe.refresh.update(|n| *n = n.wrapping_add(1)),
                    ) {{move || i18n::text("tags-detect")}}
                }
                {commands}
            }
        })
    }
}
