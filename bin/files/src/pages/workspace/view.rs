#![allow(dead_code)]
//! Workspace shell composing frame, toolbar, sidebar, content area, terminals, and overlays.

use std::time::Duration;

use gds::{
    i18n,
    ui::{
        feedback::notifications::{self, NotificationHostProps},
        frame::{FrameProps, Toolbar, WorkspaceFrame},
        interaction::keytips::KeytipHostProps,
    },
};
use zgui::prelude::*;
use zgui::reactive::RenderEffect;
use zgui::view;
use zgui_ui::{
    ButtonProps, ButtonVariant, DialogContentProps, DialogFooterProps, DialogHeaderProps,
    DialogProps, DialogTitleProps,
};
use zgui_ui_primitives::Binding;

use super::{
    model::{Pane, Prompt},
    ui::Browser,
};

pub(super) const WORKSPACE_MOTION: Duration = Duration::from_millis(160);

impl Browser {
    pub(crate) fn view(&self) -> AnyView {
        if let Some(window) = &self.0.window {
            let size = window.size();
            let window_width = self.0.window_width;
            let width_fx = RenderEffect::new(move |_| {
                let next = size.get().width.0;
                if (window_width.get_untracked() - next).abs() > f32::EPSILON {
                    window_width.set(next);
                }
            });
            on_cleanup_local(move || drop(width_fx));
        }

        let notice = self.0.notice;
        let notice_fx = RenderEffect::new(move |_| {
            if let Some(message) = notice.get() {
                notifications::error(message);
            }
        });
        on_cleanup_local(move || drop(notice_fx));

        let keyboard = self.clone();
        let mouse = self.clone();
        let browser = self.clone();
        let prompt_browser = self.clone();
        let dismiss = self.clone();
        let prompt_open =
            Signal::derive_local(move || !matches!(prompt_browser.0.prompt.get(), Prompt::Closed));
        let open_binding = Binding::controlled(prompt_open, move |open: bool| {
            if !open {
                dismiss.cancel_prompt();
            }
        });
        let body = self.clone();
        let root_focus = self.0.root_focus;
        let connector_view = self.0.connector.view();
        let menu_overlay = self.menu_overlay();

        AnyView::new(view! {
            KeytipHost {
                NotificationHost {
                    column(
                        class = "gds-files-root",
                        node_ref = root_focus,
                        tabindex = {Focus::Sequential},
                        on:key_down = move |event: &mut zgui::view::EventCx<'_, zgui::view::events::KeyDown>| {
                            keyboard.key_down(event);
                        },
                        on:pointer_down = move |event: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                            mouse.mouse_navigation(event);
                        },
                    ) {
                        {move || {
                            if browser.pane() == Pane::Help {
                                browser.help_view()
                            } else {
                                browser.workspace_view()
                            }
                        }}
                        {connector_view}
                        {menu_overlay}
                        Dialog(open = open_binding) {
                            DialogContent {
                                {render_prompt(body.clone())}
                            }
                        }
                    }
                }
            }
        })
    }

    fn help_view(&self) -> AnyView {
        let back = self.clone();
        let bar = Toolbar(
            Classes::default(),
            Attrs::default(),
            Children::from_view(view! {
                Button(
                    variant = ButtonVariant::Ghost,
                    on:click = move |_| back.back_page(),
                ) {{i18n::text("back")}}
            }),
        );
        AnyView::new(view! {
            Frame {
                {bar}
                {crate::widgets::help::view()}
            }
        })
    }

    fn workspace_view(&self) -> AnyView {
        let is_settings = {
            let browser = self.clone();
            move || browser.pane() == Pane::Settings
        };
        let browser = self.clone();
        let workspace_body = WorkspaceFrame(
            Classes::default(),
            Attrs::default(),
            Children::from_view(move || {
                if is_settings() {
                    browser.0.settings.content()
                } else {
                    browser.content_placeholder()
                }
            }),
        );
        let toolbar = self.toolbar();
        let footer = self.picker_footer();
        AnyView::new(view! {
            Frame {
                {toolbar}
                {workspace_body}
                {footer}
            }
        })
    }

    fn content_placeholder(&self) -> AnyView {
        let title = {
            let browser = self.clone();
            move || browser.read(|state| state.tab().navigation.location().display_address())
        };
        AnyView::new(view! {
            column(node_ref = self.0.content_focus, tabindex = {Focus::Sequential}) {
                text {{title}}
            }
        })
    }

    fn prompt_view(&self) -> AnyView {
        match self.0.prompt.get() {
            Prompt::Closed => AnyView::new(view! { box() }),
            Prompt::About => {
                let close = self.clone();
                AnyView::new(view! {
                    column {
                        DialogHeader {
                            DialogTitle {{i18n::text("about-app")}}
                        }
                        DialogFooter {
                            Button(
                                variant = ButtonVariant::Default,
                                on:click = move |_| close.cancel_prompt(),
                            ) {{i18n::text("dialog-close")}}
                        }
                    }
                })
            }
            Prompt::Name { form, submit, .. } => form.view(self, submit),
            Prompt::Confirm {
                title,
                message,
                primary,
                accept,
            } => {
                let cancel = self.clone();
                let confirm = self.clone();
                AnyView::new(view! {
                    column {
                        DialogHeader {
                            DialogTitle {{title}}
                        }
                        text {{message}}
                        DialogFooter {
                            Button(
                                variant = ButtonVariant::Outline,
                                on:click = move |_| cancel.cancel_prompt(),
                            ) {{i18n::text("dialog-cancel")}}
                            Button(
                                variant = ButtonVariant::Default,
                                on:click = move |_| {
                                    accept();
                                    confirm.cancel_prompt();
                                },
                            ) {{primary}}
                        }
                    }
                })
            }
        }
    }
}

fn render_prompt(browser: Browser) -> AnyView {
    AnyView::new(move || browser.prompt_view())
}
