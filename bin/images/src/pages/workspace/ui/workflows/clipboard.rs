//! System clipboard offer polling and PNG export copy using `ClipboardPlan`.

use gds::{i18n::t, ui::notifications::Level};
use gpui_kit::{Context, Window};

use super::*;
use crate::{
    entities::preferences::PreferencesExt as _,
    features::export::{build_clipboard_plan, execute_clipboard_blocking},
    shared::{clipboard::ClipboardExt as _, i18n::text},
};

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn refresh_clipboard_offer(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let task = cx.gds().clipboard().probe_offer();
        cx.spawn(async move |this, cx| {
            let offer = task.await;
            let _ = this.update(cx, |this, cx| {
                this.runtime.clipboard_offer = offer;
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::pages::workspace::ui) fn copy_to_clipboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let early_exit = self.runtime.launch.workflow.early_exit;
        let prefs = cx.gds().preferences().current();
        let plan = build_clipboard_plan(doc, self.session.metadata.as_ref(), &prefs);
        let task = self
            .tasks
            .spawn_blocking_named("Encode clipboard PNG", move || {
                execute_clipboard_blocking(plan)
            });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = cx.update(|window, cx| match result {
                Ok(Ok(png_bytes)) => match cx.gds().clipboard().write_png(png_bytes) {
                    Ok(()) => {
                        if early_exit {
                            window.remove_window();
                        } else {
                            let _ = this.update(cx, |_, cx| {
                                show_report::<CopyNotice>(
                                    Level::Success,
                                    text("toast-copy-success", cx),
                                    cx,
                                );
                            });
                        }
                    }
                    Err(err) => {
                        let _ = this.update(cx, |_, cx| {
                            show_report::<CopyNotice>(
                                Level::Error,
                                t!(@cx, "toast-copy-failed", error = err.to_string()),
                                cx,
                            );
                        });
                    }
                },
                Ok(Err(err)) => {
                    let _ = this.update(cx, |_, cx| {
                        show_report::<CopyNotice>(
                            Level::Error,
                            t!(@cx, "toast-copy-failed", error = err.to_string()),
                            cx,
                        );
                    });
                }
                Err(err) => {
                    let _ = this.update(cx, |_, cx| {
                        show_report::<CopyNotice>(
                            Level::Error,
                            t!(@cx, "toast-copy-failed", error = err.to_string()),
                            cx,
                        );
                    });
                }
            });
        })
        .detach();
    }
}
