//! Keyboard reference assembled from the same command metadata as the workspace.

use gds::i18n;
use gds::ui::Shortcut;
use gds::ui::dialog::DialogCommand;
use gds::ui::help::{HelpPageProps, ShortcutHeadingProps, ShortcutRowProps, ShortcutSectionProps};
use zgui::prelude::*;
use zgui::vocab::{Key, Modifiers, NamedKey};
use zgui::{component, view};

use crate::shared::ui::actions::Command;

const SECTIONS: &[(&str, &[Command])] = &[
    (
        "help-application",
        &[
            Command::Menu,
            Command::Settings,
            Command::Help,
            Command::Cancel,
        ],
    ),
    (
        "help-tabs",
        &[
            Command::NewTab,
            Command::CloseTab,
            Command::NextTab,
            Command::PreviousTab,
        ],
    ),
    (
        "help-navigation",
        &[
            Command::EditLocation,
            Command::Back,
            Command::Forward,
            Command::Parent,
            Command::Refresh,
            Command::Favorite,
        ],
    ),
    (
        "help-view",
        &[
            Command::Sidebar,
            Command::Details,
            Command::ViewMode,
            Command::Columns,
            Command::Sort,
            Command::Filter,
            Command::Hidden,
        ],
    ),
    (
        "help-files",
        &[
            Command::NewFolder,
            Command::Open,
            Command::OpenInNewTab,
            Command::Copy,
            Command::Cut,
            Command::Paste,
            Command::CopyPaths,
            Command::Rename,
            Command::Trash,
            Command::DeletePermanent,
        ],
    ),
    ("help-history", &[Command::Undo, Command::Redo]),
    (
        "help-selection",
        &[
            Command::SelectAll,
            Command::DeselectAll,
            Command::ContextMenu,
        ],
    ),
    (
        "help-locations",
        &[
            Command::MountVolume,
            Command::UnmountVolume,
            Command::ClearRecent,
        ],
    ),
    (
        "help-trash",
        &[
            Command::RestoreSelection,
            Command::PurgeSelection,
            Command::EmptyTrash,
        ],
    ),
    ("help-terminal", &[Command::Terminal]),
    (
        "help-focus",
        &[
            Command::FocusLocations,
            Command::FocusVolumes,
            Command::FocusTabs,
            Command::FocusContent,
        ],
    ),
    (
        "help-notifications",
        &[
            Command::FocusNotifications,
            Command::CloseNotification,
            Command::CloseAllNotifications,
        ],
    ),
];

fn dialog_row(command: DialogCommand) -> (&'static str, Shortcut) {
    match command {
        DialogCommand::Confirm => (
            "dialog-confirm",
            Shortcut::new(Key::Named(NamedKey::Enter), Modifiers::NONE),
        ),
        DialogCommand::Cancel => (
            "close",
            Shortcut::new(Key::Named(NamedKey::Escape), Modifiers::NONE),
        ),
    }
}

/// Renders the complete keyboard shortcut reference page for GDS Files.
#[component]
pub(crate) fn FilesHelp() -> impl IntoView {
    let section_views = SECTIONS
        .iter()
        .map(|&(title, commands)| {
            let rows = commands
                .iter()
                .map(|&command| {
                    view! {
                        ShortcutRow(
                            label = Signal::derive_local(move || i18n::text(command.label_id())),
                            shortcut = command.shortcut(),
                        )
                    }
                    .into_view()
                })
                .collect::<Vec<_>>();
            let extra_tabs = (title == "help-tabs").then(|| {
                view! {
                    ShortcutRow(
                        label = Signal::derive_local(|| i18n::text("help-select-tab")),
                        shortcut = Shortcut::new(Key::character("1"), Modifiers::CONTROL),
                        range_end = Shortcut::new(Key::character("9"), Modifiers::CONTROL),
                    )
                }
                .into_view()
            });
            let extra_terminal = (title == "help-terminal").then(|| {
                vec![
                    view! {
                        ShortcutRow(
                            label = Signal::derive_local(|| i18n::text("files-terminal-new")),
                            shortcut = Shortcut::new(Key::character("n"), Modifiers::CONTROL),
                        )
                    }
                    .into_view(),
                    view! {
                        ShortcutRow(
                            label = Signal::derive_local(|| i18n::text("files-terminal-end")),
                            shortcut = Shortcut::new(Key::character("w"), Modifiers::CONTROL),
                        )
                    }
                    .into_view(),
                ]
            });
            view! {
                ShortcutSection {
                    ShortcutHeading {{move || i18n::text(title)}}
                    {rows}
                    {extra_tabs}
                    {extra_terminal}
                }
            }
            .into_view()
        })
        .collect::<Vec<_>>();

    let dialog_rows = [DialogCommand::Confirm, DialogCommand::Cancel]
        .into_iter()
        .map(|command| {
            let (label_id, shortcut) = dialog_row(command);
            view! {
                ShortcutRow(
                    label = Signal::derive_local(move || i18n::text(label_id)),
                    shortcut = shortcut,
                )
            }
            .into_view()
        })
        .collect::<Vec<_>>();

    view! {
        HelpPage {
            {section_views}
            ShortcutSection {
                ShortcutHeading {{move || i18n::text("help-dialogs")}}
                {dialog_rows}
            }
        }
    }
}
