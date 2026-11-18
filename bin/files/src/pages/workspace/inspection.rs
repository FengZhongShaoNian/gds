//! Properties, content inspection, permissions, and tags dialogs share existing services.

mod info;
pub(super) mod tags;

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use gds::app::Gds;
use gds::service::file_inspection::{FileFacts, Metadata, Request};
use gds::service::file_tags;
use gds::service::task_manager::Scope;
use gds::ui::button::{ButtonSize, ButtonVariant};
use gds::ui::capacity::CapacityBarProps;
use gds::ui::file_inspection::{Cache, Options, State as InspectionState};
use gds::ui::frame::FrameProps;
use gds::ui::thumbnail::ThumbnailProps;
use gds::ui::time::Timestamp;
use gds::ui::window::{TitleBarProps, workspace_options};
use gds::ui::{IconName, IconProps};
use gds::{i18n, t};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, Signal, on_cleanup_local, spawn_local};
use zgui::runtime::{WindowHandle, try_use_windows};
use zgui::view::{current_host, install_stylesheet};
use zgui::{css, view};
use zgui_ui::{
    ButtonProps, CheckboxProps, Checked, DialogContentProps, DialogDescriptionProps,
    DialogFooterProps, DialogHeaderProps, DialogProps, DialogTitleProps, LabelProps, SpinnerProps,
};
use zgui_ui_primitives::Binding;

use crate::entities::file::{
    directory::{Entry, EntryKind},
    ui::{EntryPresentation, FileIcon, UNKNOWN_VALUE},
};
use crate::entities::location::{Location, volumes};
use crate::features::file_operations::permissions;

use super::{
    model::{Contents, Row},
    ui::{Browser, WeakBrowser},
};
use tags::TagEditor;

const INSPECTION_SHEET: &str = "gds-files-workspace-inspection";

const INSPECTION_CSS: &str = css!(
    r#"
    .gds-files-artwork-slot {
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        position: relative;
        overflow: hidden;
        border-radius: var(--gds-radius-control, 6px);
    }

    .gds-files-details {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0;
        min-height: 0;
        overflow-y: auto;
        padding: 14px;
        gap: 14px;
        background: var(--gds-panel);
        border-left: 1px solid var(--gds-border);
    }

    .gds-files-details__empty {
        flex: 1 1 0%;
        align-items: center;
        justify-content: center;
        gap: 10px;
        padding: 20px;
        background: var(--gds-panel);
        border-left: 1px solid var(--gds-border);
    }

    .gds-files-details__empty-icon {
        width: 28px;
        height: 28px;
        color: var(--gds-muted-foreground);
    }

    .gds-files-details__empty-text {
        font-size: var(--gds-font-size-sm, 14px);
        color: var(--gds-muted-foreground);
        text-align: center;
    }

    .gds-files-details__hero {
        align-items: center;
        gap: 10px;
        padding-bottom: 12px;
        border-bottom: 1px solid var(--gds-border);
    }

    .gds-files-details__thumb {
        width: 144px;
        height: 144px;
        border-radius: var(--gds-radius-control, 6px);
        overflow: hidden;
    }

    .gds-files-details__title {
        font-size: var(--gds-font-size-md, 16px);
        font-weight: 600;
        color: var(--gds-foreground);
        text-align: center;
        overflow-wrap: anywhere;
    }

    .gds-files-details__subtitle {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
        text-align: center;
        word-break: break-all;
    }

    .gds-files-details__rows {
        gap: 8px;
    }

    .gds-files-details__row {
        display: flex;
        flex-direction: row;
        align-items: baseline;
        justify-content: space-between;
        gap: 12px;
    }

    .gds-files-details__label {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
        flex-shrink: 0;
    }

    .gds-files-details__value {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-foreground);
        text-align: right;
        min-width: 0;
        overflow-wrap: anywhere;
    }

    .gds-files-details__tags-section {
        gap: 8px;
        padding-top: 10px;
        border-top: 1px solid var(--gds-border);
    }

    .gds-files-details__tags-header {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
    }

    .gds-files-details__tags-list {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        gap: 6px;
    }

    .gds-files-details__tag-chip {
        padding: 2px 8px;
        border-radius: 999px;
        font-size: var(--gds-font-size-xs, 12px);
        background: var(--gds-surface);
        border: 1px solid var(--gds-border);
        color: var(--gds-foreground);
    }

    .gds-files-details__actions {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        gap: 8px;
        padding-top: 10px;
        border-top: 1px solid var(--gds-border);
    }

    .gds-files-properties-window {
        flex: 1 1 0%;
        min-height: 0;
        overflow-y: auto;
        padding: 16px;
        gap: 14px;
    }

    .gds-files-dialog-body {
        gap: 12px;
    }

    .gds-files-permissions-grid {
        gap: 10px;
    }

    .gds-files-permissions-group {
        gap: 6px;
        padding: 10px;
        border-radius: var(--gds-radius-control, 6px);
        background: var(--gds-surface);
        border: 1px solid var(--gds-border);
    }

    .gds-files-permissions-group__title {
        font-size: var(--gds-font-size-xs, 12px);
        font-weight: 600;
        color: var(--gds-foreground);
    }

    .gds-files-permissions-group__checks {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        gap: 14px;
    }

    .gds-files-permissions-check {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 6px;
    }

    .gds-files-permissions-note {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
    }

    .gds-files-permissions-error {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-error);
    }
    "#
);

fn install_inspection_stylesheet() {
    if current_host().is_some() {
        install_stylesheet(INSPECTION_SHEET, INSPECTION_CSS);
    }
}

#[derive(Clone)]
struct DialogSlots {
    browser: WeakBrowser,
    tags: RwSignal<Option<TagEditor>, LocalStorage>,
    permissions: RwSignal<Option<PermissionEditor>, LocalStorage>,
}

