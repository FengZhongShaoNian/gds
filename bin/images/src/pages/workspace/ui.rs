//! Root `WorkspacePage` entity decomposed into 6 cohesive state slices and sub-modules.

mod chrome;
mod navigation;
mod surfaces;
#[cfg(all(test, feature = "gui-tests"))]
mod tests;
mod workflows;

use std::{cell::Cell, path::PathBuf, rc::Rc};

use gds::{
    app::AppExt as _,
    service::{image::source::Origin, task_manager::Scope},
    ui::{
        Frame,
        file_inspection::Cache as InspectionCache,
        notifications::{Level, Report},
        sidebar::{SidebarPresentation, SidebarState},
    },
};
use gds_uim::el;
use gpui_kit::{
    App, Bounds, Context, Entity, FocusHandle, Hsla, IntoElement, MouseButton, Pixels, Render,
    SharedString, Size, Subscription, Task, Window,
    component::{
        ActiveTheme as _,
        color_picker::{ColorPickerEvent, ColorPickerState},
        input::{InputEvent, InputState, TextareaState},
    },
    div,
    prelude::*,
    px,
};
use tokio_util::sync::CancellationToken;

use super::LaunchOptions;
use crate::{
    entities::{
        catalog::CatalogState,
        document::{DocumentState, RgbaColor, SpatialTransformOp},
        metadata::MetadataState,
        preferences::{DEFAULT_INSPECTOR_WIDTH, PreferencesExt as _},
    },
    features::{
        edit_canvas::{EditHistory, HoverTarget, ToolFsm, ToolMode},
        metadata_edit::MetadataCommitRequest,
    },
    shared::{
        clipboard::ClipboardOfferKind,
        geometry::{Point2, Size2, ViewportTransform},
        i18n::text,
        ui::actions::{SETTINGS_CONTEXT, WORKSPACE_CONTEXT},
    },
    widgets,
};

pub(crate) struct LoadNotice;
pub(crate) struct SaveNotice;
pub(crate) struct CopyNotice;
pub(crate) struct RenameNotice;
pub(crate) struct MetadataNotice;
pub(crate) struct SettingsNotice;
pub(crate) struct TrashNotice;
pub(crate) struct EditNotice;

type TransformOp = SpatialTransformOp;

struct RuntimeState {
    pub launch: LaunchOptions,
    pub inspections: Entity<InspectionCache>,
    pub is_loading: bool,
    pub load_error: Option<String>,
    pub loading_origin: Option<Origin>,
    pub is_saving: bool,
    pub in_flight_origin_op: Option<Origin>,
    pub clipboard_offer: Option<ClipboardOfferKind>,
    pub last_window_size: Size<Pixels>,
}

struct FocusState {
    pub canvas: FocusHandle,
    pub path_input: Entity<InputState>,
    pub text_overlay_input: Entity<TextareaState>,
    pub color_picker: Entity<ColorPickerState>,
}

struct ChromeState {
    pub filmstrip_expanded: bool,
    pub sidebar_presentation: SidebarPresentation,
    pub filmstrip_scroll_accum_y: f32,
    pub filmstrip_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub inspector_visible: bool,
    pub inspector_width: u16,
    pub inspector_resizing: bool,
    pub inspector_drag_anchor: Option<(f32, u16)>,
    pub app_menu_open: bool,
    pub app_menu_generation: u64,
    pub last_viewport_size: Size2,
}

