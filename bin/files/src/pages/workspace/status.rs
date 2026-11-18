//! The footer's task, selection, and editable clipboard summaries.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gds::appearance::tokens::ICON_SIZE;
use gds::i18n;
use gds::ui::button::{ButtonSize, IconButtonProps};
use gds::ui::feedback::tooltip::{HintTooltipProps, wrap_tokens};
use gds::ui::{IconName, IconProps};
use tokio_util::sync::CancellationToken;
use zgui::prelude::*;
use zgui::task::spawn_local;
use zgui::view;
use zgui::vocab::Role;
use zgui_ui::overlay::OverlayState;
use zgui_ui::{PopoverContentProps, VirtualListProps};
use zgui_ui_primitives::{Align, Binding, Placement, Side};

use super::menus::Surface;
use super::model::{Operation, Row};
use super::ui::Browser;
use crate::shared::lib::clipboard::{Clipboard, Mode, Snapshot};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Tasks,
    Selection,
    Clipboard,
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Self::Tasks => "tasks-title",
            Self::Selection => "selection-title",
            Self::Clipboard => "clipboard-title",
        }
    }

    fn empty(self) -> &'static str {
        match self {
            Self::Tasks => "tasks-empty",
            Self::Selection => "selection-empty",
            Self::Clipboard => "clipboard-empty",
        }
    }
}

struct Summary {
    icon: IconName,
    count: usize,
    label: String,
}

enum Entry {
    Task {
        label: String,
        cancel: CancellationToken,
    },
    Selection {
        tab: u64,
        row: Row,
    },
    Clipboard {
        expected: Arc<Snapshot>,
        path: PathBuf,
    },
}

impl Entry {
    fn title(&self) -> String {
        match self {
            Self::Task { label, .. } => label.clone(),
            Self::Selection { row, .. } => match row {
                Row::File(entry) => entry.path.to_string_lossy().into_owned(),
                Row::Trash(entry) => entry.original_path.to_string_lossy().into_owned(),
                Row::Place(location) => location.display_address(),
                Row::Remote(entry) => entry.address.to_string(),
                Row::Tag(tag) => tag.clone(),
                Row::Volume(volume) => volume.name.clone(),
            },
            Self::Clipboard { path, .. } => path.to_string_lossy().into_owned(),
        }
    }
}

impl Browser {
    pub(super) fn status_controls(&self) -> AnyView {
        AnyView::new(view! {
            row(class = "gds-files-status-controls") {
                {self.status_control(Kind::Tasks)}
                {self.status_control(Kind::Selection)}
                {self.status_control(Kind::Clipboard)}
            }
        })
    }

