//! Desktop shell integrations: Reveal in Files, Open With, and Move to Trash.

use std::path::PathBuf;

use gds::{
    i18n,
    service::{
        desktop::{Desktop, OpenMode},
        task_manager::Scope,
    },
};
use gpui_kit::{App, Task};

/// Reveals `path` inside the user's running desktop file manager (`org.freedesktop.FileManager1`).
pub fn reveal_in_files(path: PathBuf, scope: &Scope, cx: &mut App) {
    #[cfg(target_os = "linux")]
    {
        let task = scope.spawn_named("Reveal image in file manager", async move {
            gds::service::desktop::file_manager::show_items(&[path], "").await
        });
        cx.spawn(async move |cx| {
            if let Ok(Err(err)) = task.await {
                tracing::warn!(error = %err, "cannot reveal image in file manager");
                cx.update(|cx| {
                    let msg = i18n::t!(@cx, "error-reveal-failed", error = err.to_string());
                    gds::ui::notifications::error(msg, cx);
                });
            }
        })
        .detach();
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (path, scope, cx);
}

/// Opens the desktop application chooser dialog for `path`.
pub fn open_with_chooser(path: PathBuf, scope: &Scope, cx: &mut App) {
    let task = scope.spawn_named("Open image with chooser", async move {
        Desktop::default()
            .open_file(&path, OpenMode::ChooseApplication)
            .await
    });
    cx.spawn(async move |cx| {
        if let Ok(Err(err)) = task.await {
            tracing::warn!(error = %err, "open-with dialog failed");
            cx.update(|cx| {
                let msg = i18n::t!(@cx, "error-open-with-failed", error = err.to_string());
                gds::ui::notifications::error(msg, cx);
            });
        }
    })
    .detach();
}

/// Moves `path` cleanly to the freedesktop Trash using a supervised blocking task.
pub fn move_to_trash(path: PathBuf, scope: &Scope, cx: &App) -> Task<anyhow::Result<()>> {
    let task = scope.spawn_blocking_named("Trash image file", move || trash::delete(&path));
    cx.spawn(async move |_| {
        task.await?
            .map_err(|err| anyhow::anyhow!("move to trash failed: {err}"))
    })
}