struct CanvasState {
    pub transform: ViewportTransform,
    pub panning: bool,
    pub last_pointer: Option<Point2>,
    pub tool_fsm: ToolFsm,
    pub history: EditHistory,
    pub hover_target: HoverTarget,
    pub tool_popover: Option<ToolMode>,
    pub long_press_task: Option<Task<()>>,
    pub color_picker_open: bool,
    pub render_generation: u64,
    pub render_cancel: Option<CancellationToken>,
    pub gesture_generation: u64,
    pub gesture_render: Option<workflows::document_io::GestureRenderHandle>,
    pub svg_rerender_scale: Option<f32>,
    pub svg_rerender_generation: u64,
    pub svg_rerender_cancel: Option<CancellationToken>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            transform: ViewportTransform::default(),
            panning: false,
            last_pointer: None,
            tool_fsm: ToolFsm::default(),
            history: EditHistory::default(),
            hover_target: HoverTarget::None,
            tool_popover: None,
            long_press_task: None,
            color_picker_open: false,
            render_generation: 0,
            render_cancel: None,
            gesture_generation: 0,
            gesture_render: None,
            svg_rerender_scale: None,
            svg_rerender_generation: 0,
            svg_rerender_cancel: None,
        }
    }
}

struct SessionState {
    pub document: Option<DocumentState>,
    pub metadata: Option<MetadataState>,
    pub catalog: CatalogState,
    pub load_generation: u64,
    pub load_cancel: Option<CancellationToken>,
    pub scan_generation: u64,
    pub scan_cancel: Option<CancellationToken>,
    pub editing_path: bool,
    pub suppress_blur_commit: bool,
    pub clipboard_dismissed: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            document: None,
            metadata: None,
            catalog: CatalogState::empty(),
            load_generation: 0,
            load_cancel: None,
            scan_generation: 0,
            scan_cancel: None,
            editing_path: false,
            suppress_blur_commit: false,
            clipboard_dismissed: false,
        }
    }
}

#[derive(Default)]
struct SettingsState {
    pub open: bool,
    pub view: Option<Entity<widgets::settings::View>>,
    pub subscription: Option<Subscription>,
    pub applying_from_view: bool,
}

/// Interactive image viewing and editing workspace.
pub struct WorkspacePage {
    tasks: Scope,
    runtime: RuntimeState,
    focus: FocusState,
    chrome: ChromeState,
    canvas: CanvasState,
    session: SessionState,
    settings: SettingsState,
    _subscriptions: Vec<Subscription>,
}