thread_local! {
    static DIALOGS: RefCell<HashMap<usize, DialogSlots>> = RefCell::new(HashMap::new());
    static PROPERTIES_WINDOWS: RefCell<HashMap<PathBuf, WindowHandle>> = RefCell::new(HashMap::new());
}

fn dialog_slots(browser: &Browser) -> DialogSlots {
    let key = Rc::as_ptr(&browser.0) as usize;
    if let Some(existing) = DIALOGS.with(|map| {
        map.borrow().get(&key).and_then(|slots| {
            slots
                .browser
                .upgrade()
                .filter(|b| Rc::ptr_eq(&b.0, &browser.0))
                .map(|_| slots.clone())
        })
    }) {
        return existing;
    }

    let created = browser.0.owner.with(|| {
        on_cleanup_local(move || {
            DIALOGS.with(|map| {
                map.borrow_mut().remove(&key);
            });
        });
        DialogSlots {
            browser: browser.downgrade(),
            tags: RwSignal::new_local(None),
            permissions: RwSignal::new_local(None),
        }
    });
    DIALOGS.with(|map| {
        map.borrow_mut().insert(key, created.clone());
    });
    created
}

fn metadata_row(label_key: &'static str, value: AnyView) -> AnyView {
    AnyView::new(view! {
        row(class = "gds-files-details__row") {
            text(class = "gds-files-details__label") {{i18n::text(label_key)}}
            box(class = "gds-files-details__value") {
                {value}
            }
        }
    })
}

fn text_row(label_key: &'static str, value: String) -> AnyView {
    metadata_row(
        label_key,
        AnyView::new(view! {
            text(class = "gds-files-details__value") {{value}}
        }),
    )
}

fn file_artwork_view(icon: FileIcon, size: f32) -> AnyView {
    let badge = icon.badge().map(|badge_icon| {
        let badge_size = (size * 0.45).max(10.0);
        view! {
            box(style:position = "absolute", style:right = "0px", style:bottom = "0px") {
                Icon(name = badge_icon, size = badge_size)
            }
        }
        .into_view()
    });
    AnyView::new(view! {
        box(
            class = "gds-files-artwork-slot",
            style:width = format!("{size:.1}px"),
            style:height = format!("{size:.1}px"),
        ) {
            Icon(name = icon.base(), size = size) {}
            {badge}
        }
    })
}

#[derive(Clone)]
enum EntryState {
    Loading,
    Ready(Entry),
    Failed(String),
}

#[derive(Clone)]
struct PropertiesInner {
    path: PathBuf,
    scope: Scope,
    cache: Cache,
    entry: RwSignal<EntryState, LocalStorage>,
    tags: RwSignal<Option<Result<Vec<String>, String>>, LocalStorage>,
    tags_enabled: bool,
}

#[derive(Clone)]
pub(crate) struct Properties(Rc<PropertiesInner>);

impl Properties {
    pub(crate) fn new(path: PathBuf, scope: Scope) -> Self {
        let cache = Cache::with_scope(scope.clone(), Options::default());
        let inner = Rc::new(PropertiesInner {
            path,
            scope,
            cache,
            entry: RwSignal::new_local(EntryState::Loading),
            tags: RwSignal::new_local(None),
            tags_enabled: true,
        });
        let props = Self(inner);
        props.reload();
        props
    }

    fn from_entry(entry: Entry, cache: Cache, scope: Scope, tags_enabled: bool) -> Self {
        let path = entry.path.clone();
        let inner = Rc::new(PropertiesInner {
            path: path.clone(),
            scope,
            cache,
            entry: RwSignal::new_local(EntryState::Ready(entry)),
            tags: RwSignal::new_local(None),
            tags_enabled,
        });
        let props = Self(inner);
        if tags_enabled {
            props.load_tags();
        }
        props
    }

    pub(crate) fn open(path: PathBuf, scope: Scope) {
        let existing = PROPERTIES_WINDOWS.with(|windows| {
            let mut map = windows.borrow_mut();
            if let Some(handle) = map.get(&path)
                && handle.is_open()
            {
                return Some(handle.clone());
            }
            map.remove(&path);
            None
        });
        if let Some(handle) = existing {
            handle.focus();
            return;
        }

        let Some(windows) = try_use_windows() else {
            return;
        };

        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| path.display().to_string());
        let window_path = path.clone();
        let options = workspace_options(title.clone(), None)
            .with_size(460.0, 620.0)
            .with_min_size(360.0, 420.0);

        let handle = windows.open(options, move || {
            let _ = Gds::current().appearance().install_window_sheet();
            let props = Properties::new(window_path.clone(), scope.clone());
            let title_text = title.clone();
            view! {
                Frame {
                    TitleBar(title = title_text)
                    column(class = "gds-files-properties-window") {
                        {props.view(None, false)}
                    }
                }
            }
        });