    fn status_summary(&self, kind: Kind) -> Summary {
        match kind {
            Kind::Tasks => self.read(|state| match &state.operation {
                Operation::Idle => Summary {
                    icon: IconName::Activity,
                    count: 0,
                    label: String::new(),
                },
                Operation::Running { label, .. } => Summary {
                    icon: IconName::Activity,
                    count: 1,
                    label: label.clone(),
                },
            }),
            Kind::Selection => self.read(|state| {
                let count = state.tab().navigation.view().selection.len();
                Summary {
                    icon: IconName::Check,
                    count,
                    label: if count == 0 {
                        String::new()
                    } else {
                        i18n::count("selection-count", count)
                    },
                }
            }),
            Kind::Clipboard => {
                let snapshot = Clipboard::current().snapshot();
                let count = snapshot.as_ref().map_or(0, |snapshot| snapshot.paths.len());
                let icon = if snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.mode == Mode::Cut)
                {
                    IconName::Scissors
                } else {
                    IconName::Copy
                };
                let label = snapshot.as_ref().map_or_else(String::new, |snapshot| {
                    if let [path] = snapshot.paths.as_slice() {
                        path.file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        i18n::count("item-count", count)
                    }
                });
                Summary { icon, count, label }
            }
        }
    }

    fn status_control(&self, kind: Kind) -> AnyView {
        let anchor = NodeRef::new();
        let summary_browser = self.clone();
        let toggle_browser = self.clone();
        let close_browser = self.clone();
        let details_browser = self.clone();
        let open = Signal::derive_local({
            let browser = self.clone();
            move || matches!(browser.0.menus.surface.get(), Surface::Status(current) if current == kind)
        });
        let summary = Signal::derive_local(move || {
            let s = summary_browser.status_summary(kind);
            (s.icon, s.count, s.label, summary_browser.compact())
        });

        let state = OverlayState::new(
            Binding::controlled(open, move |next: bool| {
                if !next {
                    close_browser.0.menus.close();
                }
            }),
            false,
            None,
        )
        .anchored_to(anchor)
        .provide();

        let title = i18n::text(kind.title());
        let a11y_title = title.clone();

        let button = view! {
            HintTooltip(text = title) {
                control(
                    node_ref = anchor,
                    class = "gds-files-status-button",
                    tabindex = Focus::Sequential,
                    a11y:role = Role::Button,
                    a11y:label = a11y_title,
                    on:click = move |_| {
                        toggle_browser.0.menus.surface.update(|surface| {
                            *surface = if matches!(surface, Surface::Status(current) if *current == kind) {
                                Surface::Closed
                            } else {
                                Surface::Status(kind)
                            };
                        });
                        if kind == Kind::Clipboard {
                            Clipboard::current().refresh();
                        }
                    }
                ) {
                    {{move || {
                        let (icon, count, label, compact) = summary.get();
                        view! {
                            row(class = "gds-files-status-button-inner") {
                                box(class = "gds-files-status-icon-wrap") {
                                    Icon(name = icon, size = ICON_SIZE) {}
                                    {{(compact && count > 0).then(|| AnyView::new(view! {
                                        box(class = "gds-files-status-badge") {
                                            text(class = "gds-files-status-badge-text") {{count.to_string()}}
                                        }
                                    }))}}
                                }
                                {{(!compact && !label.is_empty()).then(|| AnyView::new(view! {
                                    text(class = "gds-files-status-label") {{label}}
                                }))}}
                            }
                        }
                    }}}
                }
            }
        };

        let _ = state;
        AnyView::new(view! {
            box(class = "gds-files-status-slot") {
                {button}
                PopoverContent(
                    placement = Placement::new(Side::Top, Align::Start),
                    class = "gds-files-status-popover"
                ) {
                    {{
                        let b = details_browser.clone();
                        move || b.status_details(kind)
                    }}
                }
            }
        })
    }

    fn status_details(&self, kind: Kind) -> AnyView {
        let rows: Vec<Entry> = match kind {
            Kind::Tasks => {
                let task = self.read(|state| match &state.operation {
                    Operation::Idle => None,
                    Operation::Running { label, cancel, .. } => {
                        Some((label.clone(), cancel.clone()))
                    }
                });
                task.into_iter()
                    .map(|(label, cancel)| Entry::Task { label, cancel })
                    .collect()
            }
            Kind::Selection => {
                let (tab, rows) = self.read(|state| (state.tab().id, state.tab().selected()));
                rows.into_iter()
                    .map(|row| Entry::Selection { tab, row })
                    .collect()
            }
            Kind::Clipboard => Clipboard::current()
                .snapshot()
                .map_or_else(Vec::new, |snapshot| {
                    snapshot
                        .paths
                        .iter()
                        .cloned()
                        .map(|path| Entry::Clipboard {
                            expected: snapshot.clone(),
                            path,
                        })
                        .collect()
                }),
        };
        if rows.is_empty() {
            return AnyView::new(view! {
                text(class = "gds-files-status-empty") {{i18n::text(kind.empty())}}
            });
        }
        let rows = Rc::new(rows);
        let count = rows.len();
        let browser = self.clone();
        let height = (count as f32 * 32.0).min(312.0);
        AnyView::new(view! {
            box(
                class = "gds-files-status-list-wrap",
                style:height = format!("{height:.1}px")
            ) {
                VirtualList(
                    count = count,
                    row_size = 32.0,
                    label = i18n::text(kind.title()),
                    class = "gds-files-status-list",
                    row = move |index: usize| {
                        rows.get(index)
                            .map(|entry| browser.status_entry(entry))
                            .unwrap_or_else(|| AnyView::new(view! { box() }))
                    }
                )
            }
        })
    }

    fn status_entry(&self, entry: &Entry) -> AnyView {
        let title = entry.title();
        match entry {
            Entry::Task { label, cancel } => {
                let browser = self.clone();
                let cancel = cancel.clone();
                let cancelled = cancel.is_cancelled();
                let label = if cancelled {
                    format!("{label} — {}", i18n::text("tasks-cancelling"))
                } else {
                    label.clone()
                };
                self.status_row(
                    IconName::Activity,
                    label,
                    title,
                    AnyView::new(view! {
                        IconButton(
                            icon = IconName::Close,
                            label = i18n::text("operation-cancel"),
                            size = ButtonSize::IconXs,
                            disabled = Signal::stored_local(cancelled),
                            on:click = move |_| {
                                cancel.cancel();
                                browser.changed();
                            }
                        )
                    }),
                )
            }
            Entry::Selection { tab, row } => {
                let tab = *tab;
                let browser = self.clone();
                let key = row.key();
                self.status_row(
                    row.icon(),
                    row.label(),
                    title,
                    AnyView::new(view! {
                        IconButton(
                            icon = IconName::Close,
                            label = i18n::text("selection-remove-item"),
                            size = ButtonSize::IconXs,
                            on:click = move |_| {
                                browser.update_selection(|state| {
                                    if state.tab().id != tab {
                                        return;
                                    }
                                    let visit = state.tab_mut().navigation.view_mut();
                                    visit.selection.remove(&key);
                                    if visit.anchor.as_ref() == Some(&key) {
                                        visit.anchor = None;
                                    }
                                    if visit.cursor.as_ref() == Some(&key) {
                                        visit.cursor = None;
                                    }
                                });
                            }
                        )
                    }),
                )
            }
            Entry::Clipboard { expected, path } => {
                let path = path.clone();
                let label = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned();
                let expected = expected.clone();
                let browser = self.clone();
                self.status_row(
                    IconName::File,
                    label,
                    title,
                    AnyView::new(view! {
                        IconButton(
                            icon = IconName::Close,
                            label = i18n::text("clipboard-remove-item"),
                            size = ButtonSize::IconXs,
                            on:click = move |_| {
                                let notice = browser.clone();
                                let expected = expected.clone();
                                let path = path.clone();
                                browser.0.owner.with(|| {
                                    drop(spawn_local(async move {
                                        if let Err(error) =
                                            Clipboard::current().remove(expected, vec![path]).await
                                        {
                                            notice.report_notice(error.to_string());
                                        }
                                    }));
                                });
                            }
                        )
                    }),
                )
            }
        }
    }

    fn status_row(&self, icon: IconName, label: String, title: String, action: AnyView) -> AnyView {
        let tooltip = wrap_tokens(&title);
        AnyView::new(view! {
            HintTooltip(text = tooltip) {
                row(class = "gds-files-status-row") {
                    Icon(name = icon, size = ICON_SIZE, class = "gds-files-status-row-icon")
                    text(class = "gds-files-status-row-label") {{label}}
                    {action}
                }
            }
        })
    }
}
