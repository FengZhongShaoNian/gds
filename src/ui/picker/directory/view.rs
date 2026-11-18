use std::cell::Cell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Signal, UnsyncCallback};
use zgui::view::focus_is_text_entry;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, css, view};
use zgui_ui::{
    BreadcrumbItemProps, BreadcrumbLinkProps, BreadcrumbListProps, BreadcrumbPageProps,
    BreadcrumbProps, BreadcrumbSeparatorProps, ButtonProps, ButtonSize, ButtonVariant,
    EmptyContentProps, EmptyDescriptionProps, EmptyHeaderProps, EmptyProps, EmptyTitleProps,
    InputProps, SpinnerProps, VirtualListProps,
};
use zgui_ui_primitives::Binding;

use super::commands::PickerCommand;
use super::table::DirectoryRows;
use super::{DirectoryPicker, Location, ROW_HEIGHT};
use crate::app::Gds;
use crate::ui::button::IconButtonProps;
use crate::ui::dialog::{
    DialogActionsProps, DialogAnchor, DialogContentProps, DialogDecision, DialogFooterProps,
    DialogHeaderProps, DialogProps, DialogTitleProps,
};
use crate::ui::icon::{IconName, IconProps};
use crate::ui::picker::{
    PickerContentProps, PickerLocationProps, PickerProps, PickerStatusProps, PickerToolbarProps,
};

const SHEET: &str = css!(
    r#"
    .gds-directory-picker {
        width: 100%;
    }

    .gds-picker-filter-row {
        display: flex;
        align-items: center;
        gap: 6px;
    }

    .gds-picker-filter-input,
    .gds-picker-address-input {
        flex: 1;
        min-width: 0;
    }

    .gds-picker-address-row {
        display: flex;
        align-items: center;
        gap: 6px;
        flex: 1;
        min-width: 0;
    }

    .gds-picker-breadcrumbs {
        flex: 1;
        min-width: 0;
    }

    .gds-picker-list {
        flex: 1;
        width: 100%;
        height: 260px;
    }

    .gds-picker-row {
        display: flex;
        align-items: center;
        gap: 8px;
        width: 100%;
        height: 100%;
        padding: 0 10px;
        cursor: pointer;
        border-radius: 4px;
        color: var(--gds-foreground);
    }

    .gds-picker-row:hover {
        background: var(--gds-panel);
    }

    .gds-picker-row[data-selected="true"] {
        background: var(--gds-selection);
        color: var(--gds-selection-foreground);
    }

    .gds-picker-row__label {
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: 13px;
    }

    .gds-picker-center {
        flex: 1;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 8px;
        padding: 20px;
    }

    .gds-picker-dialog {
        width: 600px;
        max-width: calc(100vw - 32px);
    }
    "#
);

fn install_directory_picker_stylesheets() {
    if zgui::view::current_host().is_some() {
        crate::ui::styles::install_ui_stylesheets();
        zgui::view::install_stylesheet("gds-ui-directory-picker", SHEET);
    }
}

fn text_or(key: &'static str, fallback: &'static str) -> String {
    Gds::try_current().map_or_else(|| fallback.to_owned(), |gds| gds.i18n().text(key))
}