        PROPERTIES_WINDOWS.with(|windows| {
            windows.borrow_mut().insert(path, handle);
        });
    }

    fn reload(&self) {
        self.0.entry.set(EntryState::Loading);
        let weak = Rc::downgrade(&self.0);
        let path = self.0.path.clone();
        let task = self
            .0
            .scope
            .spawn_blocking_named("Inspect item properties", move || {
                Entry::inspect(&path).map_err(|err| err.to_string())
            });
        drop(spawn_local(async move {
            let result = task
                .await
                .map_err(|err| err.to_string())
                .and_then(std::convert::identity);
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let target = Self(inner);
            match result {
                Ok(entry) => {
                    target.0.entry.set(EntryState::Ready(entry));
                    if target.0.tags_enabled {
                        target.load_tags();
                    }
                }
                Err(message) => {
                    target.0.entry.set(EntryState::Failed(message));
                }
            }
        }));
    }

    fn load_tags(&self) {
        let weak = Rc::downgrade(&self.0);
        let path = self.0.path.clone();
        let task = self
            .0
            .scope
            .spawn_blocking_named("Read properties tags", move || {
                file_tags::read(&path).map_err(|err| err.to_string())
            });
        drop(spawn_local(async move {
            let result = task
                .await
                .map_err(|err| err.to_string())
                .and_then(std::convert::identity);
            let Some(inner) = weak.upgrade() else {
                return;
            };
            inner.tags.set(Some(result));
        }));
    }

    fn format_label<'a>(entry: &'a Entry, metadata: Option<&'a Metadata>) -> Option<String> {
        match metadata {
            Some(Metadata::File(FileFacts::Image(img))) => Some(img.format.label().to_owned()),
            Some(Metadata::File(FileFacts::Audio(audio))) => Some(audio.format.label().to_owned()),
            Some(Metadata::File(FileFacts::Video(video))) => Some(video.format.label().to_owned()),
            Some(Metadata::File(FileFacts::Text(_))) => Some(i18n::text("content-text")),
            _ => {
                let detailed = EntryPresentation::detailed_type(entry);
                (!detailed.is_empty() && detailed != UNKNOWN_VALUE).then_some(detailed)
            }
        }
    }

    fn base_rows(&self, entry: &Entry, metadata: Option<&Metadata>) -> Vec<AnyView> {
        let mut rows = Vec::new();
        rows.push(text_row("type", EntryPresentation::type_label(entry)));
        if let Some(format) = Self::format_label(entry, metadata) {
            rows.push(text_row("details-format", format));
        }
        rows.push(text_row("size", EntryPresentation::size_label(entry)));
        rows.push(metadata_row(
            "modified",
            AnyView::new(view! { {Timestamp::new(entry.modified)} }),
        ));
        if entry.details.accessed.is_some() {
            rows.push(metadata_row(
                "accessed",
                AnyView::new(view! { {Timestamp::new(entry.details.accessed)} }),
            ));
        }
        if entry.details.created.is_some() {
            rows.push(metadata_row(
                "details-created",
                AnyView::new(view! { {Timestamp::new(entry.details.created)} }),
            ));
        }
        let owner = EntryPresentation::owner(entry);
        rows.push(text_row("owner", owner.text));
        rows.push(text_row("group", EntryPresentation::group(entry)));
        if let Some(perms) = entry.details.permissions.as_ref() {
            rows.push(text_row("permissions", info::permissions(perms)));
        }
        rows
    }

    fn fact_rows(metadata: &Metadata) -> Vec<AnyView> {
        let mut rows = Vec::new();
        match metadata {
            Metadata::File(FileFacts::Image(image)) => {
                if let Some(size) = image.size {
                    rows.push(text_row("details-dimensions", info::dimensions(size)));
                    if let Some(aspect) = info::aspect_ratio(size) {
                        rows.push(text_row("details-aspect", aspect));
                    }
                }
                if let Some(depth) = image.bit_depth {
                    rows.push(text_row("details-bit-depth", depth.to_string()));
                }
                if let Some(alpha) = image.alpha {
                    rows.push(text_row(
                        "details-alpha",
                        i18n::text(if alpha { "details-yes" } else { "details-no" }),
                    ));
                }
            }
            Metadata::File(FileFacts::Audio(audio)) => {
                if let Some(duration) = audio.duration {
                    rows.push(text_row("details-duration", info::duration(duration)));
                }
                if let Some(rate) = audio.sample_rate {
                    rows.push(text_row("details-sample-rate", info::sample_rate(rate)));
                }
                if let Some(channels) = audio.channels {
                    rows.push(text_row("details-channels", channels.to_string()));
                }
                if let Some(depth) = audio.bit_depth {
                    rows.push(text_row("details-bit-depth", depth.to_string()));
                }
                if let Some(bitrate) = audio.bitrate {
                    rows.push(text_row("details-bitrate", info::bitrate(bitrate)));
                }
                if let Some(title) = audio.tags.title.as_ref() {
                    rows.push(text_row("details-title", title.clone()));
                }
                if let Some(artist) = audio.tags.artist.as_ref() {
                    rows.push(text_row("details-artist", artist.clone()));
                }
                if let Some(album) = audio.tags.album.as_ref() {
                    rows.push(text_row("details-album", album.clone()));
                }
                if let Some(genre) = audio.tags.genre.as_ref() {
                    rows.push(text_row("details-genre", genre.clone()));
                }
                if let Some(year) = audio.tags.year {
                    rows.push(text_row("details-year", year.to_string()));
                }
                if let Some(track) = audio.tags.track {
                    rows.push(text_row("details-track", track.to_string()));
                }
            }
            Metadata::File(FileFacts::Video(video)) => {
                if let Some(duration) = video.duration {
                    rows.push(text_row("details-duration", info::duration(duration)));
                }
                if let Some(dims) = video.dimensions {
                    rows.push(text_row("details-dimensions", info::dimensions(dims)));
                    if let Some(aspect) = info::aspect_ratio(dims) {
                        rows.push(text_row("details-aspect", aspect));
                    }
                }
                if let Some(codec) = video.codec.as_ref() {
                    rows.push(text_row("details-codec", codec.clone()));
                }
            }
            Metadata::File(FileFacts::Text(text)) => {
                rows.push(text_row(
                    "details-encoding",
                    text.encoding.label().to_owned(),
                ));
                if let Some(endings) = text.line_endings {
                    let label = match info::line_endings(endings) {
                        info::LineEndingLabel::Literal(value) => value.to_owned(),
                        info::LineEndingLabel::Message(key) => i18n::text(key),
                    };
                    rows.push(text_row("details-line-endings", label));
                }
            }
            Metadata::File(FileFacts::Unknown) => {}
            Metadata::Directory(dir) => {
                rows.push(text_row(
                    "details-folders",
                    info::count(dir.folders, dir.capped),
                ));
                rows.push(text_row(
                    "details-files",
                    info::count(dir.files, dir.capped),
                ));
                rows.push(text_row(
                    "details-links",
                    info::count(dir.links, dir.capped),
                ));
            }
            Metadata::Symlink(link) => {
                rows.push(text_row(
                    "details-target",
                    link.target.display().to_string(),
                ));
            }
        }
        rows
    }

    fn tags_section(&self, browser: Option<&Browser>) -> Option<AnyView> {
        if !self.0.tags_enabled {
            return None;
        }
        let tags_sig = self.0.tags;
        let path = self.0.path.clone();
        let edit_browser = browser.cloned();

        Some(AnyView::new(view! {
            column(class = "gds-files-details__tags-section") {
                row(class = "gds-files-details__tags-header") {
                    text(class = "gds-files-details__label") {{i18n::text("tags-title")}}
                    {edit_browser.map(|b| {
                        let edit_path = path.clone();
                        view! {
                            Button(
                                variant = ButtonVariant::Ghost,
                                size = ButtonSize::Sm,
                                on:click = move |_| b.edit_tags(edit_path.clone()),
                            ) {{i18n::text("tags-edit")}}
                        }
                        .into_view()
                    })}
                }
                {move || match tags_sig.get() {
                    None => AnyView::new(view! {
                        text(class = "gds-files-details__label") {{i18n::text("tags-loading")}}
                    }),
                    Some(Err(err)) => AnyView::new(view! {
                        text(class = "gds-files-permissions-error") {{err}}
                    }),
                    Some(Ok(list)) if list.is_empty() => AnyView::new(view! {
                        text(class = "gds-files-details__label") {{i18n::text("tags-none")}}
                    }),
                    Some(Ok(list)) => {
                        let chips: Vec<AnyView> = list
                            .into_iter()
                            .map(|tag| {
                                AnyView::new(view! {
                                    text(class = "gds-files-details__tag-chip") {{tag}}
                                })
                            })
                            .collect();
                        AnyView::new(view! {
                            row(class = "gds-files-details__tags-list") {
                                {chips}
                            }
                        })
                    }
                }}
            }
        }))
    }

    fn view(&self, browser: Option<Browser>, panel_chrome: bool) -> AnyView {
        install_inspection_stylesheet();
        let entry_sig = self.0.entry;
        let props = self.clone();
        let class_name = if panel_chrome {
            "gds-files-details"
        } else {
            "gds-files-properties-window"
        };

        AnyView::new(view! {
            column(class = class_name) {
                {move || match entry_sig.get() {
                    EntryState::Loading => {
                        AnyView::new(view! {
                            column(class = "gds-files-collection-state") {
                                Spinner(label = i18n::text("loading"))
                                text(class = "gds-files-details__empty-text") {{i18n::text("loading")}}
                            }
                        })
                    }
                    EntryState::Failed(message) => {
                        let retry_props = props.clone();
                        AnyView::new(view! {
                            column(class = "gds-files-collection-state") {
                                Icon(name = IconName::CircleAlert, class = "gds-files-details__empty-icon")
                                text(class = "gds-files-details__empty-text") {{message}}
                                Button(
                                    variant = ButtonVariant::Outline,
                                    on:click = move |_| retry_props.reload(),
                                ) {{i18n::text("retry")}}
                            }
                        })
                    }
                    EntryState::Ready(entry) => {
                        let source = entry.inspection_source();
                        let snap = props.0.cache.request(&source, Request::Metadata);
                        let metadata_ref = match &snap.metadata {
                            InspectionState::Ready(meta) => Some(meta.as_ref()),
                            _ => None,
                        };
                        let hero_visual = if let Some(thumb_src) = entry.thumbnail_source() {
                            let _ = props.0.cache.request(&thumb_src, Request::Thumbnail);
                            AnyView::new(view! {
                                Thumbnail(
                                    cache = props.0.cache.clone(),
                                    source = thumb_src,
                                    show_status = true,
                                    class = "gds-files-details__thumb",
                                )
                            })
                        } else {
                            file_artwork_view(FileIcon::for_entry(&entry), 64.0)
                        };

                        let mut all_rows = props.base_rows(&entry, metadata_ref);
                        if let Some(meta) = metadata_ref {
                            all_rows.extend(Self::fact_rows(meta));
                        }

                        let tags_view = props.tags_section(browser.as_ref());
                        let perm_button = browser.as_ref().map(|b| {
                            let b = b.clone();
                            let target_entry = entry.clone();
                            view! {
                                Button(
                                    variant = ButtonVariant::Outline,
                                    size = ButtonSize::Sm,
                                    on:click = move |_| b.edit_permissions(target_entry.clone()),
                                ) {{i18n::text("permissions")}}
                            }
                            .into_view()
                        });

                        AnyView::new(view! {
                            column(style:gap = "12px") {
                                column(class = "gds-files-details__hero") {
                                    {hero_visual}
                                    text(class = "gds-files-details__title") {{entry.display_name.clone()}}
                                    text(class = "gds-files-details__subtitle") {{entry.path.display().to_string()}}
                                }
                                column(class = "gds-files-details__rows") {
                                    {all_rows}
                                }
                                {tags_view}
                                {perm_button.map(|btn| view! {
                                    row(class = "gds-files-details__actions") {
                                        {btn}
                                    }
                                }.into_view())}
                            }
                        })
                    }
                }}
            }
        })
    }
}

