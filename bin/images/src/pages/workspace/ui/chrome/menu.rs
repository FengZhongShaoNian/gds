//! Application hamburger menu popover and shared GDS About modal launcher.

use gds::{
    service::image::source::Origin,
    ui::{
        Shortcut,
        about::{self, About, Application},
        button::Button,
        dialog::{DialogCommand, DialogDecision, DialogFooter},
        logs::OpenLogs,
        menu::{item as menu_item, popup},
    },
};
use gds_uim::el;
use gpui_kit::{
    Context, DismissEvent, Entity, Focusable as _, IntoElement, SharedString, TestSupportExt as _,
    WeakEntity, Window,
    assets::IconName,
    component::{
        WindowExt as _,
        dialog::AlertDialog,
        menu::PopupMenu,
        popover::{Popover, PopoverState},
    },
    div,
    prelude::*,
};

use super::*;
use crate::shared::{
    i18n::text,
    ui::actions::{
        CopyImage, EditLocation, MoveToTrash, OpenClipboard, OpenFile, OpenSettings, OpenWithApp,
        RevealInFiles, SaveImage, SaveImageAs, WORKSPACE_CONTEXT,
    },
};

#[derive(Default)]
struct AppMenuState {
    generation: u64,
    menu: Option<Entity<PopupMenu>>,
}

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn render_app_menu_control(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let open = self.chrome.app_menu_open;
        let menu_gen = self.chrome.app_menu_generation;
        let view = cx.entity().downgrade();
        let application = Application::cargo(env!("CARGO_BIN_NAME"));
        let trigger = el! {
            ("application-menu", $(application.icon)).Button::application_menu {
                accessibility_label: $(text("action-app-menu", cx));
                tooltip: $(text("action-app-menu", cx));
            }
        };

        let opener = view.clone();
        let content_view = view.clone();
        let popover = el! {
            "application-menu-popover".Popover::new {
                appearance: false;
                overlay_closable: false;
                open: open;
                trigger: trigger;
                on_open_change: $(move |next: &bool, _, cx| {
                    let _ = opener.update(cx, |this, cx| {
                        this.set_app_menu_open(*next, cx);
                    });
                });
                content: $(move |_: &mut PopoverState, window: &mut Window, cx: &mut Context<PopoverState>| {
                    build_app_menu_popup(content_view.clone(), menu_gen, window, cx)
                });
            }
        };
        el! {
            .div { # "application-menu-slot"; test_support; flex; items_center; justify_center; } [
                popover,
            ]
        }
        .into_any_element()
    }
    pub(in crate::pages::workspace::ui) fn show_about(
        view: WeakEntity<Self>,
        window: &mut Window,
        _: &mut gpui_kit::App,
    ) {
        window.on_next_frame(move |window, cx| {
            let Some(view) = view.upgrade() else {
                return;
            };
            if window.has_active_dialog(cx) {
                return;
            }
            let focus = view.read(cx).focus.canvas.clone();
            focus.focus(window, cx);
            let about = cx.new(|cx| About::new(Application::cargo(env!("CARGO_BIN_NAME")), cx));
            window.open_alert_dialog(cx, move |dialog: AlertDialog, _, cx| {
                let footer = el! {
                    .DialogFooter::default { px_4; pb_4; } [
                        DialogCommand::Cancel.DialogDecision::new { label: $(text("close", cx)); },
                    ]
                };
                el! {
                    dialog {
                        p_0;
                        footer: footer;
                    } [
                        .div { h: about::HEIGHT.px; min_h_0; } [$about.clone()],
                    ]
                }
            });
        });
    }
}

fn build_app_menu_popup(
    view: WeakEntity<WorkspacePage>,
    menu_gen: u64,
    window: &mut Window,
    cx: &mut Context<PopoverState>,
) -> gpui_kit::AnyElement {
    let state = window.use_keyed_state(SharedString::from("images-app-menu"), cx, |_, _| {
        AppMenuState::default()
    });
    if state.read(cx).generation == menu_gen
        && let Some(menu) = state.read(cx).menu.clone()
    {
        return menu.into_any_element();
    }

    let build_view = view.clone();
    let menu = PopupMenu::build(window, cx, move |menu, _, cx| {
        populate_app_menu(menu, build_view, cx)
    });
    state.update(cx, |s, _| {
        s.generation = menu_gen;
        s.menu = Some(menu.clone());
    });

    let opening_menu = menu.downgrade();
    let opening_view = view.clone();
    window.on_next_frame(move |window, cx| {
        if opening_view
            .upgrade()
            .is_some_and(|v| v.read(cx).chrome.app_menu_open)
            && let Some(menu) = opening_menu.upgrade()
        {
            menu.focus_handle(cx).focus(window, cx);
        }
    });

    let popover = cx.entity().downgrade();
    window
        .subscribe(&menu, cx, {
            let state = state.downgrade();
            move |_, _: &DismissEvent, window, cx| {
                let _ = popover.update(cx, |p, cx| p.dismiss(window, cx));
                let _ = state.update(cx, |s, _| s.menu = None);
                let _ = view.update(cx, |this, cx| this.set_app_menu_open(false, cx));
            }
        })
        .detach();

    menu.into_any_element()
}