fn handle_picker_keys(
    picker: &DirectoryPicker,
    address_ref: &NodeRef,
    filter_ref: &NodeRef,
    ev: &mut EventCx<'_, zgui::view::events::KeyDown>,
) {
    if PickerCommand::Back.matches_event(ev, ev.modifiers) {
        picker.back();
        ev.prevent_default();
        ev.stop_propagation();
        return;
    }
    if PickerCommand::Forward.matches_event(ev, ev.modifiers) {
        picker.forward();
        ev.prevent_default();
        ev.stop_propagation();
        return;
    }
    if PickerCommand::Up.matches_event(ev, ev.modifiers) {
        picker.parent();
        ev.prevent_default();
        ev.stop_propagation();
        return;
    }
    if PickerCommand::Refresh.matches_event(ev, ev.modifiers) {
        picker.refresh();
        ev.prevent_default();
        ev.stop_propagation();
        return;
    }
    if PickerCommand::EditLocation.matches_event(ev, ev.modifiers) {
        picker.begin_edit_location();
        address_ref.focus();
        address_ref.select_all();
        ev.prevent_default();
        ev.stop_propagation();
        return;
    }
    if PickerCommand::Filter.matches_event(ev, ev.modifiers) {
        filter_ref.focus();
        filter_ref.select_all();
        ev.prevent_default();
        ev.stop_propagation();
        return;
    }

    let mods = ev.modifiers.held();
    if !ev.repeat
        && (mods == zgui::vocab::Modifiers::PRIMARY || mods == zgui::vocab::Modifiers::CONTROL)
        && matches!(&ev.key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("h"))
    {
        picker.toggle_hidden();
        ev.prevent_default();
        ev.stop_propagation();
        return;
    }

    if focus_is_text_entry() || !mods.is_empty() {
        return;
    }

    match &ev.key {
        Key::Named(NamedKey::ArrowDown) => {
            picker.move_selection(1);
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::ArrowUp) => {
            picker.move_selection(-1);
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::Home) => {
            picker.select_first();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::End) => {
            picker.select_last();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::Enter) => {
            picker.open_selected();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::Backspace) if !ev.repeat => {
            picker.parent();
            ev.prevent_default();
            ev.stop_propagation();
        }
        _ => {}
    }
}

