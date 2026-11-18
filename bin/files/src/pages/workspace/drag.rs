//! Native tab reordering, pointer/OS drag-and-drop, spring-loading, and lazy file drag snapshots.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, OnceLock},
    time::Duration,
};

use gds::ui::drag::{DragCardProps, DragOverlayState, drop_hit_test, drop_target_attrs};
use zgui::prelude::*;
use zgui::reactive::{Signal, UnsyncCallback, on_cleanup_local};
use zgui::view;
use zgui::view::{EventCx, TimeoutHandle, Timers};
use zgui::vocab::PointerButton;

use super::{
    geometry::Grid,
    model::{Contents, Row},
    ui::Browser,
};
use crate::{
    entities::file::directory::Entry,
    features::file_operations::{paste::PasteJob, transfer},
    shared::{i18n, lib::selection::Selection},
};

#[derive(Clone, Debug)]
struct DragItem {
    path: PathBuf,
}

impl DragItem {
    fn new(entry: &Entry) -> Self {
        Self {
            path: entry.path.clone(),
        }
    }
}

#[derive(Debug)]
struct DragSnapshotSource {
    rows: Arc<[Entry]>,
    selected: Selection<PathBuf>,
}

#[derive(Debug)]
pub(in crate::pages::workspace) struct DragSnapshotInner {
    source: std::sync::Mutex<Option<DragSnapshotSource>>,
    entries: OnceLock<Arc<[DragItem]>>,
}

impl DragSnapshotInner {
    fn entries(&self) -> &[DragItem] {
        self.entries
            .get_or_init(|| {
                let Some(source) = self
                    .source
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                else {
                    return Arc::from([]);
                };
                source
                    .rows
                    .iter()
                    .filter(|row| source.selected.contains(&row.path))
                    .map(DragItem::new)
                    .collect::<Vec<_>>()
                    .into()
            })
            .as_ref()
    }
}

#[derive(Clone, Debug)]
enum DragPayload {
    Single(Entry),
    Selection(Arc<DragSnapshotInner>),
}

/// Files carried by a drag that started in the file table.
#[derive(Clone, Debug)]
pub(super) struct FileDrag {
    payload: DragPayload,
}

impl FileDrag {
    fn selection(inner: Arc<DragSnapshotInner>) -> Self {
        Self {
            payload: DragPayload::Selection(inner),
        }
    }

    fn single(entry: &Entry) -> Self {
        Self {
            payload: DragPayload::Single(entry.clone()),
        }
    }

    pub(super) fn is_source(_path: &Path) -> bool {
        false
    }

    pub(super) fn paths(&self) -> Vec<PathBuf> {
        match &self.payload {
            DragPayload::Single(entry) => vec![entry.path.clone()],
            DragPayload::Selection(inner) => inner
                .entries()
                .iter()
                .map(|item| item.path.clone())
                .collect(),
        }
    }

    pub(super) fn count(&self) -> usize {
        match &self.payload {
            DragPayload::Single(_) => 1,
            DragPayload::Selection(inner) => inner.entries().len(),
        }
    }
}

/// Frozen selection for drag payloads whose item slice materializes lazily on drag start.
#[derive(Clone, Default)]
pub(super) struct DragSnapshot {
    inner: Option<Arc<DragSnapshotInner>>,
}

impl DragSnapshot {
    pub(super) fn new(rows: &Arc<[Entry]>, selected: &Selection<PathBuf>) -> Self {
        if rows.is_empty() || selected.len() <= 1 {
            return Self { inner: None };
        }
        Self {
            inner: Some(Arc::new(DragSnapshotInner {
                source: std::sync::Mutex::new(Some(DragSnapshotSource {
                    rows: Arc::clone(rows),
                    selected: selected.clone(),
                })),
                entries: OnceLock::new(),
            })),
        }
    }

    #[cfg(test)]
    pub(super) fn drag_for(&self, entry: &Entry, selected: &Selection<PathBuf>) -> FileDrag {
        self.drag_for_row(entry, selected.contains(&entry.path))
    }

    pub(super) fn drag_for_row(&self, entry: &Entry, selected: bool) -> FileDrag {
        match (&self.inner, selected) {
            (Some(inner), true) => FileDrag::selection(Arc::clone(inner)),
            _ => FileDrag::single(entry),
        }
    }

    #[cfg(test)]
    pub(super) fn is_materialized(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.entries.get().is_some())
    }
}