struct PermissionEditorInner {
    browser: WeakBrowser,
    entry: Entry,
    directory: bool,
    mode: RwSignal<u32, LocalStorage>,
    recursive: RwSignal<bool, LocalStorage>,
    loading: RwSignal<bool, LocalStorage>,
    saving: RwSignal<bool, LocalStorage>,
    load_failed: RwSignal<bool, LocalStorage>,
    error: RwSignal<Option<String>, LocalStorage>,
}

#[derive(Clone)]
struct PermissionEditor(Rc<PermissionEditorInner>);

impl PermissionEditor {
    fn new(browser: &Browser, entry: Entry) -> Self {
        let directory = entry.kind == EntryKind::Directory;
        let initial_mode = entry
            .details
            .permissions
            .as_ref()
            .map_or(0o644, |perms| perms.mode & 0o777);
        let editor = Self(Rc::new(PermissionEditorInner {
            browser: browser.downgrade(),
            entry: entry.clone(),
            directory,
            mode: RwSignal::new_local(initial_mode),
            recursive: RwSignal::new_local(false),
            loading: RwSignal::new_local(true),
            saving: RwSignal::new_local(false),
            load_failed: RwSignal::new_local(false),
            error: RwSignal::new_local(None),
        }));

        if entry.kind == EntryKind::SymbolicLink {
            editor.0.loading.set(false);
            editor.0.load_failed.set(true);
            editor
                .0
                .error
                .set(Some(i18n::text_untracked("permissions-symlink")));
            return editor;
        }

        let weak = Rc::downgrade(&editor.0);
        let read_path = entry.path.clone();
        browser.blocking(
            "Read file permissions",
            move || permissions::read(&read_path).map_err(|err| err.to_string()),
            move |_, result| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let target = Self(inner);
                target.0.loading.set(false);
                match result {
                    Ok(mode) => {
                        target.0.mode.set(mode & 0o777);
                        target.0.load_failed.set(false);
                        target.0.error.set(None);
                    }
                    Err(message) => {
                        target.0.load_failed.set(true);
                        target.0.error.set(Some(message));
                    }
                }
            },
        );
        editor
    }

    fn is_saving(&self) -> bool {
        self.0.saving.get()
    }

    fn can_save(&self) -> bool {
        !self.0.loading.get() && !self.0.load_failed.get() && !self.0.saving.get()
    }

    fn save(&self, complete: impl FnOnce() + 'static) {
        if self.0.loading.get_untracked()
            || self.0.load_failed.get_untracked()
            || self.0.saving.get_untracked()
        {
            return;
        }
        let Some(browser) = self.0.browser.upgrade() else {
            return;
        };
        let path = self.0.entry.path.clone();
        let access = self.0.mode.get_untracked() & 0o777;
        let recursive = self.0.directory && self.0.recursive.get_untracked();
        let weak = Rc::downgrade(&self.0);

        self.0.saving.set(true);
        self.0.error.set(None);

        let done = RefCell::new(Some(Box::new(complete) as Box<dyn FnOnce()>));
        if recursive {
            let Some(cancel) = browser.begin_operation(&i18n::text_untracked("permissions-saving"))
            else {
                self.0.saving.set(false);
                return;
            };
            let refresh_path = path.clone();
            browser.blocking(
                "Apply recursive permissions",
                move || Ok(permissions::apply_cancellable(&path, access, true, &cancel)),
                move |browser, result| {
                    browser.end_operation();
                    let Some(inner) = weak.upgrade() else {
                        return;
                    };
                    let target = Self(inner);
                    target.0.saving.set(false);
                    match result {
                        Ok(report) => {
                            browser.refresh_affected(&[refresh_path]);
                            if report.failures.is_empty() {
                                browser.report_success(i18n::text_untracked("permissions-saved"));
                                if let Some(cb) = done.borrow_mut().take() {
                                    cb();
                                }
                            } else {
                                for failure in &report.failures {
                                    tracing::warn!(
                                        path = %failure.path.display(),
                                        error = %failure.error,
                                        "recursive permission update failed"
                                    );
                                }
                                let succeeded = report.succeeded;
                                let failed = report.failures.len();
                                let msg =
                                    gds::t_untracked!("permissions-partial", succeeded, failed);
                                browser.report_notice(msg.clone());
                                target.0.error.set(Some(msg));
                            }
                        }
                        Err(err) => {
                            target.0.error.set(Some(err));
                        }
                    }
                },
            );
        } else {
            let refresh_path = path.clone();
            browser.blocking(
                "Save file permissions",
                move || permissions::save(&path, access).map_err(|err| err.to_string()),
                move |browser, result| {
                    let Some(inner) = weak.upgrade() else {
                        return;
                    };
                    let target = Self(inner);
                    target.0.saving.set(false);
                    match result {
                        Ok(()) => {
                            browser.refresh_affected(&[refresh_path]);
                            browser.report_success(i18n::text_untracked("permissions-saved"));
                            if let Some(cb) = done.borrow_mut().take() {
                                cb();
                            }
                        }
                        Err(err) => {
                            target.0.error.set(Some(err));
                        }
                    }
                },
            );
        }
    }

    fn bit_checkbox(&self, label_key: &'static str, mask: u32) -> AnyView {
        let mode_sig = self.0.mode;
        let loading_sig = self.0.loading;
        let saving_sig = self.0.saving;
        let failed_sig = self.0.load_failed;
        let disabled =
            Signal::derive_local(move || loading_sig.get() || saving_sig.get() || failed_sig.get());
        let checked = Binding::controlled(
            Signal::derive_local(move || {
                if (mode_sig.get() & mask) != 0 {
                    Checked::Yes
                } else {
                    Checked::No
                }
            }),
            move |next| {
                mode_sig.update(|bits| {
                    if matches!(next, Checked::Yes) {
                        *bits |= mask;
                    } else {
                        *bits &= !mask;
                    }
                });
            },
        );
        let box_ref = NodeRef::new();
        let label_ref = NodeRef::new();
        AnyView::new(view! {
            row(class = "gds-files-permissions-check") {
                Checkbox(
                    node_ref = box_ref,
                    checked = checked,
                    disabled = disabled,
                    labelled_by = label_ref,
                )
                Label(node_ref = label_ref, control = box_ref) {{i18n::text(label_key)}}
            }
        })
    }

    fn group(&self, title_key: &'static str, shift: u32) -> AnyView {
        let read = self.bit_checkbox("permissions-read", 0o4 << shift);
        let write = self.bit_checkbox("permissions-write", 0o2 << shift);
        let execute = self.bit_checkbox("permissions-execute", 0o1 << shift);
        AnyView::new(view! {
            column(class = "gds-files-permissions-group") {
                text(class = "gds-files-permissions-group__title") {{i18n::text(title_key)}}
                row(class = "gds-files-permissions-group__checks") {
                    {read}
                    {write}
                    {execute}
                }
            }
        })
    }

    fn fields(&self) -> AnyView {
        let loading_sig = self.0.loading;
        let saving_sig = self.0.saving;
        let failed_sig = self.0.load_failed;
        let error_sig = self.0.error;
        let rec_sig = self.0.recursive;

        let owner_group = self.group("permissions-owner", 6);
        let group_group = self.group("permissions-group", 3);
        let other_group = self.group("permissions-others", 0);

        let recursive_view = self.0.directory.then(|| {
            let disabled = Signal::derive_local(move || {
                loading_sig.get() || saving_sig.get() || failed_sig.get()
            });
            let checked = Binding::controlled(
                Signal::derive_local(move || {
                    if rec_sig.get() {
                        Checked::Yes
                    } else {
                        Checked::No
                    }
                }),
                move |next| rec_sig.set(matches!(next, Checked::Yes)),
            );
            let rec_ref = NodeRef::new();
            let rec_label = NodeRef::new();
            view! {
                column(style:gap = "4px") {
                    row(class = "gds-files-permissions-check") {
                        Checkbox(
                            node_ref = rec_ref,
                            checked = checked,
                            disabled = disabled,
                            labelled_by = rec_label,
                        )
                        Label(node_ref = rec_label, control = rec_ref) {{i18n::text("permissions-recursive")}}
                    }
                    text(class = "gds-files-permissions-note") {{i18n::text("permissions-recursive-hint")}}
                }
            }
            .into_view()
        });

        AnyView::new(view! {
            column(class = "gds-files-permissions-grid") {
                {owner_group}
                {group_group}
                {other_group}
                {recursive_view}
                {move || {
                    if loading_sig.get() {
                        Some(AnyView::new(view! {
                            text(class = "gds-files-permissions-note") {{i18n::text("permissions-loading")}}
                        }))
                    } else if saving_sig.get() {
                        Some(AnyView::new(view! {
                            text(class = "gds-files-permissions-note") {{i18n::text("permissions-saving")}}
                        }))
                    } else {
                        None
                    }
                }}
                {move || {
                    error_sig.get().map(|err| {
                        view! {
                            text(class = "gds-files-permissions-error") {{err}}
                        }
                        .into_view()
                    })
                }}
            }
        })
    }
}

