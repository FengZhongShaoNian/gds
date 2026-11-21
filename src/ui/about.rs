//! Shared About content, independent of its dialog, page, or window host.

mod metadata;
#[cfg(test)]
mod tests;

pub use metadata::{APPLICATIONS, Application, Credit};

use std::process::{Command, Stdio};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, Signal, UnsyncCallback};
use zgui::view::{ListenerOptions, PropValue, events, install_stylesheet};
use zgui::vocab::{Key, Modifiers, NamedKey, Role};
use zgui::{component, css, view};
use zgui_ui::overlay::OverlayState;
use zgui_ui::{ButtonSize, ButtonVariant};
use zgui_ui_primitives::Binding;
use zgui_ui_primitives::prelude::{DismissReason, DismissableLayerProps};

use crate::app::Gds;
use crate::appearance::tokens::SIDEBAR_ICON_SIZE;
use crate::ui::button::{ButtonProps, IconButtonProps};
use crate::ui::dialog::{DialogAnchor, ModalDialogProps};
use crate::ui::icon::{IconName, IconProps};
use crate::ui::page::{ContentPageProps, PageContentProps, PageHeaderProps, PageTitleProps};
use crate::ui::shortcut::Shortcut;
use crate::ui::tooltip::HintTooltipProps;

pub const WIDTH: f32 = 480.;
pub const HEIGHT: f32 = 360.;

const ABOUT_SHEET: &str = "gds-ui-about";

const ABOUT_CSS: &str = css!(
    r#"
    .gds-about.gds-about {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
        padding: 0px;
        gap: 0px;
        outline: none;
        color: var(--gds-foreground);
        font-size: var(--gds-font-size-small, 14px);
    }

    .gds-about__brand-icon {
        width: 18px;
        height: 18px;
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: var(--gds-foreground);
    }

    .gds-about__body.gds-about__body {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        padding: 0px 24px 16px 24px;
        overflow: hidden;
    }

    .gds-about__overview {
        display: flex;
        flex-direction: column;
        min-width: 0px;
        min-height: 0px;
        gap: 16px;
    }

    .gds-about__dl {
        display: flex;
        flex-direction: column;
        min-width: 0px;
        gap: 10px;
    }

    .gds-about__dl-item {
        display: flex;
        flex-direction: row;
        align-items: baseline;
        gap: 16px;
        min-width: 0px;
    }

    .gds-about__dt {
        width: 120px;
        flex-shrink: 0;
        color: var(--gds-muted-foreground);
        font-size: var(--gds-font-size-small, 14px);
        line-height: 1.4;
    }

    .gds-about__dd {
        flex: 1 1 0%;
        min-width: 0px;
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 8px;
        color: var(--gds-foreground);
        font-size: var(--gds-font-size-small, 14px);
        line-height: 1.4;
    }

    .gds-about__selectable {
        min-width: 0px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        user-select: text;
    }

    .gds-about__links {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        align-items: center;
        gap: 16px;
    }

    .gds-about__link {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        padding: 0px;
        border: none;
        border-radius: 4px;
        background: transparent;
        color: var(--gds-link);
        font-size: var(--gds-font-size-small, 14px);
        cursor: pointer;
        text-decoration: none;
    }

    .gds-about__link:hover {
        text-decoration: underline;
    }

    .gds-about__link:focus-visible {
        outline: 1px solid var(--gds-primary);
        outline-offset: 2px;
    }

    .gds-about__actions {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        align-items: center;
        gap: 8px;
    }

    .gds-about__details-wrap {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        gap: 16px;
    }

    .gds-about__subheader {
        display: flex;
        flex-direction: row;
        align-items: center;
        flex-shrink: 0;
        gap: 8px;
        min-width: 0px;
    }

    .gds-about__subtitle {
        flex: 1 1 0%;
        min-width: 0px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: calc(16px * var(--gds-font-scale, 1));
        font-weight: 600;
        color: var(--gds-foreground);
    }

    .gds-about__details {
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        overflow-y: auto;
        overflow-x: hidden;
    }

    .gds-about__projects {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: 8px;
        min-width: 0px;
    }

    .gds-about__authors {
        min-width: 0px;
        overflow-wrap: anywhere;
        user-select: text;
    }

    .gds-about__license {
        display: flex;
        flex-direction: column;
        gap: 16px;
        min-width: 0px;
        font-size: var(--gds-font-size-small, 14px);
    }

    .gds-about__license-id {
        font-weight: 600;
        color: var(--gds-foreground);
    }

    .gds-about__license-text {
        white-space: pre-wrap;
        overflow-wrap: anywhere;
        line-height: 1.5;
        color: var(--gds-foreground);
        user-select: text;
    }

    .gds-about-dialog.gds-about-dialog {
        width: 480px;
        height: 360px;
        max-width: calc(100vw - 32px);
        max-height: calc(100vh - 32px);
        padding: 0px;
        overflow: hidden;
    }
    "#
);