const DRAG_THRESHOLD: f32 = 6.0;
const SPRING_LOAD_DELAY: Duration = Duration::from_millis(600);

#[derive(Clone)]
enum WindowDragPayload {
    Files(FileDrag),
    Tab { id: u64, title: String },
}

#[derive(Clone)]
enum ArmedDrag {
    File {
        entry: Entry,
        tab_id: u64,
        start: (f32, f32),
    },
    Tab {
        id: u64,
        pinned: bool,
        start: (f32, f32),
    },
}

#[derive(Clone)]
enum TargetKind {
    Folder(PathBuf),
    Tab(u64),
}

#[derive(Clone)]
struct RegisteredTarget {
    key: String,
    node: NodeRef,
    kind: TargetKind,
}

#[derive(Clone)]
struct RegisteredTab {
    id: u64,
    pinned: bool,
    node: NodeRef,
}

struct SpringLoad {
    _timer: TimeoutHandle,
}

struct DragCoordinator {
    overlay: DragOverlayState<WindowDragPayload>,
    armed: RefCell<Option<ArmedDrag>>,
    targets: RefCell<Vec<RegisteredTarget>>,
    tabs: RefCell<Vec<RegisteredTab>>,
    spring: RefCell<Option<SpringLoad>>,
    copy_modifier: Cell<bool>,
}

impl DragCoordinator {
    fn new() -> Self {
        Self {
            overlay: DragOverlayState::new(),
            armed: RefCell::new(None),
            targets: RefCell::new(Vec::new()),
            tabs: RefCell::new(Vec::new()),
            spring: RefCell::new(None),
            copy_modifier: Cell::new(false),
        }
    }

    fn register_target(&self, key: String, node: NodeRef, kind: TargetKind) {
        let mut targets = self.targets.borrow_mut();
        targets.retain(|item| item.node.get_untracked().is_some() && item.key != key);
        targets.push(RegisteredTarget { key, node, kind });
    }

    fn unregister_target(&self, key: &str) {
        self.targets.borrow_mut().retain(|item| item.key != key);
    }

    fn register_tab(&self, id: u64, pinned: bool, node: NodeRef) {
        let mut tabs = self.tabs.borrow_mut();
        tabs.retain(|item| item.node.get_untracked().is_some() && item.id != id);
        tabs.push(RegisteredTab { id, pinned, node });
    }

    fn unregister_tab(&self, id: u64) {
        self.tabs.borrow_mut().retain(|item| item.id != id);
    }

    fn hit_target(&self, pos: (f32, f32)) -> Option<RegisteredTarget> {
        let mut targets = self.targets.borrow_mut();
        targets.retain(|item| item.node.get_untracked().is_some());
        targets
            .iter()
            .rev()
            .find(|item| drop_hit_test(&item.node, pos))
            .cloned()
    }

    fn hit_tab(&self, pos: (f32, f32)) -> Option<RegisteredTab> {
        let mut tabs = self.tabs.borrow_mut();
        tabs.retain(|item| item.node.get_untracked().is_some());
        tabs.iter()
            .find(|item| drop_hit_test(&item.node, pos))
            .cloned()
    }

    fn clear_spring(&self) {
        *self.spring.borrow_mut() = None;
    }

    fn cancel_all(&self) {
        *self.armed.borrow_mut() = None;
        self.clear_spring();
        self.overlay.cancel();
    }
}

