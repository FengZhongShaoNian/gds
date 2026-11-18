//! Network location surface showing saved connections, active sessions, and mounted shares.

use std::collections::HashSet;
use std::path::PathBuf;

use gds::i18n;
use gds::service::remote::Address;
use gds::ui::button::{ButtonSize, ButtonVariant};
use gds::ui::page::{
    ContentPageProps, PageActionsProps, PageContentProps, PageHeaderProps, PageNoticeProps,
    PageTitleProps,
};
use gds::ui::surface::CardSurfaceProps;
use gds::ui::{IconButtonProps, IconName, IconProps};
use zgui::prelude::*;
use zgui::view::{current_host, install_stylesheet};
use zgui::{css, view};
use zgui_ui::{ButtonProps, SpinnerProps};

use crate::entities::location::Location;
use crate::features::connect_server::Intent;
use crate::pages::workspace::{model::Contents, ui::Browser};

const NETWORK_SHEET: &str = "gds-files-workspace-network";

const NETWORK_CSS: &str = css!(
    r#"
    .gds-files-network-scroll {
        flex: 1 1 0%;
        min-height: 0;
        overflow-y: auto;
        gap: 16px;
        padding-bottom: 12px;
    }

    .gds-files-network-section {
        gap: 8px;
    }

    .gds-files-network-section__title {
        font-size: var(--gds-font-size-xs, 12px);
        font-weight: 600;
        color: var(--gds-muted-foreground);
    }

    .gds-files-network-list {
        gap: 8px;
    }

    .gds-files-network-card {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 10px 12px;
    }

    .gds-files-network-card__leading {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 10px;
        min-width: 0;
        flex: 1 1 0%;
    }

    .gds-files-network-card__info {
        min-width: 0;
        flex: 1 1 0%;
        gap: 2px;
    }

    .gds-files-network-card__title {
        font-size: var(--gds-font-size-sm, 14px);
        font-weight: 500;
        color: var(--gds-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-network-card__detail {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-network-card__status {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
        padding: 2px 8px;
        border-radius: var(--gds-radius-control, 6px);
        background: var(--gds-surface);
        border: 1px solid var(--gds-border);
        flex-shrink: 0;
    }

    .gds-files-network-card__status[data-connected="true"] {
        color: var(--gds-primary);
        border-color: var(--gds-primary);
    }

    .gds-files-network-card__actions {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 6px;
        flex-shrink: 0;
    }

    .gds-files-network-notice {
        align-items: center;
        justify-content: space-between;
        gap: 10px;
    }

    .gds-files-network-notice--error {
        border-color: var(--gds-error);
    }

    .gds-files-network-notice__lead {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 8px;
        min-width: 0;
        flex: 1 1 0%;
    }

    .gds-files-network-notice__text {
        font-size: var(--gds-font-size-sm, 14px);
        color: var(--gds-foreground);
    }

    .gds-files-network-empty {
        flex: 1 1 0%;
        min-height: 220px;
        align-items: center;
        justify-content: center;
        gap: 10px;
        padding: 24px;
    }

    .gds-files-network-empty__icon {
        width: 32px;
        height: 32px;
        color: var(--gds-muted-foreground);
    }

    .gds-files-network-empty__text {
        font-size: var(--gds-font-size-sm, 14px);
        color: var(--gds-muted-foreground);
    }
    "#
);

fn install_network_stylesheet() {
    if current_host().is_some() {
        install_stylesheet(NETWORK_SHEET, NETWORK_CSS);
    }
}

fn display_address_title(address: &Address) -> String {
    let name = address.name();
    if name.is_empty() || name == "/" {
        address.host().to_owned()
    } else {
        name
    }
}

fn open_add_location_prompt(browser: Browser) {
    let target = browser.clone();
    browser
        .0
        .connector
        .prompt(None, Intent::Add, move |result| match result {
            Ok(address) => {
                target.update(|state| {
                    state.places.set_remote_favorite(address, true);
                });
                target.save_places();
                target.refresh();
            }
            Err(message) => {
                if !message.is_empty() {
                    target.report_notice(message);
                }
            }
        });
}

fn connect_and_open(browser: Browser, address: Address) {
    let target = browser.clone();
    browser
        .0
        .connector
        .prompt(Some(address), Intent::Open, move |result| match result {
            Ok(connected) => target.open(Location::Remote(connected)),
            Err(message) => {
                if !message.is_empty() {
                    target.report_notice(message);
                }
            }
        });
}

fn disconnect_address(browser: Browser, address: Address) {
    let target = browser.clone();
    browser.0.connector.disconnect(address, move |result| {
        if let Err(message) = result {
            target.report_notice(message);
        }
        target.changed();
    });
}

fn remote_location_card(
    browser: Browser,
    address: Address,
    saved: bool,
    connected: bool,
    connecting: bool,
) -> AnyView {
    let title = display_address_title(&address);
    let uri = address.uri().to_owned();
    let status = if connecting {
        i18n::text("remote-connecting")
    } else if connected {
        i18n::text("remote-connected")
    } else {
        i18n::text("remote-disconnected")
    };

    let open_addr = address.clone();
    let open_browser = browser.clone();
    let connect_addr = address.clone();
    let connect_browser = browser.clone();
    let disconnect_addr = address.clone();
    let disconnect_browser = browser.clone();
    let fav_addr = address.clone();
    let fav_browser = browser;

    AnyView::new(view! {
        CardSurface(class = "gds-files-network-card") {
            row(class = "gds-files-network-card__leading") {
                Icon(name = IconName::Server)
                column(class = "gds-files-network-card__info") {
                    text(class = "gds-files-network-card__title") {{title}}
                    text(class = "gds-files-network-card__detail") {{uri}}
                }
            }
            text(
                class = "gds-files-network-card__status",
                attr:data-connected = connected.then(|| "true".to_owned()),
            ) {{status}}
            row(class = "gds-files-network-card__actions") {
                {if connected {
                    AnyView::new(view! {
                        Button(
                            variant = ButtonVariant::Outline,
                            on:click = move |_| open_browser.open(Location::Remote(open_addr.clone())),
                        ) {{i18n::text("remote-open")}}
                    })
                } else {
                    AnyView::new(view! {
                        Button(
                            variant = ButtonVariant::Outline,
                            disabled = connecting,
                            on:click = move |_| connect_and_open(connect_browser.clone(), connect_addr.clone()),
                        ) {{i18n::text("remote-connect-action")}}
                    })
                }}
                {connected.then(|| view! {
                    IconButton(
                        icon = IconName::Unplug,
                        label = i18n::text("remote-disconnect"),
                        on:click = move |_| disconnect_address(disconnect_browser.clone(), disconnect_addr.clone()),
                    )
                }.into_view())}
                {if saved {
                    AnyView::new(view! {
                        IconButton(
                            icon = IconName::StarOff,
                            label = i18n::text("remote-remove-saved"),
                            on:click = move |_| {
                                fav_browser.update(|state| {
                                    state.places.set_remote_favorite(fav_addr.clone(), false);
                                });
                                fav_browser.save_places();
                                fav_browser.refresh();
                            },
                        )
                    })
                } else {
                    AnyView::new(view! {
                        IconButton(
                            icon = IconName::Star,
                            label = i18n::text("favorite-add"),
                            on:click = move |_| {
                                fav_browser.update(|state| {
                                    state.places.set_remote_favorite(fav_addr.clone(), true);
                                });
                                fav_browser.save_places();
                                fav_browser.changed();
                            },
                        )
                    })
                }}
            }
        }
    })
}

fn mounted_location_card(browser: Browser, path: PathBuf) -> AnyView {
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string());
    let detail = path.display().to_string();
    AnyView::new(view! {
        CardSurface(class = "gds-files-network-card") {
            row(class = "gds-files-network-card__leading") {
                Icon(name = IconName::Network)
                column(class = "gds-files-network-card__info") {
                    text(class = "gds-files-network-card__title") {{title}}
                    text(class = "gds-files-network-card__detail") {{detail}}
                }
            }
            row(class = "gds-files-network-card__actions") {
                Button(
                    variant = ButtonVariant::Outline,
                    on:click = move |_| browser.open(Location::from(path.clone())),
                ) {{i18n::text("remote-open")}}
            }
        }
    })
}

fn network_section(title_key: &'static str, cards: Vec<AnyView>) -> AnyView {
    AnyView::new(view! {
        column(class = "gds-files-network-section") {
            text(class = "gds-files-network-section__title") {{i18n::text(title_key)}}
            column(class = "gds-files-network-list") {
                {cards}
            }
        }
    })
}

impl Browser {
    pub(in crate::pages::workspace) fn network_content(&self) -> AnyView {
        install_network_stylesheet();
        self.track();

        let (saved, mounts, error) = self.read(|state| {
            let saved: Vec<Address> = state
                .places
                .favorites
                .iter()
                .filter_map(|location| match location {
                    Location::Remote(address) => Some(address.clone()),
                    _ => None,
                })
                .collect();
            let (mounts, error) = match &state.tab().contents {
                Contents::Network { mounts, error } => (mounts.clone(), error.clone()),
                _ => (Vec::new(), None),
            };
            (saved, mounts, error)
        });

        let pending = self.0.connector.pending();
        let pending_key = pending.as_ref().map(|(addr, _)| addr.connection_key());
        let connections = self.0.connector.connections();

        let saved_keys: HashSet<String> = saved.iter().map(Address::connection_key).collect();
        let connected_extra: Vec<Address> = connections
            .into_iter()
            .filter(|addr| !saved_keys.contains(&addr.connection_key()))
            .collect();

        let pending_notice = pending.map(|(address, intent)| {
            let label = match intent {
                Intent::Add => i18n::text("remote-adding"),
                Intent::Open => format!("{} {}", i18n::text("remote-connecting"), address.uri()),
            };
            let cancel_browser = self.clone();
            AnyView::new(view! {
                PageNotice(class = "gds-files-network-notice") {
                    row(class = "gds-files-network-notice__lead") {
                        Spinner(label = label.clone())
                        text(class = "gds-files-network-notice__text") {{label}}
                    }
                    Button(
                        variant = ButtonVariant::Outline,
                        on:click = move |_| cancel_browser.0.connector.cancel(),
                    ) {{i18n::text("dialog-cancel")}}
                }
            })
        });

        let error_notice = error.map(|message| {
            let retry_browser = self.clone();
            AnyView::new(view! {
                PageNotice(class = "gds-files-network-notice gds-files-network-notice--error") {
                    row(class = "gds-files-network-notice__lead") {
                        Icon(name = IconName::CircleAlert)
                        text(class = "gds-files-network-notice__text") {{message}}
                    }
                    Button(
                        variant = ButtonVariant::Outline,
                        on:click = move |_| retry_browser.refresh(),
                    ) {{i18n::text("retry")}}
                }
            })
        });

        let has_pending = pending_notice.is_some();
        let has_error = error_notice.is_some();
        let empty = saved.is_empty() && connected_extra.is_empty() && mounts.is_empty();

        let mut sections = Vec::new();
        if !saved.is_empty() {
            let cards = saved
                .into_iter()
                .map(|address| {
                    let connected = self.0.connector.session(&address).is_some();
                    let connecting = pending_key.as_deref() == Some(&address.connection_key());
                    remote_location_card(self.clone(), address, true, connected, connecting)
                })
                .collect();
            sections.push(network_section("remote-saved-locations", cards));
        }

        if !connected_extra.is_empty() {
            let cards = connected_extra
                .into_iter()
                .map(|address| {
                    let connecting = pending_key.as_deref() == Some(&address.connection_key());
                    remote_location_card(self.clone(), address, false, true, connecting)
                })
                .collect();
            sections.push(network_section("remote-connected-servers", cards));
        }

        if !mounts.is_empty() {
            let cards = mounts
                .into_iter()
                .map(|path| mounted_location_card(self.clone(), path))
                .collect();
            sections.push(network_section("remote-mounted-folders", cards));
        }

        let add_header_browser = self.clone();
        let refresh_browser = self.clone();
        let empty_add_browser = self.clone();

        AnyView::new(view! {
            ContentPage {
                PageHeader {
                    PageTitle {{i18n::text("network")}}
                    PageActions {
                        Button(
                            variant = ButtonVariant::Outline,
                            size = ButtonSize::Sm,
                            on:click = move |_| open_add_location_prompt(add_header_browser.clone()),
                        ) {
                            Icon(name = IconName::Plus)
                            text {{i18n::text("remote-add-location")}}
                        }
                        IconButton(
                            icon = IconName::RefreshCw,
                            label = i18n::text("refresh"),
                            on:click = move |_| refresh_browser.refresh(),
                        )
                    }
                }
                PageContent {
                    {pending_notice}
                    {error_notice}
                    {if empty && !has_pending && !has_error {
                        AnyView::new(view! {
                            column(class = "gds-files-network-empty") {
                                Icon(name = IconName::Network, class = "gds-files-network-empty__icon")
                                text(class = "gds-files-network-empty__text") {{i18n::text("remote-network-empty")}}
                                Button(
                                    variant = ButtonVariant::Default,
                                    on:click = move |_| open_add_location_prompt(empty_add_browser.clone()),
                                ) {
                                    Icon(name = IconName::Plus)
                                    text {{i18n::text("remote-add-location")}}
                                }
                            }
                        })
                    } else {
                        AnyView::new(view! {
                            column(class = "gds-files-network-scroll") {
                                {sections}
                            }
                        })
                    }}
                }
            }
        })
    }
}
