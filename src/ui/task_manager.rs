//! Live, read-only view of the application's registered task scopes and tasks.

use std::collections::HashSet;
use std::time::Duration;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Memo, RwSignal, StoredValue, on_cleanup_local};
use zgui::view::install_stylesheet;
use zgui::view::time::{IntervalHandle, set_interval};
use zgui::{component, view};
use zgui_ui::prelude::*;

use crate::app::Gds;
use crate::i18n;
use crate::service::task_manager::{Execution, Snapshot as TaskSnapshot};
use crate::ui::icon::{IconName, IconProps};

pub(crate) const REFRESH: Duration = Duration::from_millis(250);
const ROW_HEIGHT: f32 = 30.0;
const SHEET: &str = "gds-task-manager";

const TASK_MANAGER_CSS: &str = "
.gds-tasks-root {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
}
.gds-tasks-header {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--gds-border);
    background-color: var(--gds-surface-muted);
    color: var(--gds-fg-muted);
    font-size: var(--gds-font-size-caption, 12px);
    font-weight: 500;
}
.gds-tasks-col-name {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.gds-tasks-col-exec {
    width: 120px;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.gds-tasks-col-age {
    width: 72px;
    flex-shrink: 0;
    text-align: right;
    white-space: nowrap;
}
.gds-tasks-body {
    flex: 1 1 0;
    min-height: 0;
    width: 100%;
    overflow: hidden;
    display: flex;
    flex-direction: column;
}
.gds-tasks-list {
    flex: 1 1 0;
    min-height: 0;
    width: 100%;
    height: 100%;
}
.gds-tasks-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 30px;
    padding-right: 12px;
    font-size: 12px;
    cursor: pointer;
    box-sizing: border-box;
}
.gds-tasks-row:hover {
    background-color: var(--gds-surface-hover);
}
.gds-tasks-row--selected {
    background-color: var(--zui-accent);
    color: var(--zui-accent-foreground);
}
.gds-tasks-row-name {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
}
.gds-tasks-row-title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.gds-tasks-row-id {
    color: var(--gds-fg-muted);
    font-family: var(--gds-font-mono);
    font-size: 11px;
    flex-shrink: 0;
}
.gds-tasks-empty {
    padding: 16px;
    font-size: 13px;
    color: var(--gds-fg-muted);
}
.gds-tasks-detail {
    max-height: 180px;
    flex-shrink: 0;
    border-top: 1px solid var(--gds-border);
    background-color: var(--gds-surface-muted);
    padding: 8px 12px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-family: var(--gds-font-mono);
    font-size: 12px;
}
.gds-tasks-detail-title {
    font-weight: 600;
    color: var(--gds-fg);
}
.gds-tasks-detail-source {
    color: var(--gds-fg-muted);
    white-space: pre-wrap;
    word-break: break-all;
}
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Scope(usize, usize),
    Task(usize, usize),
}

fn build_rows(snapshot: &TaskSnapshot, collapsed: &HashSet<u64>) -> Vec<Row> {
    fn visit(
        snapshot: &TaskSnapshot,
        collapsed: &HashSet<u64>,
        parent: Option<u64>,
        depth: usize,
        out: &mut Vec<Row>,
    ) {
        for (index, scope) in snapshot.scopes.iter().enumerate() {
            if scope.parent != parent {
                continue;
            }
            out.push(Row::Scope(index, depth));
            if collapsed.contains(&scope.id) {
                continue;
            }
            for (task_index, task) in snapshot.tasks.iter().enumerate() {
                if task.scope == scope.id {
                    out.push(Row::Task(task_index, depth + 1));
                }
            }
            visit(snapshot, collapsed, Some(scope.id), depth + 1, out);
        }
    }

    let mut rows = Vec::new();
    visit(snapshot, collapsed, None, 0, &mut rows);
    rows
}