thread_local! {
    static COORDINATORS: RefCell<HashMap<usize, Rc<DragCoordinator>>> = RefCell::new(HashMap::new());
    static CLEANUP_REGISTERED: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static ROW_PRESSED_WINDOW: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn mark_row_pointer_down(browser: &Browser) {
    ROW_PRESSED_WINDOW.with(|slot| slot.set(Rc::as_ptr(&browser.0) as usize));
}

pub(super) fn take_row_pointer_down(browser: &Browser) -> bool {
    let key = Rc::as_ptr(&browser.0) as usize;
    ROW_PRESSED_WINDOW.with(|slot| {
        let matched = slot.get() == key;
        slot.set(0);
        matched
    })
}

fn coordinator_for(browser: &Browser) -> Rc<DragCoordinator> {
    let window_key = Rc::as_ptr(&browser.0) as usize;
    let needs_cleanup = CLEANUP_REGISTERED.with(|slot| slot.borrow_mut().insert(window_key));
    if needs_cleanup {
        browser.0.owner.with(|| {
            on_cleanup_local(move || {
                COORDINATORS.with(|slot| {
                    slot.borrow_mut().remove(&window_key);
                });
                CLEANUP_REGISTERED.with(|slot| {
                    slot.borrow_mut().remove(&window_key);
                });
            });
        });
    }
    if let Some(coord) = COORDINATORS.with(|slot| slot.borrow().get(&window_key).cloned()) {
        return coord;
    }
    // DragCoordinator allocates RwSignal overlay state; anchor it to the window owner so
    // unmounting the first list/row branch does not dispose signals cached per window.
    let coord = browser.0.owner.with(|| Rc::new(DragCoordinator::new()));
    COORDINATORS.with(|slot| {
        Rc::clone(
            slot.borrow_mut()
                .entry(window_key)
                .or_insert_with(|| Rc::clone(&coord)),
        )
    })
}

impl Browser {
    pub(super) fn background_drop_target(&self, view: AnyView) -> AnyView {
        let coord = coordinator_for(self);
        let overlay = coord.overlay;
        let browser = self.clone();
        let on_external_drop =
            UnsyncCallback::new(move |(paths, _pos): (Vec<PathBuf>, (f32, f32))| {
                let destination = browser.read_untracked(|state| {
                    let tab = state.tab();
                    matches!(tab.contents, Contents::Directory)
                        .then(|| tab.navigation.path().map(Path::to_path_buf))
                        .flatten()
                });
                if let Some(destination) = destination
                    && !paths.is_empty()
                {
                    let job = PasteJob::new(destination, transfer::Mode::Copy, paths);
                    browser.inspect_paste(job, false, true, None);
                }
            });
        let inactive = Signal::stored_local(false);
        let drop_attrs = drop_target_attrs(inactive, Some(on_external_drop));
        let copy_mode = Signal::derive_local(move || {
            let _ = overlay.position().get();
            coord.copy_modifier.get()
        });

        AnyView::new(view! {
            box(class = "gds-files-drop-background", {..drop_attrs}) {
                {view}
                {move || overlay.payload().get().map(|payload| {
                    let (header, detail) = match payload {
                        WindowDragPayload::Files(drag) => {
                            let count = drag.count();
                            let title = if count == 1 {
                                drag.paths()
                                    .first()
                                    .and_then(|path| path.file_name())
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| gds::i18n::count("selection-count", count))
                            } else {
                                gds::i18n::count("selection-count", count)
                            };
                            let action = if copy_mode.get() {
                                i18n::text("copy-files")
                            } else {
                                String::new()
                            };
                            (title, action)
                        }
                        WindowDragPayload::Tab { title, .. } => (title, String::new()),
                    };
                    AnyView::new(view! {
                        DragCard(position = overlay.position()) {
                            row(class = "gds-drag-card__header") { {header} }
                            {(!detail.is_empty()).then(|| view! {
                                column(class = "gds-drag-card__content") { {detail} }
                            })}
                        }
                    })
                })}
            }
        })
    }

    pub(super) fn draggable_tab(&self, view: AnyView, id: u64, pinned: bool) -> AnyView {
        let coord = coordinator_for(self);
        let node = NodeRef::new();
        let target_key = format!("tab:{id}");
        coord.register_tab(id, pinned, node);
        coord.register_target(target_key.clone(), node, TargetKind::Tab(id));
        let cleanup_coord = Rc::clone(&coord);
        let cleanup_key = target_key.clone();
        on_cleanup_local(move || {
            cleanup_coord.unregister_tab(id);
            cleanup_coord.unregister_target(&cleanup_key);
        });

        let overlay = coord.overlay;
        let is_hovered = Signal::derive_local(move || {
            overlay.hovered_target().get().as_deref() == Some(&target_key)
        });

        let down_coord = Rc::clone(&coord);
        let move_coord = Rc::clone(&coord);
        let move_browser = self.clone();
        let up_coord = Rc::clone(&coord);
        let up_browser = self.clone();
        let cancel_coord = Rc::clone(&coord);

        AnyView::new(view! {
            box(
                node_ref = node,
                attr:data-drop-active = move || is_hovered.get().then(|| "true".to_owned()),
                on:pointer_down = move |ev| {
                    if ev.button != Some(PointerButton::Primary) {
                        return;
                    }
                    *down_coord.armed.borrow_mut() = Some(ArmedDrag::Tab {
                        id,
                        pinned,
                        start: (ev.position.x.0, ev.position.y.0),
                    });
                },
                on:pointer_move = move |ev| {
                    let pos = (ev.position.x.0, ev.position.y.0);
                    let mut start_payload = None;
                    if let Some(ArmedDrag::Tab {
                        id: armed_id,
                        pinned: armed_pinned,
                        start,
                    }) = *move_coord.armed.borrow()
                        && armed_id == id
                        && distance(start, pos) >= DRAG_THRESHOLD
                    {
                        let _ = armed_pinned;
                        let title = move_browser.read_untracked(|state| {
                            state
                                .tabs
                                .iter()
                                .find(|tab| tab.id == id)
                                .map_or_else(String::new, |tab| {
                                    move_browser.location_title(tab.navigation.location())
                                })
                        });
                        start_payload = Some(WindowDragPayload::Tab { id, title });
                    }
                    if let Some(payload) = start_payload {
                        *move_coord.armed.borrow_mut() = None;
                        ev.capture_pointer();
                        move_coord.overlay.begin(payload, pos.0, pos.1);
                    }
                    if matches!(move_coord.overlay.payload().get_untracked(), Some(WindowDragPayload::Tab { .. })) {
                        move_coord.overlay.move_to(pos.0, pos.1);
                        let hovered = move_coord
                            .hit_tab(pos)
                            .filter(|tab| tab.pinned == pinned)
                            .map(|tab| format!("tab:{}", tab.id));
                        move_coord.overlay.set_hovered_target(hovered);
                    }
                },
                on:pointer_up = move |ev| {
                    if ev.button != Some(PointerButton::Primary) {
                        return;
                    }
                    if matches!(
                        up_coord.armed.borrow().as_ref(),
                        Some(ArmedDrag::Tab { id: armed_id, .. }) if *armed_id == id
                    ) {
                        *up_coord.armed.borrow_mut() = None;
                    }
                    let is_tab_drag = matches!(
                        up_coord.overlay.payload().get_untracked(),
                        Some(WindowDragPayload::Tab { .. })
                    );
                    if !is_tab_drag {
                        return;
                    }
                    let pos = (ev.position.x.0, ev.position.y.0);
                    if let Some(WindowDragPayload::Tab { id: from_id, .. }) = up_coord.overlay.end()
                        && let Some(target_tab) = up_coord.hit_tab(pos)
                        && target_tab.id != from_id
                        && let (Some(from_idx), Some(to_idx)) = (
                            up_browser.tab_index(from_id),
                            up_browser.tab_index(target_tab.id),
                        )
                    {
                        up_browser.reorder_tab(from_idx, to_idx);
                        ev.stop_propagation();
                    }
                },
                on:pointer_cancel = move |_| {
                    cancel_coord.cancel_all();
                },
            ) {
                {view}
            }
        })
    }

    pub(super) fn file_drag_target(&self, view: AnyView, destination: PathBuf) -> AnyView {
        let coord = coordinator_for(self);
        let node = NodeRef::new();
        let target_key = format!("dir:{}", destination.display());
        coord.register_target(
            target_key.clone(),
            node,
            TargetKind::Folder(destination.clone()),
        );
        let cleanup_coord = Rc::clone(&coord);
        let cleanup_key = target_key.clone();
        on_cleanup_local(move || {
            cleanup_coord.unregister_target(&cleanup_key);
        });

        let overlay = coord.overlay;
        let internal_active = Signal::derive_local(move || {
            overlay.hovered_target().get().as_deref() == Some(&target_key)
        });

        let drop_browser = self.clone();
        let drop_dest = destination;
        let on_external_drop =
            UnsyncCallback::new(move |(paths, _pos): (Vec<PathBuf>, (f32, f32))| {
                if !paths.is_empty() {
                    let job = PasteJob::new(drop_dest.clone(), transfer::Mode::Copy, paths);
                    drop_browser.inspect_paste(job, false, true, None);
                }
            });
        let attrs = drop_target_attrs(internal_active, Some(on_external_drop));

        AnyView::new(view! {
            box(node_ref = node, {..attrs}) {
                {view}
            }
        })
    }

    pub(super) fn file_row_drag(&self, view: AnyView, row: &Row, _grid: Option<Grid>) -> AnyView {
        let Row::File(entry) = row else {
            return view;
        };
        let coord = coordinator_for(self);
        let row_entry = entry.clone();
        let press_browser = self.clone();
        let press_coord = Rc::clone(&coord);
        let move_browser = self.clone();
        let move_coord = Rc::clone(&coord);
        let up_browser = self.clone();
        let up_coord = Rc::clone(&coord);
        let cancel_coord = Rc::clone(&coord);

        AnyView::new(view! {
            box(
                on:pointer_down = move |ev| {
                    if ev.button != Some(PointerButton::Primary) {
                        return;
                    }
                    mark_row_pointer_down(&press_browser);
                    let tab_id = press_browser.read_untracked(|state| state.tab().id);
                    *press_coord.armed.borrow_mut() = Some(ArmedDrag::File {
                        entry: row_entry.clone(),
                        tab_id,
                        start: (ev.position.x.0, ev.position.y.0),
                    });
                },
                on:pointer_move = move |ev| {
                    let pos = (ev.position.x.0, ev.position.y.0);
                    move_coord.copy_modifier.set(ev.modifiers.control());

                    let start_drag = {
                        let armed = move_coord.armed.borrow();
                        match armed.as_ref() {
                            Some(ArmedDrag::File { entry, tab_id, start })
                                if distance(*start, pos) >= DRAG_THRESHOLD =>
                            {
                                Some((entry.clone(), *tab_id))
                            }
                            _ => None,
                        }
                    };
                    if let Some((entry, tab_id)) = start_drag {
                        *move_coord.armed.borrow_mut() = None;
                        let drag = materialize_file_drag(&move_browser, tab_id, &entry);
                        if let Some(drag) = drag {
                            let outbound_supported = move_browser
                                .0
                                .window
                                .as_ref()
                                .is_some_and(|w| w.capabilities().drag_source);
                            let _ = outbound_supported;
                            ev.capture_pointer();
                            move_coord.overlay.begin(WindowDragPayload::Files(drag), pos.0, pos.1);
                        }
                    }

                    if matches!(
                        move_coord.overlay.payload().get_untracked(),
                        Some(WindowDragPayload::Files(_))
                    ) {
                        move_coord.overlay.move_to(pos.0, pos.1);
                        update_file_drag_hover(&move_browser, &move_coord, pos);
                    }
                },
                on:pointer_up = move |ev| {
                    if ev.button != Some(PointerButton::Primary) {
                        return;
                    }
                    if matches!(up_coord.armed.borrow().as_ref(), Some(ArmedDrag::File { .. })) {
                        *up_coord.armed.borrow_mut() = None;
                    }
                    if !matches!(
                        up_coord.overlay.payload().get_untracked(),
                        Some(WindowDragPayload::Files(_))
                    ) {
                        return;
                    }
                    up_coord.clear_spring();
                    let pos = (ev.position.x.0, ev.position.y.0);
                    let target = up_coord.hit_target(pos);
                    if let Some(WindowDragPayload::Files(drag)) = up_coord.overlay.end()
                        && let Some(target) = target
                    {
                        let destination = match target.kind {
                            TargetKind::Folder(path) => Some(path),
                            TargetKind::Tab(tab_id) => up_browser.read_untracked(|state| {
                                state
                                    .tabs
                                    .iter()
                                    .find(|tab| tab.id == tab_id)
                                    .and_then(|tab| tab.navigation.path().map(Path::to_path_buf))
                            }),
                        };
                        if let Some(destination) = destination {
                            let copy = ev.modifiers.control();
                            let mode = if copy {
                                transfer::Mode::Copy
                            } else {
                                transfer::Mode::Move
                            };
                            let paths = drag.paths();
                            let same_dir = mode == transfer::Mode::Move
                                && paths.iter().all(|p| p.parent() == Some(destination.as_path()));
                            if !paths.is_empty() && !same_dir {
                                let job = PasteJob::new(destination, mode, paths);
                                up_browser.inspect_paste(job, false, true, None);
                            }
                        }
                        ev.stop_propagation();
                    }
                },
                on:pointer_cancel = move |_| {
                    cancel_coord.cancel_all();
                },
            ) {
                {view}
            }
        })
    }

    pub(super) fn mouse_navigation(
        &self,
        event: &mut EventCx<'_, zgui::view::events::PointerDown>,
    ) {
        match event.button {
            Some(PointerButton::Back) => {
                self.history(false);
                event.prevent_default();
                event.stop_propagation();
            }
            Some(PointerButton::Forward) => {
                self.history(true);
                event.prevent_default();
                event.stop_propagation();
            }
            _ => {}
        }
    }
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    (dx * dx + dy * dy).sqrt()
}

