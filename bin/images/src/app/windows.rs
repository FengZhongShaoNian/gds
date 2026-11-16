//! Top-level GPUI window creation, Wayland App ID binding, and notification layer wrapping.

use gds::{app::AppExt as _, service::task_manager::Scope};
use gpui_kit::{App, Bounds, WindowBounds, WindowKind, WindowOptions, prelude::*};

use super::config::{AppConfig, LaunchMode as ConfigLaunchMode};
use crate::{
    pages::workspace::{CaptureWorkflow, LaunchMode, LaunchOptions, WorkspacePage},
    shared::i18n,
};

/// Opens the primary `GDS Images` window for normal viewing or capture annotation.
///
/// # Errors
/// Returns an error if the platform window cannot be created.
pub(super) fn open_main_window(
    tasks: Scope,
    config: AppConfig,
    cx: &mut App,
) -> anyhow::Result<()> {
    let dimensions = gds::ui::window::fit_size(gds::ui::window::initial_size(), cx);
    let options = WindowOptions {
        window_min_size: Some(gds::ui::window::minimum_size()),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None, dimensions, cx,
        ))),
        app_id: Some(config.app_id.into()),
        kind: WindowKind::Normal,
        ..gds::ui::window::window_options()
    };

    let launch = launch_options(config);
    let is_capture = launch.is_capture;
    gpui_kit::open_window(options, cx, move |window, cx| {
        cx.gds_mut().appearance().theme().attach_observer(window);
        let title_key = if is_capture {
            "app-capture-title"
        } else {
            "app-title"
        };
        window.set_window_title(i18n::text(title_key, cx).as_ref());
        gds::ui::notifications::bind_window(window, cx);
        let page = cx.new(|cx| WorkspacePage::new(tasks, launch, window, cx));
        gds::ui::notifications::surface::wrap(page, window, cx)
    })?;
    Ok(())
}

fn launch_options(config: AppConfig) -> LaunchOptions {
    let mode = match config.mode {
        ConfigLaunchMode::EmptySmartState => LaunchMode::EmptySmartState,
        ConfigLaunchMode::OpenFile(path) => LaunchMode::OpenFile(path),
        ConfigLaunchMode::Clipboard => LaunchMode::Clipboard,
        ConfigLaunchMode::Stdin(bytes) => LaunchMode::Stdin(bytes),
    };
    let workflow = CaptureWorkflow {
        output: config.workflow.output,
        copy_on_save: config.workflow.copy_on_save,
        early_exit: config.workflow.early_exit,
    };
    LaunchOptions {
        mode,
        workflow,
        is_capture: config.is_capture,
    }
}