/// Directory chooser view component driven by [`DirectoryPicker`].
#[component]
pub fn DirectoryPickerView(
    picker: DirectoryPicker,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    install_directory_picker_stylesheets();

    picker.start();

    let address_ref = NodeRef::new();
    let filter_ref = NodeRef::new();
    let list_ref = NodeRef::new();

    let page = picker.page();
    let rows = picker.rows();
    let selected_index = picker.selected_index();
    let is_loading = picker.is_loading();
    let loading_more = picker.is_loading_more();
    let error = picker.error();
    let show_hidden = picker.show_hidden();
    let filter_query = picker.filter_query();
    let editing = picker.is_editing_location();
    let address_text = picker.address_text();
    let can_back = picker.can_go_back();
    let can_forward = picker.can_go_forward();
    let can_parent = picker.can_go_parent();

    let back_label = text_or(PickerCommand::Back.label_id(), "Back");
    let forward_label = text_or(PickerCommand::Forward.label_id(), "Forward");
    let parent_label = text_or(PickerCommand::Up.label_id(), "Parent folder");
    let location_label = Signal::stored_local(text_or(
        PickerCommand::EditLocation.label_id(),
        "Edit location",
    ));
    let stop_label = text_or("picker-stop", "Stop loading");
    let refresh_label = text_or(PickerCommand::Refresh.label_id(), "Refresh");
    let filter_placeholder = text_or(PickerCommand::Filter.label_id(), "Filter folders…");
    let hidden_label = text_or("picker-hidden", "Show hidden folders");
    let loading_label = Signal::stored_local(text_or("picker-loading", "Loading folders…"));
    let failed_title = Signal::stored_local(text_or("picker-failed", "Cannot open this folder"));
    let retry_label = Signal::stored_local(text_or("picker-retry", "Retry"));
    let empty_title = Signal::stored_local(text_or("picker-empty", "No visible folders"));
    let no_matches_title =
        Signal::stored_local(text_or("picker-no-matches", "No matching folders"));
    let folders_label = Signal::stored_local(text_or("picker-folders", "Folders"));
    let confirm_label = Signal::stored_local(text_or("dialog-confirm", "Confirm"));

    let picker_keys = picker.clone();
    let addr_key_ref = address_ref;
    let filter_key_ref = filter_ref;

    let picker_back = picker.clone();
    let picker_fwd = picker.clone();
    let picker_up = picker.clone();
    let picker_edit = picker.clone();
    let addr_focus_ref = address_ref;
    let picker_refresh = picker.clone();
    let picker_hidden = picker.clone();
    let picker_filter = picker.clone();
    let picker_addr_input = picker.clone();
    let picker_addr_submit = picker.clone();
    let picker_addr_cancel = picker.clone();
    let picker_addr_btn = picker.clone();
    let picker_crumb = picker.clone();
    let picker_retry = picker.clone();
    let picker_row = picker.clone();
    let picker_scroll = picker.clone();

    let row_count = Signal::derive_local(move || rows.with(DirectoryRows::len));

    view! {
        Picker(
            class = "gds-directory-picker",
            class = class,
            on:key_down = move |ev: &mut EventCx<'_, zgui::view::events::KeyDown>| {
                handle_picker_keys(&picker_keys, &addr_key_ref, &filter_key_ref, ev);
            }
        ) {
            PickerToolbar {
                IconButton(
                    icon = IconName::ArrowLeft,
                    label = back_label,
                    shortcut = PickerCommand::Back.default_stroke(),
                    disabled = Signal::derive_local(move || !can_back.get()),
                    on:click = move |_| picker_back.back(),
                )
                IconButton(
                    icon = IconName::ArrowRight,
                    label = forward_label,
                    shortcut = PickerCommand::Forward.default_stroke(),
                    disabled = Signal::derive_local(move || !can_forward.get()),
                    on:click = move |_| picker_fwd.forward(),
                )
                IconButton(
                    icon = IconName::CornerLeftUp,
                    label = parent_label,
                    shortcut = PickerCommand::Up.default_stroke(),
                    disabled = Signal::derive_local(move || !can_parent.get()),
                    on:click = move |_| picker_up.parent(),
                )

                PickerLocation {
                    {move || {
                        if editing.get() {
                            let p_input = picker_addr_input.clone();
                            let p_key_enter = picker_addr_submit.clone();
                            let p_key_esc = picker_addr_cancel.clone();
                            let p_btn = picker_addr_btn.clone();
                            AnyView::new(view! {
                                row(class = "gds-picker-address-row") {
                                    Input(
                                        node_ref = address_ref,
                                        class = "gds-picker-address-input",
                                        value = Binding::controlled(
                                            address_text,
                                            move |next| p_input.set_address(next),
                                        ),
                                        label = location_label.get_untracked(),
                                        on:key_down = move |ev: &mut EventCx<'_, zgui::view::events::KeyDown>| {
                                            if ev.key == Key::Named(NamedKey::Enter) && !ev.repeat {
                                                p_key_enter.apply_address(address_text.get_untracked());
                                                ev.prevent_default();
                                                ev.stop_propagation();
                                            } else if ev.key == Key::Named(NamedKey::Escape) && !ev.repeat {
                                                p_key_esc.cancel_edit_location();
                                                ev.prevent_default();
                                                ev.stop_propagation();
                                            }
                                        }
                                    )
                                    Button(
                                        variant = ButtonVariant::Secondary,
                                        size = ButtonSize::Sm,
                                        on:click = move |_| p_btn.apply_address(address_text.get_untracked()),
                                    ) {{confirm_label.get_untracked()}}
                                }
                            })
                        } else {
                            let p_edit = picker_edit.clone();
                            let a_ref = addr_focus_ref;
                            let p_crumb = picker_crumb.clone();
                            AnyView::new(view! {
                                row(class = "gds-picker-address-row") {
                                    Breadcrumb(
                                        class = "gds-picker-breadcrumbs",
                                        label = location_label.get_untracked(),
                                    ) {
                                        BreadcrumbList {
                                            {move || {
                                                let Some(current_page) = page.get() else {
                                                    return AnyView::new(());
                                                };
                                                let ancestors = current_page.ancestors;
                                                let current_label = current_page.location.label;
                                                let p_nav = Signal::stored_local(p_crumb.clone());
                                                AnyView::new(view! {
                                                    for item in move || {
                                                        ancestors.clone().into_iter().enumerate().collect::<Vec<_>>()
                                                    }, key = |(idx, loc)| (*idx, loc.path.clone()) {
                                                        BreadcrumbItem {
                                                            BreadcrumbLink(
                                                                on:click = {
                                                                    let nav_path = item.1.path.clone();
                                                                    let nav_picker = p_nav.get_untracked();
                                                                    move |_| nav_picker.navigate(nav_path.clone())
                                                                },
                                                            ) {{item.1.label}}
                                                        }
                                                        BreadcrumbSeparator()
                                                    }
                                                    BreadcrumbItem {
                                                        BreadcrumbPage {{current_label}}
                                                    }
                                                })
                                            }}
                                        }
                                    }
                                    IconButton(
                                        icon = IconName::Pencil,
                                        label = location_label,
                                        shortcut = PickerCommand::EditLocation.default_stroke(),
                                        on:click = move |_| {
                                            p_edit.begin_edit_location();
                                            a_ref.focus();
                                            a_ref.select_all();
                                        },
                                    )
                                }
                            })
                        }
                    }}
                }

                IconButton(
                    icon = Signal::derive_local(move || {
                        if is_loading.get() {
                            IconName::X
                        } else {
                            IconName::RefreshCw
                        }
                    }),
                    label = Signal::derive_local(move || {
                        if is_loading.get() {
                            stop_label.clone()
                        } else {
                            refresh_label.clone()
                        }
                    }),
                    shortcut = PickerCommand::Refresh.default_stroke(),
                    on:click = move |_| {
                        if picker_refresh.is_loading().get_untracked() {
                            picker_refresh.cancel_loading();
                        } else {
                            picker_refresh.refresh();
                        }
                    },
                )
                IconButton(
                    icon = IconName::Eye,
                    label = hidden_label,
                    toggled = show_hidden,
                    on:click = move |_| picker_hidden.toggle_hidden(),
                )
            }

            row(class = "gds-picker-filter-row") {
                Icon(name = IconName::Search)
                Input(
                    node_ref = filter_ref,
                    class = "gds-picker-filter-input",
                    placeholder = filter_placeholder.clone(),
                    label = filter_placeholder.clone(),
                    value = Binding::controlled(
                        filter_query,
                        move |next| picker_filter.set_filter(next),
                    ),
                )
            }

            PickerContent {
                {move || {
                    if let Some(err_msg) = error.get() {
                        let p_retry = picker_retry.clone();
                        return AnyView::new(view! {
                            box(class = "gds-picker-center") {
                                Empty {
                                    EmptyHeader {
                                        EmptyTitle {{failed_title.get_untracked()}}
                                        EmptyDescription {{err_msg}}
                                    }
                                    EmptyContent {
                                        Button(
                                            variant = ButtonVariant::Outline,
                                            on:click = move |_| p_retry.refresh(),
                                        ) {{retry_label.get_untracked()}}
                                    }
                                }
                            }
                        });
                    }

                    if is_loading.get() && rows.with(DirectoryRows::is_empty) {
                        return AnyView::new(view! {
                            row(class = "gds-picker-center") {
                                Spinner(label = loading_label.get_untracked())
                                text {{loading_label.get_untracked()}}
                            }
                        });
                    }

                    if rows.with(DirectoryRows::is_empty) {
                        let msg = if filter_query.get().trim().is_empty() {
                            empty_title.get_untracked()
                        } else {
                            no_matches_title.get_untracked()
                        };
                        return AnyView::new(view! {
                            box(class = "gds-picker-center") {
                                Empty {
                                    EmptyHeader {
                                        EmptyTitle {{msg}}
                                    }
                                }
                            }
                        });
                    }

                    let p_row = picker_row.clone();
                    let p_scroll = picker_scroll.clone();
                    let l_ref = list_ref;
                    AnyView::new(view! {
                        VirtualList(
                            node_ref = list_ref,
                            class = "gds-picker-list",
                            count = row_count,
                            row_size = Signal::stored_local(ROW_HEIGHT),
                            overscan = 4,
                            label = folders_label.get_untracked(),
                            on:scroll = move |_| {
                                let pos = l_ref.scroll_position();
                                if pos.content_size.height.0 > 0.0
                                    && pos.remaining().height.0 <= ROW_HEIGHT * 3.0 * l_ref.scale()
                                {
                                    p_scroll.load_more();
                                }
                            },
                            row = move |index: usize| {
                                let entry = rows.with(|r| r.get(index).cloned());
                                let Some(entry) = entry else {
                                    return AnyView::new(view! { box() });
                                };
                                let path = entry.location.path;
                                let label = entry.location.label;
                                let p_click = p_row.clone();
                                let p_dbl = p_row.clone();
                                let open_path = path.clone();
                                AnyView::new(view! {
                                    control(
                                        class = "gds-picker-row",
                                        tabindex = Focus::Sequential,
                                        a11y:role = Role::ListItem,
                                        a11y:label = label.clone(),
                                        attr:data-selected = move || {
                                            (selected_index.get() == Some(index)).then(|| "true".to_owned())
                                        },
                                        on:click = move |_| p_click.select_row(index),
                                        on:double_click = move |_| p_dbl.navigate(open_path.clone()),
                                    ) {
                                        Icon(name = IconName::Folder)
                                        text(class = "gds-picker-row__label") {{label}}
                                    }
                                })
                            }
                        )
                    })
                }}
            }

            PickerStatus {
                text {{move || {
                    let count = rows.with(DirectoryRows::len);
                    format!("{count} {}", folders_label.get())
                }}}
                {move || {
                    if loading_more.get() {
                        AnyView::new(view! { Spinner(label = loading_label.get_untracked()) })
                    } else {
                        let summary = page.get().map_or_else(String::new, |p| p.location.path);
                        AnyView::new(view! { text {{summary}} })
                    }
                }}
            }
        }
    }
}