fn materialize_file_drag(browser: &Browser, tab_id: u64, entry: &Entry) -> Option<FileDrag> {
    browser.read_untracked(|state| {
        let tab = state.tabs.iter().find(|tab| tab.id == tab_id)?;
        if !matches!(tab.contents, Contents::Directory | Contents::Tags { .. }) {
            return None;
        }
        let entries: Arc<[Entry]> = tab
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::File(item) => Some(item.clone()),
                _ => None,
            })
            .collect();
        let selected_paths: Selection<PathBuf> = tab
            .navigation
            .view()
            .selection
            .iter()
            .filter_map(|key| match key {
                super::model::RowKey::Local(path) => Some(path.clone()),
                _ => None,
            })
            .collect();
        let snapshot = DragSnapshot::new(&entries, &selected_paths);
        Some(snapshot.drag_for_row(entry, selected_paths.contains(&entry.path)))
    })
}

fn update_file_drag_hover(browser: &Browser, coord: &Rc<DragCoordinator>, pos: (f32, f32)) {
    let target = coord.hit_target(pos);
    let next_key = target.as_ref().map(|t| t.key.clone());
    let prev_key = coord.overlay.hovered_target().get_untracked();
    if next_key == prev_key {
        return;
    }
    coord.overlay.set_hovered_target(next_key.clone());
    coord.clear_spring();
    let Some(target) = target else {
        return;
    };
    let already_open = match &target.kind {
        TargetKind::Folder(dest) => {
            browser.read_untracked(|state| state.tab().navigation.path() == Some(dest.as_path()))
        }
        TargetKind::Tab(tab_id) => browser.read_untracked(|state| state.tab().id == *tab_id),
    };
    if already_open {
        return;
    }
    let Some(timers) = Timers::current() else {
        return;
    };
    let weak = browser.downgrade();
    let spring_coord = Rc::downgrade(coord);
    let expected_key = target.key.clone();
    let kind = target.kind;
    let handle = timers.set_timeout(SPRING_LOAD_DELAY, move || {
        let Some(browser) = weak.upgrade() else {
            return;
        };
        let Some(coord) = spring_coord.upgrade() else {
            return;
        };
        if coord.overlay.hovered_target().get_untracked().as_deref() != Some(expected_key.as_str())
        {
            return;
        }
        match kind {
            TargetKind::Folder(dest) => {
                let location = browser.local_location(&dest);
                browser.open(location);
            }
            TargetKind::Tab(tab_id) => {
                if let Some(index) = browser.tab_index(tab_id) {
                    browser.select_tab(index);
                }
            }
        }
    });
    *coord.spring.borrow_mut() = Some(SpringLoad { _timer: handle });
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use super::{DragSnapshot, Entry, FileDrag};
    use crate::{
        entities::file::directory::{EntryDetails, EntryKind},
        shared::lib::selection::Selection,
    };

    fn sample_entry(index: usize) -> Entry {
        let name = format!("file-{index:04}.txt");
        Entry::new(
            PathBuf::from(format!("/tmp/drag-test/{name}")),
            name.into(),
            EntryKind::File,
            Some(128),
            None,
            EntryDetails::default(),
        )
    }

    #[test]
    fn drag_for_row_defers_selection_materialization_until_drag_starts() {
        let rows: Arc<[Entry]> = (0..5_000).map(sample_entry).collect::<Vec<_>>().into();
        let selected: Selection<PathBuf> = rows[..1_000]
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        let snapshot = DragSnapshot::new(&rows, &selected);

        let selected_drags: Vec<FileDrag> = rows[..30]
            .iter()
            .map(|entry| snapshot.drag_for_row(entry, true))
            .collect();
        let unselected_drags: Vec<FileDrag> = rows[1_000..1_030]
            .iter()
            .map(|entry| snapshot.drag_for(entry, &selected))
            .collect();
        assert!(!snapshot.is_materialized());

        assert_eq!(unselected_drags[0].paths(), vec![rows[1_000].path.clone()]);
        assert_eq!(unselected_drags[0].count(), 1);
        assert!(!snapshot.is_materialized());

        let expected_selected: Vec<PathBuf> = rows[..1_000]
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert_eq!(selected_drags[0].paths(), expected_selected);
        assert!(snapshot.is_materialized());
        assert_eq!(selected_drags[15].count(), 1_000);
    }
}