impl WorkspacePage {
    pub fn new(
        tasks: Scope,
        launch: LaunchOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prefs = cx.gds().preferences().current();
        let canvas_focus = cx.focus_handle();
        let inspections = cx.new(|_| InspectionCache::new(Default::default()));

        let path_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(text("path-placeholder", cx)));
        let text_overlay_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 6)
                .submit_on_enter(true)
                .placeholder(text("tool-text", cx))
        });
        let initial_hsla = color_to_hsla(ToolFsm::default().settings.color);
        let color_picker =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(initial_hsla));

        let mut subscriptions = Vec::with_capacity(8);
        subscriptions.push(cx.subscribe_in(
            &path_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_path_edit(window, cx),
                InputEvent::Blur => {
                    if this.session.suppress_blur_commit {
                        this.session.suppress_blur_commit = false;
                    } else if this.session.editing_path {
                        this.commit_path_edit(window, cx);
                    }
                }
                _ => {}
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &text_overlay_input,
            window,
            |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => {
                    this.commit_text_overlay(window, cx);
                }
                InputEvent::Change => {
                    let value = input.read(cx).value().to_string();
                    this.on_text_draft_changed(value, cx);
                }
                _ => {}
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &color_picker,
            window,
            |this, _, event: &ColorPickerEvent, _, cx| {
                if let ColorPickerEvent::Change(Some(hsla)) = event {
                    this.set_tool_color(hsla_to_rgba(*hsla), cx);
                }
            },
        ));

        let pref_service = cx.gds().preferences().service();
        subscriptions.push(cx.observe_in(&pref_service, window, |this, _, window, cx| {
            let prefs = cx.gds().preferences().current();
            this.sync_preferences(&prefs, window, cx);
        }));

        subscriptions.push(cx.observe_window_bounds(window, |this, window, cx| {
            let size = window.bounds().size;
            if size != this.runtime.last_window_size {
                this.runtime.last_window_size = size;
                cx.notify();
            }
        }));
        subscriptions.push(cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.on_canvas_focus_lost(cx);
            }
        }));
        subscriptions.push(cx.on_blur(&canvas_focus, window, |this, _, cx| {
            this.on_canvas_focus_lost(cx);
        }));

        cx.on_release(|this, cx| {
            if let Some(cancel) = this.session.load_cancel.take() {
                cancel.cancel();
            }
            if let Some(cancel) = this.session.scan_cancel.take() {
                cancel.cancel();
            }
            if let Some(cancel) = this.canvas.render_cancel.take() {
                cancel.cancel();
            }
            this.cancel_gesture_render();
            this.cancel_svg_rerender();
            if let Some(doc) = this.session.document.as_mut() {
                doc.cache.release_textures(cx);
            }
        })
        .detach();

        let last_window_size = window.bounds().size;
        let initial_mode = launch.mode.clone();

        let mut page = Self {
            tasks,
            runtime: RuntimeState {
                launch,
                inspections,
                is_loading: false,
                load_error: None,
                loading_origin: None,
                is_saving: false,
                in_flight_origin_op: None,
                clipboard_offer: None,
                last_window_size,
            },
            focus: FocusState {
                canvas: canvas_focus,
                path_input,
                text_overlay_input,
                color_picker,
            },
            chrome: ChromeState {
                filmstrip_expanded: prefs.filmstrip_open,
                sidebar_presentation: SidebarPresentation::default(),
                filmstrip_scroll_accum_y: 0.0,
                filmstrip_bounds: Rc::new(Cell::new(Bounds::default())),
                inspector_visible: prefs.inspector_open,
                inspector_width: prefs.effective_inspector_width(),
                inspector_resizing: false,
                inspector_drag_anchor: None,
                app_menu_open: false,
                app_menu_generation: 0,
                last_viewport_size: Size2::new(800.0, 600.0),
            },
            canvas: CanvasState::default(),
            session: SessionState::default(),
            settings: SettingsState::default(),
            _subscriptions: subscriptions,
        };

        page.focus.canvas.focus(window, cx);
        page.start_initial_launch(initial_mode, window, cx);
        page
    }

    fn start_initial_launch(
        &mut self,
        mode: super::LaunchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match mode {
            super::LaunchMode::EmptySmartState => {
                self.refresh_clipboard_offer(cx);
            }
            super::LaunchMode::OpenFile(path) => {
                self.open_origin(Origin::LocalFile { path, stamp: None }, window, cx);
            }
            super::LaunchMode::Clipboard => {
                self.open_from_clipboard(window, cx);
            }
            super::LaunchMode::Stdin(bytes) => {
                let origin = Origin::Memory {
                    kind: gds::service::image::source::MemoryOrigin::Stdin,
                    bytes,
                    format_hint: None,
                };
                self.open_origin(origin, window, cx);
            }
        }
    }

    fn sync_computed_viewport(&mut self, viewport: Size2) {
        self.chrome.last_viewport_size = viewport;
        if !self.canvas.transform.scale.is_finite() || self.canvas.transform.scale <= 0.0 {
            self.canvas.transform.set_scale_centered(1.0, viewport);
        }
        if let Some(doc) = &self.session.document {
            let (w, h) = doc.output_dimensions();
            self.canvas
                .transform
                .recenter_if_fitted(viewport, Size2::new(w as f32, h as f32));
        }
    }

    fn persist_chrome_preferences(&mut self, cx: &mut Context<Self>) {
        let filmstrip_open = self.chrome.filmstrip_expanded;
        let inspector_open = self.chrome.inspector_visible;
        let inspector_width = self.chrome.inspector_width;
        let result = cx.gds_mut().preferences().edit(|prefs| {
            prefs.filmstrip_open = filmstrip_open;
            prefs.inspector_open = inspector_open;
            prefs.inspector_panel_width = if inspector_width == DEFAULT_INSPECTOR_WIDTH {
                None
            } else {
                Some(inspector_width)
            };
        });
        if let Err(error) = result {
            self.report_settings_save_error(&error.to_string(), cx);
        }
    }

    fn is_origin_busy(&self, origin: &Origin) -> bool {
        self.runtime.is_saving
            || self
                .runtime
                .in_flight_origin_op
                .as_ref()
                .is_some_and(|active| active == origin)
    }

    fn base_window_title(&self, cx: &App) -> SharedString {
        let key = if self.runtime.launch.is_capture {
            "app-capture-title"
        } else {
            "app-title"
        };
        text(key, cx)
    }

    fn sync_window_title(&self, window: &mut Window, cx: &App) {
        let base = self.base_window_title(cx);
        let title = match self
            .session
            .document
            .as_ref()
            .map(|doc| &doc.decoded.origin)
        {
            Some(origin) => {
                let name = origin_display_name(origin);
                let dirty_prefix = if self.has_unsaved_changes() {
                    "• "
                } else {
                    ""
                };
                format!("{dirty_prefix}{name} — {base}")
            }
            None => base.to_string(),
        };
        window.set_window_title(&title);
    }

    fn has_unsaved_changes(&self) -> bool {
        let doc_dirty = self.session.document.as_ref().is_some_and(|d| d.is_dirty());
        let meta_dirty = self.session.metadata.as_ref().is_some_and(|m| m.is_dirty());
        doc_dirty || meta_dirty
    }

    fn sidebar_state(&self) -> SidebarState {
        if self.chrome.filmstrip_expanded {
            SidebarState::Expanded
        } else {
            SidebarState::Collapsed
        }
    }

    fn has_sidebar(&self) -> bool {
        !self.runtime.launch.is_capture
            || !self.session.catalog.entries.is_empty()
            || self.chrome.filmstrip_expanded
            || self.settings.open
    }

    pub(in crate::pages::workspace::ui) fn set_app_menu_open(
        &mut self,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        if self.chrome.app_menu_open == open {
            return;
        }
        self.chrome.app_menu_open = open;
        if !open {
            self.chrome.app_menu_generation = self.chrome.app_menu_generation.wrapping_add(1);
        }
        cx.notify();
    }

    fn on_drag_release_capture(
        &mut self,
        _: &gpui_kit::MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_inspector_resize(cx);
    }
}