fn empty_details_state(icon: IconName, text: String) -> AnyView {
    AnyView::new(view! {
        column(class = "gds-files-details__empty") {
            Icon(name = icon, class = "gds-files-details__empty-icon")
            text(class = "gds-files-details__empty-text") {{text}}
        }
    })
}

fn static_row_details(browser: &Browser, row: &Row) -> AnyView {
    match row {
        Row::Place(location) => AnyView::new(view! {
            column(class = "gds-files-details") {
                column(class = "gds-files-details__hero") {
                    {file_artwork_view(FileIcon::plain(location.icon()), 64.0)}
                    text(class = "gds-files-details__title") {{location.label()}}
                    text(class = "gds-files-details__subtitle") {{location.display_address()}}
                }
                column(class = "gds-files-details__rows") {
                    {text_row("place-location", location.display_address())}
                }
            }
        }),
        Row::Trash(entry) => {
            let size = entry
                .size
                .map(EntryPresentation::format_size)
                .or_else(|| entry.items.map(|n| n.to_string()))
                .unwrap_or_else(|| UNKNOWN_VALUE.to_owned());
            AnyView::new(view! {
                column(class = "gds-files-details") {
                    column(class = "gds-files-details__hero") {
                        {file_artwork_view(FileIcon::plain(row.icon()), 64.0)}
                        text(class = "gds-files-details__title") {{row.label()}}
                    }
                    column(class = "gds-files-details__rows") {
                        {text_row("recycle-original-location", entry.original_path.display().to_string())}
                        {metadata_row("recycle-deleted-at", AnyView::new(view! { {Timestamp::new(entry.deleted_at)} }))}
                        {text_row("size", size)}
                    }
                }
            })
        }
        Row::Tag(tag) => AnyView::new(view! {
            column(class = "gds-files-details") {
                column(class = "gds-files-details__hero") {
                    {file_artwork_view(FileIcon::plain(IconName::Tag), 64.0)}
                    text(class = "gds-files-details__title") {{row.label()}}
                    text(class = "gds-files-details__subtitle") {{tag.clone()}}
                }
            }
        }),
        Row::Remote(entry) => {
            let size = entry
                .size
                .map(EntryPresentation::format_size)
                .unwrap_or_else(|| UNKNOWN_VALUE.to_owned());
            AnyView::new(view! {
                column(class = "gds-files-details") {
                    column(class = "gds-files-details__hero") {
                        {file_artwork_view(FileIcon::plain(row.icon()), 64.0)}
                        text(class = "gds-files-details__title") {{entry.name.clone()}}
                        text(class = "gds-files-details__subtitle") {{entry.address.uri().to_owned()}}
                    }
                    column(class = "gds-files-details__rows") {
                        {metadata_row("modified", AnyView::new(view! { {Timestamp::new(entry.modified)} }))}
                        {text_row("size", size)}
                    }
                }
            })
        }
        Row::Volume(volume) => {
            let state = browser.0.volumes.capacity(&volume.id);
            let (cap_label, fraction, unavailable) = match state {
                Some(volumes::CapacityState::Ready(cap)) => (
                    t!(
                        "device-capacity",
                        available = EntryPresentation::format_size(cap.available),
                        total = EntryPresentation::format_size(cap.total)
                    ),
                    Some(cap.unavailable_fraction()),
                    false,
                ),
                Some(volumes::CapacityState::Unavailable(reason)) => (reason, None, true),
                None | Some(volumes::CapacityState::Loading) => {
                    if volume.mount_points.is_empty() {
                        (i18n::text("device-not-mounted"), None, true)
                    } else {
                        (i18n::text("device-capacity-loading"), None, false)
                    }
                }
            };
            let fs = if volume.filesystem.is_empty() {
                UNKNOWN_VALUE.to_owned()
            } else {
                volume.filesystem.clone()
            };
            let mounts = if volume.mount_points.is_empty() {
                i18n::text("device-not-mounted")
            } else {
                volume
                    .mount_points
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            AnyView::new(view! {
                column(class = "gds-files-details") {
                    column(class = "gds-files-details__hero") {
                        {file_artwork_view(FileIcon::plain(IconName::HardDrive), 64.0)}
                        text(class = "gds-files-details__title") {{volume.name.clone()}}
                    }
                    column(class = "gds-files-details__rows") {
                        {text_row("volume-format", fs)}
                        {text_row("volume-default-mount", mounts)}
                    }
                    CapacityBar(label = cap_label, fraction = fraction, unavailable = unavailable)
                }
            })
        }
        Row::File(_) => AnyView::new(view! { box() }),
    }
}

impl Browser {
    pub(super) fn edit_tags(&self, path: PathBuf) {
        self.0.menus.close();
        let slots = dialog_slots(self);
        if slots
            .tags
            .get_untracked()
            .as_ref()
            .is_some_and(TagEditor::is_saving)
        {
            return;
        }
        let editor = TagEditor::new(self, path);
        slots.tags.set(Some(editor.clone()));
        editor.focus();
    }

    pub(super) fn tags_dialog(&self) -> AnyView {
        install_inspection_stylesheet();
        let slots = dialog_slots(self);
        let tags_sig = slots.tags;
        let open = Signal::derive_local(move || tags_sig.get().is_some());

        AnyView::new(view! {
            Dialog(
                open = Binding::controlled(open, move |next: bool| {
                    if !next
                        && !tags_sig
                            .get_untracked()
                            .as_ref()
                            .is_some_and(TagEditor::is_saving)
                    {
                        tags_sig.set(None);
                    }
                }),
            ) {
                DialogContent {
                    {move || match tags_sig.get() {
                        None => AnyView::new(view! { box() }),
                        Some(editor) => {
                            let title_path = editor
                                .path()
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| editor.path().display().to_string());
                            let fields = editor.fields();
                            let save_editor = editor.clone();
                            let disabled = Signal::derive_local(move || {
                                !editor.available() || editor.is_saving()
                            });
                            let can_cancel = Signal::derive_local(move || {
                                tags_sig
                                    .get()
                                    .as_ref()
                                    .is_some_and(TagEditor::is_saving)
                            });
                            AnyView::new(view! {
                                column(class = "gds-files-dialog-body") {
                                    DialogHeader {
                                        DialogTitle {{i18n::text("tags-editor-title")}}
                                        DialogDescription {{title_path}}
                                    }
                                    {fields}
                                    DialogFooter {
                                        Button(
                                            variant = ButtonVariant::Outline,
                                            disabled = can_cancel,
                                            on:click = move |_| tags_sig.set(None),
                                        ) {{i18n::text("dialog-cancel")}}
                                        Button(
                                            variant = ButtonVariant::Default,
                                            disabled = disabled,
                                            on:click = move |_| save_editor.save(move || tags_sig.set(None)),
                                        ) {{i18n::text("permissions-save")}}
                                    }
                                }
                            })
                        }
                    }}
                }
            }
        })
    }

    pub(super) fn edit_permissions(&self, entry: Entry) {
        self.0.menus.close();
        let slots = dialog_slots(self);
        if slots
            .permissions
            .get_untracked()
            .as_ref()
            .is_some_and(PermissionEditor::is_saving)
        {
            return;
        }
        let editor = PermissionEditor::new(self, entry);
        slots.permissions.set(Some(editor));
    }

    pub(super) fn permissions_dialog(&self) -> AnyView {
        install_inspection_stylesheet();
        let slots = dialog_slots(self);
        let perm_sig = slots.permissions;
        let open = Signal::derive_local(move || perm_sig.get().is_some());

        AnyView::new(view! {
            Dialog(
                open = Binding::controlled(open, move |next: bool| {
                    if !next
                        && !perm_sig
                            .get_untracked()
                            .as_ref()
                            .is_some_and(PermissionEditor::is_saving)
                    {
                        perm_sig.set(None);
                    }
                }),
            ) {
                DialogContent {
                    {move || match perm_sig.get() {
                        None => AnyView::new(view! { box() }),
                        Some(editor) => {
                            let name = editor.0.entry.display_name.clone();
                            let fields = editor.fields();
                            let save_editor = editor.clone();
                            let cancel_disabled = Signal::derive_local(move || editor.is_saving());
                            let save_disabled = Signal::derive_local(move || {
                                !perm_sig.get().as_ref().is_some_and(PermissionEditor::can_save)
                            });
                            AnyView::new(view! {
                                column(class = "gds-files-dialog-body") {
                                    DialogHeader {
                                        DialogTitle {{i18n::text("permissions-title")}}
                                        DialogDescription {{name}}
                                    }
                                    {fields}
                                    DialogFooter {
                                        Button(
                                            variant = ButtonVariant::Outline,
                                            disabled = cancel_disabled,
                                            on:click = move |_| perm_sig.set(None),
                                        ) {{i18n::text("dialog-cancel")}}
                                        Button(
                                            variant = ButtonVariant::Default,
                                            disabled = save_disabled,
                                            on:click = move |_| save_editor.save(move || perm_sig.set(None)),
                                        ) {{i18n::text("permissions-save")}}
                                    }
                                }
                            })
                        }
                    }}
                }
            }
        })
    }

    pub(super) fn row_artwork_with_selection(
        &self,
        row: &Row,
        size: f32,
        selected: Signal<f32, LocalStorage>,
    ) -> AnyView {
        self.artwork(row, size, selected)
    }

    pub(super) fn artwork(
        &self,
        row: &Row,
        size: f32,
        selected: Signal<f32, LocalStorage>,
    ) -> AnyView {
        install_inspection_stylesheet();
        let inner = match row {
            Row::File(entry) => {
                if size >= 32.0
                    && let Some(source) = entry.thumbnail_source()
                {
                    let cache = self.0.inspections.get_untracked();
                    let _ = cache.request(&source, Request::Thumbnail);
                    AnyView::new(view! {
                        Thumbnail(
                            cache = cache,
                            source = source,
                            show_status = false,
                        )
                    })
                } else {
                    file_artwork_view(FileIcon::for_entry(entry), size)
                }
            }
            Row::Trash(_) => file_artwork_view(FileIcon::plain(row.icon()), size),
            _ => file_artwork_view(FileIcon::plain(row.icon()), size),
        };

        AnyView::new(view! {
            box(
                class = "gds-files-artwork-slot",
                style:width = format!("{size:.1}px"),
                style:height = format!("{size:.1}px"),
                attr:data-selected = move || (selected.get() > 0.5).then(|| "true".to_owned()),
            ) {
                {inner}
            }
        })
    }

    pub(super) fn toggle_details(&self) {
        self.0.details.update(|visible| *visible = !*visible);
        let visible = self.0.details.get_untracked();
        let location = self.0.state.borrow().tab().navigation.location().clone();
        self.update(|state| {
            state.places.set_details(location, visible);
        });
        self.save_places();
    }

    pub(super) fn details_view(&self) -> AnyView {
        install_inspection_stylesheet();
        let browser = self.clone();
        let cached_entry: Rc<RefCell<Option<(PathBuf, Option<(i64, i64)>, bool, Properties)>>> =
            Rc::new(RefCell::new(None));

        AnyView::new(move || {
            let (pure_tags, selected) = browser.read(|state| {
                let tab = state.tab();
                let pure_tags = matches!(tab.contents, Contents::Tags { .. })
                    && !tab.rows.iter().any(|row| matches!(row, Row::File(_)));
                (pure_tags, tab.selected())
            });

            if pure_tags {
                return AnyView::new(view! { box() });
            }

            if selected.is_empty() {
                return empty_details_state(IconName::Info, i18n::text("preview-empty"));
            }

            if selected.len() > 1 {
                let count = selected.len();
                return AnyView::new(view! {
                    column(class = "gds-files-details__empty") {
                        {file_artwork_view(FileIcon::multiple(), 48.0)}
                        text(class = "gds-files-details__empty-text") {{t!("preview-multiple", count)}}
                    }
                });
            }

            let row = &selected[0];
            let tags_enabled = browser.0.preferences.state().with(|p| p.tags.enabled);
            let local_entry_or_place: Option<Entry> = match row {
                Row::File(entry) => Some(entry.clone()),
                Row::Place(Location::Directory(path)) => {
                    let path_buf = PathBuf::from(path);
                    let props = {
                        let cache_hit = cached_entry
                            .borrow()
                            .as_ref()
                            .filter(|(p, rev, enabled, _)| {
                                *p == path_buf && rev.is_none() && *enabled == tags_enabled
                            })
                            .map(|(_, _, _, props)| props.clone());
                        if let Some(hit) = cache_hit {
                            hit
                        } else {
                            let created = Properties::new(path_buf.clone(), browser.scope());
                            *cached_entry.borrow_mut() =
                                Some((path_buf, None, tags_enabled, created.clone()));
                            created
                        }
                    };
                    return props.view(Some(browser.clone()), true);
                }
                _ => None,
            };

            if let Some(entry) = local_entry_or_place {
                let props = {
                    let cache_hit = cached_entry
                        .borrow()
                        .as_ref()
                        .filter(|(p, rev, enabled, _)| {
                            *p == entry.path
                                && *rev == entry.details.changed
                                && *enabled == tags_enabled
                        })
                        .map(|(_, _, _, props)| props.clone());
                    if let Some(hit) = cache_hit {
                        hit
                    } else {
                        let created = Properties::from_entry(
                            entry.clone(),
                            browser.0.inspections.get_untracked(),
                            browser.scope(),
                            tags_enabled,
                        );
                        *cached_entry.borrow_mut() = Some((
                            entry.path.clone(),
                            entry.details.changed,
                            tags_enabled,
                            created.clone(),
                        ));
                        created
                    }
                };
                return props.view(Some(browser.clone()), true);
            }

            static_row_details(&browser, row)
        })
    }
}
