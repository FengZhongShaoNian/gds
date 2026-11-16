//! Top toolbar preserving filename-only inline rename, Copy Image button, and suite chrome.

use gds::{
    service::image::source::Origin,
    ui::{Frame, Shortcut, button::IconButton, field, window::WindowControls},
};
use gds_uim::el;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, MouseButton, SharedString, TestSupportExt as _,
    Window,
    assets::IconName,
    component::{ActiveTheme as _, StyledExt as _},
    prelude::*,
    px,
};

use super::*;
use crate::shared::{
    i18n::text,
    ui::actions::{
        CloseSettings, CopyImage, EditLocation, OpenClipboard, OpenFile, RedoEdit, RevealInFiles,
        SETTINGS_CONTEXT, SaveImage, ToggleFilmstrip, ToggleInspector, UndoEdit, WORKSPACE_CONTEXT,
    },
};

const SMALL_GAP: f32 = 4.0;
const CAPTION_PADDING: f32 = 6.0;
const LOCATION_RADIUS: f32 = 6.0;
const LOCATION_HEIGHT: f32 = gds::appearance::tokens::ICON_BUTTON_SIZE + 2.0;

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn render_toolbar(
        &mut self,
        frame: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let window_controls = el! {
            .WindowControls::default {
                on_close_window: $(cx.listener(|_, _, window, _| window.remove_window()));
            }
        };

        if self.settings.open {
            let back_btn = el! {
                "settings-back".IconButton::new {
                    icon: IconName::ArrowLeft;
                    label: $(text("action-back", cx));
                    command: $(Shortcut::new(Box::new(CloseSettings), Some(SETTINGS_CONTEXT)));
                    on_click: $(cx.listener(|this, _, window, cx| this.close_settings(window, cx)));
                }
            };
            let sidebar_btn = el! {
                "toolbar-sidebar".IconButton::new {
                    icon: IconName::PanelLeft;
                    label: $(text("action-toggle-filmstrip", cx));
                    command: $(Shortcut::new(Box::new(ToggleFilmstrip), Some(SETTINGS_CONTEXT)));
                    toggled: $(self.chrome.filmstrip_expanded);
                    on_click: $(cx.listener(|this, _, _, cx| this.toggle_filmstrip(cx)));
                }
            };
            let title = text("settings-title", cx);
            let controls = el! {
                .div { flex; items_center; gap_2; flex_1; min_w_0; } [
                    sidebar_btn,
                    .div {
                        text_sm; font_semibold;
                        text_color: $(cx.theme().foreground);
                        truncate;
                    } [title],
                ]
            };
            return frame.toolbar_with_window_controls(
                back_btn.into_any_element(),
                controls.into_any_element(),
                window_controls,
                cx,
            );
        }

        let menu = self.render_app_menu_control(window, cx);
        let sidebar_btn = el! {
            "toolbar-sidebar".IconButton::new {
                icon: IconName::PanelLeft;
                label: $(text("action-toggle-filmstrip", cx));
                command: $(Shortcut::new(Box::new(ToggleFilmstrip), Some(WORKSPACE_CONTEXT)));
                toggled: $(self.chrome.filmstrip_expanded);
                on_click: $(cx.listener(|this, _, _, cx| this.toggle_filmstrip(cx)));
            }
        };
        let open_btn = el! {
            "toolbar-open".IconButton::new {
                icon: IconName::FolderOpen;
                label: $(text("action-open", cx));
                command: $(Shortcut::new(Box::new(OpenFile), Some(WORKSPACE_CONTEXT)));
                on_click: $(cx.listener(|this, _, window, cx| this.open_file_picker(window, cx)));
            }
        };
        let paste_btn = el! {
            "toolbar-clipboard".IconButton::new {
                icon: IconName::Clipboard;
                label: $(text("action-open-clipboard", cx));
                command: $(Shortcut::new(Box::new(OpenClipboard), Some(WORKSPACE_CONTEXT)));
                on_click: $(cx.listener(|this, _, window, cx| this.open_from_clipboard(window, cx)));
            }
        };

        let has_doc = self.session.document.is_some();
        let is_dirty = self.has_unsaved_changes();
        let is_saving = self.runtime.is_saving;

        let save_btn = el! {
            "toolbar-save".IconButton::new {
                icon: IconName::Save;
                label: $(text("action-save", cx));
                command: $(Shortcut::new(Box::new(SaveImage), Some(WORKSPACE_CONTEXT)));
                disabled: $(!has_doc || is_saving);
                toggled: is_dirty;
                on_click: $(cx.listener(|this, _, window, cx| this.save_current_image(window, cx)));
            }
        };
        let copy_btn = el! {
            "toolbar-copy".IconButton::new {
                icon: IconName::Copy;
                label: $(text("action-copy", cx));
                command: $(Shortcut::new(Box::new(CopyImage), Some(WORKSPACE_CONTEXT)));
                disabled: $(!has_doc);
                active: $(self.runtime.launch.is_capture && !is_dirty);
                on_click: $(cx.listener(|this, _, window, cx| this.copy_to_clipboard(window, cx)));
            }
        };
        let undo_btn = el! {
            "toolbar-undo".IconButton::new {
                icon: IconName::Undo;
                label: $(text("action-undo", cx));
                command: $(Shortcut::new(Box::new(UndoEdit), Some(WORKSPACE_CONTEXT)));
                disabled: $(!has_doc || !self.canvas.history.can_undo());
                on_click: $(cx.listener(|this, _, _, cx| this.undo_edit(cx)));
            }
        };
        let redo_btn = el! {
            "toolbar-redo".IconButton::new {
                icon: IconName::Redo;
                label: $(text("action-redo", cx));
                command: $(Shortcut::new(Box::new(RedoEdit), Some(WORKSPACE_CONTEXT)));
                disabled: $(!has_doc || !self.canvas.history.can_redo());
                on_click: $(cx.listener(|this, _, _, cx| this.redo_edit(cx)));
            }
        };
        let inspector_btn = el! {
            "toolbar-inspector".IconButton::new {
                icon: IconName::PanelRight;
                label: $(text("action-toggle-inspector", cx));
                command: $(Shortcut::new(Box::new(ToggleInspector), Some(WORKSPACE_CONTEXT)));
                toggled: $(self.chrome.inspector_visible);
                on_click: $(cx.listener(|this, _, _, cx| this.toggle_inspector(cx)));
            }
        };

        let location = self.render_location_pill(cx);

        let controls = el! {
            .div {
                flex; items_center; gap: $(px(SMALL_GAP)); flex_1; min_w_0;
            } [
                sidebar_btn,
                open_btn,
                paste_btn,
                save_btn,
                copy_btn,
                undo_btn,
                redo_btn,
                location,
                inspector_btn,
            ]
        };

        frame.toolbar_with_window_controls(
            menu.into_any_element(),
            controls.into_any_element(),
            window_controls,
            cx,
        )
    }

    fn render_location_pill(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let editing = self.session.editing_path;
        if !editing && self.session.document.is_none() {
            return el! { .div { flex_1; min_w_0; } }.into_any_element();
        }

        let border_col = if editing {
            cx.theme().ring
        } else {
            cx.theme().border
        };

        let inner = if editing {
            let input = field::input(&self.focus.path_input, cx)
                .appearance(false)
                .h_full();
            vec![
                el! {
                    .div {
                        # "location-editor";
                        flex_1; min_w_0; h_full; px_2; flex; items_center;
                    } [input]
                }
                .into_any_element(),
            ]
        } else {
            let doc = self
                .session
                .document
                .as_ref()
                .expect("checked non-empty document");
            let filename = origin_display_name(&doc.decoded.origin);
            let display: SharedString = if self.has_unsaved_changes() {
                format!("{filename} *")
            } else {
                filename.to_owned()
            }
            .into();
            let is_local = matches!(doc.decoded.origin, Origin::LocalFile { .. });
            let is_busy = self.runtime.is_loading || self.runtime.is_saving;

            let mut label = el! {
                .div {
                    # "location-label";
                    flex_1; min_w_0; h_full; px_2; flex; items_center;
                } [
                    .div {
                        w_full; truncate; text_sm; font_medium;
                        text_color: $(cx.theme().foreground);
                    } [display],
                ]
            };
            if is_local {
                label = label
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, window, cx| this.begin_path_edit(window, cx)));
            }

            let mut children = vec![label.into_any_element()];
            if is_local {
                let reveal_btn = el! {
                    "img-reveal-inline".IconButton::new {
                        icon: IconName::Folder;
                        label: $(text("action-reveal", cx));
                        command: $(Shortcut::new(Box::new(RevealInFiles), Some(WORKSPACE_CONTEXT)));
                        on_click: $(cx.listener(|this, _, _, cx| this.reveal_active_file(cx)));
                    }
                };
                let edit_btn = el! {
                    "img-edit-path".IconButton::new {
                        icon: IconName::Pencil;
                        label: $(text("action-edit-location", cx));
                        command: $(Shortcut::new(Box::new(EditLocation), Some(WORKSPACE_CONTEXT)));
                        disabled: is_busy;
                        on_click: $(cx.listener(|this, _, window, cx| this.begin_path_edit(window, cx)));
                    }
                };
                children.push(
                    el! {
                        .div {
                            # "location-actions"; test_support;
                            flex; items_center; gap_0p5; pr_0p5; flex_shrink_0;
                            on_mouse_down: MouseButton::Left, $(|_, _, cx| cx.stop_propagation());
                        } [reveal_btn, edit_btn]
                    }
                    .into_any_element(),
                );
            }
            children
        };

        el! {
            .div {
                # "location-display";
                flex_1;
                min_w_0;
                flex;
                items_center;
                mx: $(px(CAPTION_PADDING));
                h: $(px(LOCATION_HEIGHT));
                rounded: $(px(LOCATION_RADIUS));
                border_1;
                border_color: border_col;
                overflow_hidden;
            } [..inner]
        }
        .test_support()
        .block_mouse_except_scroll()
        .into_any_element()
    }
}