impl Render for WorkspacePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let win_w = f32::from(window.bounds().size.width);
        let win_h = f32::from(window.bounds().size.height);

        let sidebar_state = self.sidebar_state();
        let prev_presentation = self.chrome.sidebar_presentation;
        let sidebar_pres = chrome::motion::sidebar_presentation(sidebar_state, window, cx);
        self.chrome.sidebar_presentation = sidebar_pres;
        if sidebar_pres != prev_presentation
            && let Some(view) = &self.settings.view
        {
            view.update(cx, |settings, cx| {
                settings.set_sidebar_presentation(sidebar_state, sidebar_pres, cx);
            });
        }

        let has_sidebar = self.has_sidebar();
        let left_w = if has_sidebar {
            sidebar_pres.width()
        } else {
            0.0
        };
        let frame = if has_sidebar {
            Frame::default().sidebar_width(left_w)
        } else {
            Frame::without_sidebar()
        };

        if self.settings.open {
            let toolbar = self.render_toolbar(&frame, window, cx);
            let settings_body = self.settings.view.clone();
            let footer = self.render_footer(&frame, cx);
            let root = el! {
                .div {
                    # "images-workspace";
                    key_context: SETTINGS_CONTEXT;
                    flex; flex_col; size_full; min_h_0; min_w_0; overflow_hidden;
                    text_color: $(cx.theme().foreground);
                    on_action: $(cx.listener(Self::on_close_settings_action));
                    on_action: $(cx.listener(Self::on_toggle_filmstrip_action));
                    on_action: $(cx.listener(Self::on_quit_app_action));
                } [
                    toolbar,
                    .div { flex_1; min_h_0; min_w_0; flex; flex_col; } [
                        @if (let Some(view) = settings_body)
                            view,
                        @end,
                    ],
                    footer,
                ]
            };
            return root.into_any_element();
        }

        let target_insp_w =
            chrome::motion::effective_inspector_width(win_w, left_w, self.chrome.inspector_width);
        let insp_w = chrome::motion::inspector_presentation(
            self.chrome.inspector_visible,
            self.chrome.inspector_resizing,
            target_insp_w,
            window,
            cx,
        );
        let viewport_size = chrome::motion::compute_viewport_size(win_w, win_h, left_w, insp_w);
        self.sync_computed_viewport(viewport_size);

        let toolbar = self.render_toolbar(&frame, window, cx);
        let sidebar = if has_sidebar {
            self.render_filmstrip(sidebar_pres, window, cx)
                .into_any_element()
        } else {
            el! { .div }.into_any_element()
        };
        let viewport = self.render_viewport(viewport_size, window, cx);
        let inspector = self.render_inspector(insp_w, window, cx);
        let footer = self.render_footer(&frame, cx);

        let content_area = el! {
            .div { size_full; flex; flex_row; min_w_0; min_h_0; } [
                .div { flex_1; min_w_0; min_h_0; h_full; relative; } [viewport],
                @if (insp_w > 0.5) inspector, @end,
            ]
        };
        let workspace = frame.workspace(sidebar, content_area.into_any_element(), cx);

        let capturing_resize =
            self.chrome.inspector_resizing || self.chrome.inspector_drag_anchor.is_some();

        let root = el! {
            .div {
                # "images-workspace";
                key_context: WORKSPACE_CONTEXT;
                flex; flex_col; size_full; min_h_0; min_w_0; overflow_hidden; relative;
                text_color: $(cx.theme().foreground);
            } [
                toolbar,
                workspace,
                footer,
                @if (capturing_resize)
                    .div {
                        # "inspector-drag-capture";
                        absolute; inset_0;
                        cursor_col_resize;
                        on_mouse_move: $(cx.listener(Self::on_inspector_drag_move));
                        on_mouse_up: MouseButton::Left, $(cx.listener(Self::on_drag_release_capture));
                        on_mouse_up_out: MouseButton::Left, $(cx.listener(Self::on_drag_release_capture));
                    },
                @end,
            ]
        };

        self.bind_workspace_actions(root, cx).into_any_element()
    }
}