fn install_about_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    install_stylesheet(ABOUT_SHEET, ABOUT_CSS);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Section {
    #[default]
    Overview,
    Credits,
    License,
}

pub(crate) fn localized(key: &'static str, fallback: &'static str) -> String {
    Gds::try_current().map_or_else(|| fallback.to_owned(), |gds| gds.i18n().text(key))
}

pub(crate) fn is_http_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.starts_with("https://") || trimmed.starts_with("http://")
}

fn spawn_url_command(url: &str) {
    #[cfg(target_os = "linux")]
    let result = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd")
        .args(["/C", "start", "", url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    if let Err(err) = result {
        tracing::warn!(%err, url, "failed to launch external URL from About view");
    }
}

fn open_external_url(url: &str) {
    let trimmed = url.trim();
    if !is_http_url(trimmed) {
        return;
    }
    let url_owned = trimmed.to_owned();
    if let Some(gds) = Gds::try_current()
        && gds.tasks().scope().runtime_handle().is_some()
    {
        gds.tasks().spawn_ui(
            "about.open_url",
            async move { spawn_url_command(&url_owned) },
            |_| {},
        );
    } else {
        spawn_url_command(&url_owned);
    }
}

fn copy_text(text: &'static str, copied: RwSignal<bool, LocalStorage>) {
    if let Some(clipboard) = try_use_clipboard() {
        clipboard.set_text(ClipboardKind::Standard, text);
    }
    copied.set(true);
}

#[component]
fn IdentityRow(
    id: &'static str,
    text_id: &'static str,
    label_key: &'static str,
    label_fallback: &'static str,
    value: &'static str,
) -> impl IntoView {
    let copied = RwSignal::new_local(false);
    let icon = Signal::derive_local(move || {
        if copied.get() {
            IconName::Check
        } else {
            IconName::Copy
        }
    });
    let field_label = Signal::derive_local(move || localized(label_key, label_fallback));
    let accessible = Signal::derive_local(move || {
        let label = localized(label_key, label_fallback);
        format!("{label}: {value}")
    });
    let copy_tip = Signal::derive_local(|| localized("gds-about-copy", "Copy"));

    view! {
        row(class = "gds-about__dl-item") {
            text(class = "gds-about__dt") {{move || field_label.get()}}
            row(class = "gds-about__dd") {
                text(class = "gds-about__selectable", attr:data-testid = text_id) {{value}}
                HintTooltip(text = copy_tip) {
                    IconButton(
                        icon = icon,
                        label = accessible,
                        size = ButtonSize::IconXs,
                        tooltip = false,
                        attr:data-testid = id,
                        on:click = move |_| copy_text(value, copied),
                    )
                }
            }
        }
    }
}

#[component]
fn ExternalLink(
    id: &'static str,
    href: &'static str,
    #[prop(into)] label: Signal<String, LocalStorage>,
) -> impl IntoView {
    view! {
        control(
            class = "gds-about__link",
            tabindex = Focus::Sequential,
            a11y:role = Role::Link,
            a11y:label = move || label.get(),
            attr:data-testid = id,
            on:click = move |_| open_external_url(href),
            on:key_down = move |ev: &mut EventCx<'_, events::KeyDown>| {
                if !ev.repeat
                    && ev.modifiers.held().is_empty()
                    && (ev.key == Key::Named(NamedKey::Enter)
                        || ev.key == Key::Character(" ".into()))
                {
                    open_external_url(href);
                    ev.prevent_default();
                    ev.stop_propagation();
                }
            },
        ) {
            text {{move || label.get()}}
        }
    }
}

#[component]
fn OverviewSection(
    application: &'static Application,
    section: RwSignal<Section, LocalStorage>,
    root_ref: NodeRef,
) -> impl IntoView {
    let has_website = is_http_url(application.website);
    let has_feedback = is_http_url(application.feedback);
    let has_links = has_website || has_feedback;
    let has_credits = !application.authors.is_empty() || !application.credits.is_empty();
    let has_license = !application.license.is_empty() || !application.license_text.is_empty();

    view! {
        column(class = "gds-about__overview", attr:data-testid = "about-overview") {
            column(class = "gds-about__dl") {
                IdentityRow(
                    id = "about-version",
                    text_id = "about-version-text",
                    label_key = "gds-about-version",
                    label_fallback = "Version",
                    value = application.version,
                )
                IdentityRow(
                    id = "about-build-tag",
                    text_id = "about-build-tag-text",
                    label_key = "gds-about-build-tag",
                    label_fallback = "Build tag",
                    value = application.build_tag,
                )
            }

            {has_links.then(|| {
                view! {
                    row(class = "gds-about__links") {
                        {has_website.then(|| {
                            view! {
                                ExternalLink(
                                    id = "about-website",
                                    href = application.website,
                                    label = Signal::derive_local(|| {
                                        localized("gds-about-website", "Website")
                                    }),
                                )
                            }
                            .into_view()
                        })}
                        {has_feedback.then(|| {
                            view! {
                                ExternalLink(
                                    id = "about-feedback",
                                    href = application.feedback,
                                    label = Signal::derive_local(|| {
                                        localized("gds-about-feedback", "Feedback")
                                    }),
                                )
                            }
                            .into_view()
                        })}
                    }
                }
                .into_view()
            })}

            row(class = "gds-about__actions") {
                {has_credits.then(|| {
                    view! {
                        Button(
                            variant = ButtonVariant::Outline,
                            size = ButtonSize::Sm,
                            attr:data-testid = "about-credits",
                            on:click = move |_| {
                                section.set(Section::Credits);
                                root_ref.focus();
                            },
                        ) {
                            Icon(name = IconName::Info, size = 16.0)
                            text {{move || localized("gds-about-credits", "Credits")}}
                        }
                    }
                    .into_view()
                })}
                {has_license.then(|| {
                    view! {
                        Button(
                            variant = ButtonVariant::Outline,
                            size = ButtonSize::Sm,
                            attr:data-testid = "about-license",
                            on:click = move |_| {
                                section.set(Section::License);
                                root_ref.focus();
                            },
                        ) {
                            Icon(name = IconName::Scale, size = 16.0)
                            text {{move || localized("gds-about-license", "License")}}
                        }
                    }
                    .into_view()
                })}
            }
        }
    }
}

#[component]
fn CreditsBody(application: &'static Application) -> impl IntoView {
    let authors_joined = application.authors.join(", ");
    view! {
        column(class = "gds-about__dl") {
            row(class = "gds-about__dl-item") {
                text(class = "gds-about__dt") {{move || localized("gds-about-authors", "Developers")}}
                row(class = "gds-about__dd") {
                    text(class = "gds-about__authors") {{authors_joined}}
                }
            }
            row(class = "gds-about__dl-item") {
                text(class = "gds-about__dt") {{move || {
                    localized("gds-about-projects", "Open-source projects used")
                }}}
                column(class = "gds-about__projects") {
                    for credit in move || application.credits.iter(), key = |credit| credit.name {
                        {if is_http_url(credit.website) {
                            let name = credit.name;
                            let href = credit.website;
                            AnyView::new(view! {
                                ExternalLink(
                                    id = name,
                                    href = href,
                                    label = Signal::stored_local(name.to_owned()),
                                )
                            })
                        } else {
                            AnyView::new(view! {
                                text {{credit.name}}
                            })
                        }}
                    }
                }
            }
        }
    }
}

#[component]
fn LicenseBody(application: &'static Application) -> impl IntoView {
    let license_body = if application.license_text.is_empty() {
        application.license
    } else {
        application.license_text
    };
    view! {
        column(class = "gds-about__license") {
            text(class = "gds-about__license-id") {{application.license}}
            text(class = "gds-about__license-text", attr:data-testid = "about-license-text") {
                {license_body}
            }
        }
    }
}

#[component]
fn DetailsSection(
    application: &'static Application,
    which: Section,
    section: RwSignal<Section, LocalStorage>,
    root_ref: NodeRef,
) -> impl IntoView {
    let (title_key, title_fallback) = match which {
        Section::Credits => ("gds-about-credits", "Credits"),
        Section::License => ("gds-about-license", "License"),
        Section::Overview => ("gds-about-credits", "Credits"),
    };
    let back_shortcut = Shortcut::new(Key::Named(NamedKey::ArrowLeft), Modifiers::ALT);
    let go_back = move || {
        section.set(Section::Overview);
        root_ref.focus();
    };
    let host_overlay = OverlayState::current();
    let exclude_surface = host_overlay.map_or(root_ref, |overlay| overlay.content());
    let on_dismiss = UnsyncCallback::new(move |reason: DismissReason| match reason {
        DismissReason::EscapeKey => go_back(),
        DismissReason::OutsidePress => {
            if let Some(overlay) = host_overlay {
                overlay.close();
            }
        }
        _ => go_back(),
    });

    view! {
        DismissableLayer(
            layer = OverlayLayer::Modal,
            class = "gds-about__details-wrap",
            exclude = exclude_surface,
            dismiss_on_outside_press = host_overlay.is_some(),
            dismiss_on_escape = true,
            on_dismiss = on_dismiss,
        ) {
            row(class = "gds-about__subheader") {
                IconButton(
                    icon = IconName::ArrowLeft,
                    label = Signal::derive_local(|| localized("gds-about-back", "Back")),
                    shortcut = back_shortcut,
                    attr:data-testid = "about-back",
                    on:click = move |_| go_back(),
                )
                text(
                    class = "gds-about__subtitle",
                    a11y:role = Role::Heading,
                    a11y:level = 2usize,
                ) {{move || localized(title_key, title_fallback)}}
            }
            scroll(class = "gds-about__details", attr:data-testid = "about-details") {
                {match which {
                    Section::Credits => AnyView::new(view! { CreditsBody(application = application) }),
                    Section::License => AnyView::new(view! { LicenseBody(application = application) }),
                    Section::Overview => AnyView::new(()),
                }}
            }
        }
    }
}

pub(crate) fn is_back_key(key: &Key, modifiers: Modifiers) -> bool {
    let held = modifiers.held();
    (held.is_empty() && *key == Key::Named(NamedKey::Escape))
        || (held == Modifiers::ALT && *key == Key::Named(NamedKey::ArrowLeft))
}

#[cfg(test)]
pub(crate) mod test_probe {
    use super::*;
    use std::cell::RefCell;

    pub(crate) struct AboutProbe {
        pub section: RwSignal<Section, LocalStorage>,
        pub root_ref: NodeRef,
    }

    thread_local! {
        pub(crate) static PROBE: RefCell<Option<AboutProbe>> = const { RefCell::new(None) };
    }
}

/// Shared application About view displaying `ContentPage` header, copyable version/build rows,
/// external Website/Feedback links, Credits, and embedded Apache-2.0 License subpages.
#[component]
pub fn About(
    application: &'static Application,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    install_about_stylesheets();
    let section = RwSignal::new_local(Section::Overview);
    let root_ref = NodeRef::new();
    let artwork = application.artwork_svg();

    #[cfg(test)]
    test_probe::PROBE.with(|slot| {
        *slot.borrow_mut() = Some(test_probe::AboutProbe { section, root_ref });
    });

    let capture_back = Attrs::new().listener(
        events::KEY_DOWN,
        ListenerOptions::CAPTURE,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            if !ev.repeat
                && section.get_untracked() != Section::Overview
                && is_back_key(&ev.key, ev.modifiers)
            {
                section.set(Section::Overview);
                root_ref.focus();
                ev.prevent_default();
                ev.stop_propagation();
            }
        },
    );

    view! {
        column(
            class = "gds-about",
            class = class,
            node_ref = root_ref,
            tabindex = Focus::Programmatic,
            attr:data-binary = application.binary,
            attr:data-testid = "about-content",
            {..capture_back},
            {..attrs}
        ) {
            ContentPage {
                PageHeader {
                    box(class = "gds-about__brand-icon") {
                        {match artwork {
                            Some(markup) => AnyView::new(view! {
                                vector(
                                    class = "gds-about__brand-icon",
                                    prop:svg = PropValue::from(markup),
                                )
                            }),
                            None => AnyView::new(view! {
                                Icon(name = IconName::Info, size = SIDEBAR_ICON_SIZE)
                            }),
                        }}
                    }
                    PageTitle {{application.name}}
                }
                PageContent(class = "gds-about__body") {
                    {move || match section.get() {
                        Section::Overview => AnyView::new(view! {
                            OverviewSection(
                                application = application,
                                section = section,
                                root_ref = root_ref,
                            )
                        }),
                        which @ (Section::Credits | Section::License) => AnyView::new(view! {
                            DetailsSection(
                                application = application,
                                which = which,
                                section = section,
                                root_ref = root_ref,
                            )
                        }),
                    }}
                }
            }
        }
    }
}

pub use About as AboutView;
pub use AboutProps as AboutViewProps;

/// Modal dialog wrapper hosting [`About`] with `DialogScope` isolation and `Alt+C` / `Esc` dismissal.
#[component]
pub fn AboutDialog(
    #[prop(into, optional)] open: Binding<bool>,
    application: &'static Application,
    #[prop(optional)] anchor: Option<DialogAnchor>,
    #[prop(optional)] on_close: Option<UnsyncCallback<()>>,
) -> impl IntoView {
    install_about_stylesheets();
    let on_decision = on_close.map(|cb| UnsyncCallback::new(move |_| cb.run(())));

    view! {
        ModalDialog(
            open = open,
            disabled = true,
            anchor = anchor,
            on_decision = on_decision,
            class = "gds-about-dialog",
        ) {
            About(application = application)
        }
    }
}

/// Convenience helper returning an [`AnyView`] modal About dialog controlled by `open`.
#[must_use]
pub fn open_about_dialog(
    open: impl Into<Binding<bool>>,
    application: &'static Application,
) -> AnyView {
    let open = open.into();
    AnyView::new(view! {
        AboutDialog(open = open, application = application)
    })
}