/// Modal directory chooser dialog built over [`zgui_ui::Dialog`] and [`DirectoryPickerView`].
#[component]
pub fn DirectoryPickerDialog(
    #[prop(into, optional)] open: Binding<bool>,
    picker: DirectoryPicker,
    #[prop(into, optional)] title: Option<Signal<String, LocalStorage>>,
    #[prop(into, optional)] confirm_label: Option<Signal<String, LocalStorage>>,
    #[prop(optional)] anchor: Option<DialogAnchor>,
    on_select: UnsyncCallback<Location>,
    #[prop(optional)] on_cancel: Option<UnsyncCallback<()>>,
) -> impl IntoView {
    install_directory_picker_stylesheets();

    let title =
        title.unwrap_or_else(|| Signal::stored_local(text_or("picker-title", "Select folder")));
    let confirm_label = confirm_label
        .unwrap_or_else(|| Signal::stored_local(text_or("picker-select", "Select folder")));
    let cancel_label = text_or("dialog-cancel", "Cancel");
    let cancel_sig = Signal::stored_local(cancel_label);

    let settled = Rc::new(Cell::new(false));
    let picker_confirm = picker.clone();
    let picker_can_confirm = picker.clone();

    let on_decision = {
        let settled = Rc::clone(&settled);
        UnsyncCallback::new(move |decision: DialogDecision| {
            if settled.replace(true) {
                return;
            }
            open.write(false);
            match decision {
                DialogDecision::Confirm => {
                    if let Some(location) = picker_confirm.selected_location() {
                        on_select.run(location);
                    }
                }
                DialogDecision::Cancel | DialogDecision::Dismiss => {
                    if let Some(on_cancel) = on_cancel {
                        on_cancel.run(());
                    }
                }
            }
            if let Some(anchor) = anchor {
                anchor.restore();
            }
        })
    };

    let on_open_change = {
        let settled = Rc::clone(&settled);
        UnsyncCallback::new(move |next_open: bool| {
            open.write(next_open);
            if next_open {
                settled.set(false);
            } else if !settled.replace(true) {
                if let Some(on_cancel) = on_cancel {
                    on_cancel.run(());
                }
                if let Some(anchor) = anchor {
                    anchor.restore();
                }
            }
        })
    };

    let disabled = Signal::derive_local(move || picker_can_confirm.page().with(Option::is_none));

    view! {
        Dialog(open = open, on_open_change = on_open_change) {
            DialogContent(
                class = "gds-picker-dialog",
                dismiss_on_outside_press = true,
                dismiss_on_escape = true,
            ) {
                DialogHeader {
                    DialogTitle {{move || title.get()}}
                }
                DirectoryPickerView(picker = picker.clone())
                DialogFooter {
                    DialogActions(
                        confirm_label = confirm_label,
                        cancel_label = cancel_sig.get_untracked(),
                        disabled = disabled,
                        on_decision = on_decision,
                    )
                }
            }
        }
    }
}