fn show_report<Slot: 'static>(level: Level, summary: impl Into<SharedString>, cx: &mut App) {
    Report::new(level, summary).replacing::<Slot>().show(cx);
}

fn origin_display_name(origin: &Origin) -> String {
    origin.display_name()
}

fn color_to_hsla(color: RgbaColor) -> Hsla {
    gpui_kit::Rgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: f32::from(color.a) / 255.0,
    }
    .into()
}

fn hsla_to_rgba(hsla: Hsla) -> RgbaColor {
    let rgba: gpui_kit::Rgba = hsla.into();
    RgbaColor {
        r: (rgba.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        g: (rgba.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        b: (rgba.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        a: (rgba.a.clamp(0.0, 1.0) * 255.0).round() as u8,
    }
}

fn read_file_meta(path: &std::path::Path) -> (u64, Option<std::time::SystemTime>) {
    match std::fs::metadata(path) {
        Ok(meta) => (meta.len(), meta.modified().ok()),
        Err(_) => (0, None),
    }
}

fn build_metadata_request(
    path: PathBuf,
    doc: &DocumentState,
    meta: &MetadataState,
    strip_privacy: bool,
) -> MetadataCommitRequest {
    let expected_stamp = match &doc.decoded.origin {
        Origin::LocalFile { stamp, .. } => stamp.clone(),
        Origin::Memory { .. } => None,
    };
    let edits = if strip_privacy {
        None
    } else if meta.metadata_dirty {
        Some(meta.draft.normalized())
    } else {
        None
    };
    MetadataCommitRequest {
        path,
        expected_stamp,
        format: doc.format(),
        source_bytes: std::sync::Arc::clone(&doc.decoded.source_bytes),
        edits,
        strip_privacy,
        original_tags: meta.original_tags.clone(),
        edited_tags: meta.tags.clone(),
    }
}