fn execution_label(execution: Execution) -> String {
    match execution {
        Execution::Supervisor => i18n::text("logs-tasks-supervisor"),
        Execution::GpuiBackground => i18n::text("logs-tasks-gpui-background"),
        Execution::Tokio => i18n::text("logs-tasks-tokio"),
        Execution::Blocking => i18n::text("logs-tasks-blocking"),
        Execution::Thread => i18n::text("logs-tasks-thread"),
    }
}

#[cfg(test)]
pub(crate) mod test_probe {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    pub(crate) struct TaskProbe {
        pub snapshot: RwSignal<TaskSnapshot, LocalStorage>,
        pub collapsed: RwSignal<HashSet<u64>, LocalStorage>,
        pub selected: RwSignal<Option<u64>, LocalStorage>,
        pub row_count: Signal<usize, LocalStorage>,
        pub active: Rc<Cell<bool>>,
    }

    thread_local! {
        pub(crate) static PROBE: RefCell<Option<TaskProbe>> = const { RefCell::new(None) };
    }
}

/// Live task and scope hierarchy inspector bound to the application task supervisor.
#[component]
pub fn TaskManagerView() -> impl IntoView {
    install_stylesheet(SHEET, TASK_MANAGER_CSS);

    let inspector = Gds::try_current().and_then(|gds| gds.tasks().inspector());
    let initial = inspector
        .as_ref()
        .map(crate::service::task_manager::Inspector::snapshot)
        .unwrap_or_default();

    let snapshot = RwSignal::new_local(initial);
    let collapsed = RwSignal::new_local(HashSet::<u64>::new());
    let selected = RwSignal::new_local(None::<u64>);

    let rows: Memo<Vec<Row>> =
        Memo::new(move |_| snapshot.with(|snap| collapsed.with(|hidden| build_rows(snap, hidden))));
    let row_count: Signal<usize, LocalStorage> = Signal::derive_local(move || rows.with(Vec::len));

    let refresh_handle: Option<IntervalHandle> = inspector.as_ref().map(|insp| {
        let inspector = insp.clone();
        set_interval(REFRESH, move || {
            let next = inspector.snapshot();
            collapsed.update(|hidden| {
                hidden.retain(|id| next.scopes.iter().any(|scope| scope.id == *id));
            });
            if let Some(current_selected) = selected.get_untracked()
                && !next.tasks.iter().any(|task| task.id == current_selected)
            {
                selected.set(None);
            }
            snapshot.set(next);
        })
    });
    let handle_store = StoredValue::new_local(refresh_handle);
    #[cfg(test)]
    let probe_active = std::rc::Rc::new(std::cell::Cell::new(true));
    #[cfg(test)]
    {
        let active_flag = std::rc::Rc::clone(&probe_active);
        test_probe::PROBE.with(|slot| {
            *slot.borrow_mut() = Some(test_probe::TaskProbe {
                snapshot,
                collapsed,
                selected,
                row_count,
                active: active_flag,
            });
        });
    }
    on_cleanup_local(move || {
        handle_store.update_value(|slot| {
            *slot = None;
        });
        #[cfg(test)]
        probe_active.set(false);
    });

    let list_view = move || {
        if row_count.get() == 0 {
            return AnyView::new(view! {
                box(class = "gds-tasks-empty", attr:data-testid = "logs-tasks-empty") {
                    text {{move || i18n::text("logs-tasks-empty")}}
                }
            });
        }

        AnyView::new(view! {
            VirtualList(
                count = row_count,
                row_size = ROW_HEIGHT,
                overscan = 4,
                label = "Tasks",
                class = "gds-tasks-list",
                row = move |index| {
                    let row_content = move || {
                        let row = rows.with(|items| items.get(index).copied())?;
                        match row {
                            Row::Scope(scope_idx, depth) => {
                                let (id, name, cancelling) = snapshot.with(|snap| {
                                    let scope = &snap.scopes[scope_idx];
                                    (scope.id, scope.name.clone(), scope.cancelling)
                                });
                                let is_collapsed = collapsed.with(|hidden| hidden.contains(&id));
                                let indent_style = format!("padding-left: {}px;", 12.0 + depth as f32 * 16.0);
                                let chevron = if is_collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                };
                                let badge_variant = if cancelling {
                                    BadgeVariant::Destructive
                                } else {
                                    BadgeVariant::Secondary
                                };
                                let badge_label = if cancelling {
                                    i18n::text("logs-tasks-scope-closing")
                                } else {
                                    i18n::text("logs-tasks-scope-open")
                                };
                                Some(AnyView::new(view! {
                                    box(
                                        class = "gds-tasks-row",
                                        style = indent_style,
                                        attr:data-task-scope = {id.to_string()},
                                        on:click = move |_| {
                                            collapsed.update(|hidden| {
                                                if !hidden.remove(&id) {
                                                    hidden.insert(id);
                                                }
                                            });
                                        },
                                    ) {
                                        box(class = "gds-tasks-row-name") {
                                            Icon(name = chevron, size = 14.0)
                                            text(class = "gds-tasks-row-title") {{name}}
                                            text(class = "gds-tasks-row-id") {{format!("#{id}")}}
                                            Badge(variant = badge_variant) {{badge_label}}
                                        }
                                    }
                                }))
                            }
                            Row::Task(task_idx, depth) => {
                                let (id, name, cancelling, exec, age_secs) = snapshot.with(|snap| {
                                    let task = &snap.tasks[task_idx];
                                    (
                                        task.id,
                                        task.name.clone(),
                                        task.cancelling,
                                        task.execution,
                                        task.age.as_secs_f32(),
                                    )
                                });
                                let is_selected = selected.get() == Some(id);
                                let indent_style = format!("padding-left: {}px;", 12.0 + depth as f32 * 16.0);
                                let badge_variant = if cancelling {
                                    BadgeVariant::Destructive
                                } else {
                                    BadgeVariant::Default
                                };
                                let status_label = if cancelling {
                                    i18n::text("logs-tasks-cancelling")
                                } else {
                                    i18n::text("logs-tasks-active")
                                };
                                Some(AnyView::new(view! {
                                    box(
                                        class = "gds-tasks-row",
                                        class = if is_selected {
                                            "gds-tasks-row--selected"
                                        } else {
                                            ""
                                        },
                                        style = indent_style,
                                        attr:data-task-entry = {id.to_string()},
                                        on:click = move |_| {
                                            selected.set(Some(id));
                                        },
                                    ) {
                                        box(class = "gds-tasks-row-name") {
                                            text(class = "gds-tasks-row-title") {{name}}
                                            text(class = "gds-tasks-row-id") {{format!("#{id}")}}
                                            Badge(variant = badge_variant) {{status_label}}
                                        }
                                        text(class = "gds-tasks-col-exec") {{execution_label(exec)}}
                                        text(class = "gds-tasks-col-age") {{format!("{age_secs:.1} s")}}
                                    }
                                }))
                            }
                        }
                    };
                    view! { {row_content} }
                },
            )
        })
    };

    let detail_view = move || {
        let sel = selected.get()?;
        let (name, id, source) = snapshot.with(|snap| {
            snap.tasks
                .iter()
                .find(|task| task.id == sel)
                .map(|task| (task.name.clone(), task.id, task.source.to_string()))
        })?;
        Some(view! {
            box(class = "gds-tasks-detail", attr:data-testid = "tasks-detail") {
                text(class = "gds-tasks-detail-title") {{format!("{name} · #{id}")}}
                text(class = "gds-tasks-detail-source") {{source}}
            }
        })
    };

    view! {
        box(class = "gds-tasks-root", attr:data-testid = "tasks-panel") {
            box(class = "gds-tasks-header") {
                text(class = "gds-tasks-col-name") {{move || i18n::text("logs-tasks-name")}}
                text(class = "gds-tasks-col-exec") {{move || i18n::text("logs-tasks-execution")}}
                text(class = "gds-tasks-col-age") {{move || i18n::text("logs-tasks-age")}}
            }
            box(class = "gds-tasks-body") {
                {list_view}
            }
            {detail_view}
        }
    }
}