fn populate_app_menu(
    menu: PopupMenu,
    view: WeakEntity<WorkspacePage>,
    cx: &gpui_kit::App,
) -> PopupMenu {
    let page = view.upgrade().map(|v| v.read(cx));
    let has_doc = page.as_ref().is_some_and(|p| p.session.document.is_some());
    let is_local = page.as_ref().is_some_and(|p| {
        p.session
            .document
            .as_ref()
            .is_some_and(|doc| matches!(doc.decoded.origin, Origin::LocalFile { .. }))
    });
    let is_busy = page.as_ref().is_some_and(|p| {
        p.session
            .document
            .as_ref()
            .is_some_and(|doc| p.is_origin_busy(&doc.decoded.origin))
    });
    let logs_enabled = cx.gds().ui().logs().is_enabled();
    let about_view = view.clone();
    let logs_view = view.clone();

    let mut builder = AppMenuBuilder {
        menu: popup(menu),
        view: &view,
    }
    .add(
        text("action-open", cx),
        Some(Shortcut::new(Box::new(OpenFile), Some(WORKSPACE_CONTEXT))),
        IconName::FolderOpen,
        true,
        |this, window, cx| this.open_file_picker(window, cx),
    )
    .add(
        text("action-open-clipboard", cx),
        Some(Shortcut::new(
            Box::new(OpenClipboard),
            Some(WORKSPACE_CONTEXT),
        )),
        IconName::Clipboard,
        true,
        |this, window, cx| this.open_from_clipboard(window, cx),
    )
    .separator()
    .add(
        text("action-save", cx),
        Some(Shortcut::new(Box::new(SaveImage), Some(WORKSPACE_CONTEXT))),
        IconName::Save,
        has_doc && !is_busy,
        |this, window, cx| this.save_current_image(window, cx),
    )
    .add(
        text("action-save-as", cx),
        Some(Shortcut::new(
            Box::new(SaveImageAs),
            Some(WORKSPACE_CONTEXT),
        )),
        IconName::Save,
        has_doc && !is_busy,
        |this, window, cx| this.save_image_as(window, cx),
    )
    .add(
        text("action-copy", cx),
        Some(Shortcut::new(Box::new(CopyImage), Some(WORKSPACE_CONTEXT))),
        IconName::Copy,
        has_doc,
        |this, window, cx| this.copy_to_clipboard(window, cx),
    )
    .separator()
    .add(
        text("action-edit-location", cx),
        Some(Shortcut::new(
            Box::new(EditLocation),
            Some(WORKSPACE_CONTEXT),
        )),
        IconName::Pencil,
        is_local && !is_busy,
        |this, window, cx| this.begin_path_edit(window, cx),
    )
    .add(
        text("action-reveal", cx),
        Some(Shortcut::new(
            Box::new(RevealInFiles),
            Some(WORKSPACE_CONTEXT),
        )),
        IconName::Folder,
        is_local,
        |this, _, cx| this.reveal_active_file(cx),
    )
    .add(
        text("action-open-with", cx),
        Some(Shortcut::new(
            Box::new(OpenWithApp),
            Some(WORKSPACE_CONTEXT),
        )),
        IconName::ExternalLink,
        is_local,
        |this, _, cx| this.open_active_file_with(cx),
    )
    .add(
        text("action-trash", cx),
        Some(Shortcut::new(
            Box::new(MoveToTrash),
            Some(WORKSPACE_CONTEXT),
        )),
        IconName::Trash,
        is_local && !is_busy,
        |this, window, cx| this.move_active_to_trash(window, cx),
    )
    .separator()
    .add(
        text("action-settings", cx),
        Some(Shortcut::new(
            Box::new(OpenSettings),
            Some(WORKSPACE_CONTEXT),
        )),
        IconName::Settings,
        true,
        |this, window, cx| this.open_settings(window, cx),
    );

    if logs_enabled {
        builder.menu = builder.menu.item(
            menu_item(
                text("action-logs", cx),
                Some(Shortcut::new(Box::new(OpenLogs), None)),
                true,
            )
            .icon(IconName::List)
            .on_click(move |_, window, cx| {
                let _ = logs_view.update(cx, |this, cx| this.set_app_menu_open(false, cx));
                window.dispatch_action(Box::new(OpenLogs), cx);
            }),
        );
    }

    builder.menu.item(
        menu_item(text("action-about", cx), None, true)
            .icon(IconName::Info)
            .on_click(move |_, window, cx| {
                let _ = about_view.update(cx, |this, cx| this.set_app_menu_open(false, cx));
                WorkspacePage::show_about(about_view.clone(), window, cx);
            }),
    )
}

struct AppMenuBuilder<'a> {
    menu: PopupMenu,
    view: &'a WeakEntity<WorkspacePage>,
}

impl AppMenuBuilder<'_> {
    fn separator(mut self) -> Self {
        self.menu = self.menu.separator();
        self
    }

    fn add(
        mut self,
        label: SharedString,
        shortcut: Option<Shortcut>,
        icon: IconName,
        enabled: bool,
        action: impl Fn(&mut WorkspacePage, &mut Window, &mut Context<WorkspacePage>) + 'static,
    ) -> Self {
        let view = self.view.clone();
        self.menu =
            self.menu
                .item(menu_item(label, shortcut, enabled).icon(icon).on_click(
                    move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.set_app_menu_open(false, cx);
                            action(this, window, cx);
                        });
                    },
                ));
        self
    }
}
