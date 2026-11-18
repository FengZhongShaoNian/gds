//! Shared About content, independent of its dialog, page, or window host.

mod metadata;
#[cfg(test)]
mod tests;

pub use metadata::{APPLICATIONS, Application, Credit};

use std::process::{Command, Stdio};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view::PropValue;
use zgui::vocab::{Key, Modifiers, NamedKey, Role};
use zgui::{component, view};
use zgui_ui::{ButtonVariant, ScrollAreaProps};

use crate::app::Gds;
use crate::ui::button::{ButtonProps, ButtonSize, IconButtonProps};
use crate::ui::icon::{IconName, IconProps};

pub const WIDTH: f32 = 480.;
pub const HEIGHT: f32 = 360.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AboutPage {
    #[default]
    Overview,
    Credits,
    License,
}

fn localized(key: &'static str, fallback: &'static str) -> String {
    Gds::try_current().map_or_else(|| fallback.to_owned(), |gds| gds.i18n().text(key))
}

fn is_http_url(url: &str) -> bool {
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
fn SubpageHeader(
    title_key: &'static str,
    title_fallback: &'static str,
    on_back: UnsyncCallback<()>,
) -> impl IntoView {
    view! {
        row(class = "gds-about__header") {
            IconButton(
                icon = IconName::ArrowLeft,
                label = Signal::derive_local(|| localized("gds-about-back", "Back")),
                size = ButtonSize::IconSm,
                on:click = move |_| on_back.run(()),
            )
            text(class = "gds-about__section-title", a11y:role = Role::Heading) {{move || {
                localized(title_key, title_fallback)
            }}}
        }
    }
}

#[component]
fn CopyableMeta(
    value: &'static str,
    copied: RwSignal<bool, LocalStorage>,
    label_key: &'static str,
    label_fallback: &'static str,
) -> impl IntoView {
    let icon = Signal::derive_local(move || {
        if copied.get() {
            IconName::Check
        } else {
            IconName::Copy
        }
    });
    let label = Signal::derive_local(move || {
        let field = localized(label_key, label_fallback);
        let action = localized("gds-about-copy", "Copy");
        format!("{action} {field}")
    });
    view! {
        row(class = "gds-about__meta") {
            text(class = "gds-about__version") {{value}}
            IconButton(
                icon = icon,
                label = label,
                size = ButtonSize::IconXs,
                on:click = move |_| copy_text(value, copied),
            )
        }
    }
}

#[component]
fn OverviewView(
    application: &'static Application,
    page: RwSignal<AboutPage, LocalStorage>,
) -> impl IntoView {
    let copied_version = RwSignal::new_local(false);
    let copied_build = RwSignal::new_local(false);
    let artwork = application.artwork_svg();
    let has_build_tag = !application.build_tag.is_empty();
    let has_website = is_http_url(application.website);
    let has_feedback = is_http_url(application.feedback);
    let has_credits = !application.authors.is_empty() || !application.credits.is_empty();
    let has_license = !application.license.is_empty() || !application.license_text.is_empty();

    view! {
        column(class = "gds-about__overview") {
            column(class = "gds-about__hero") {
                box(class = "gds-about__icon") {
                    {match artwork {
                        Some(markup) => AnyView::new(view! {
                            vector(class = "gds-about__icon", prop:svg = PropValue::from(markup))
                        }),
                        None => AnyView::new(view! {
                            Icon(name = IconName::Files, size = 48.0)
                        }),
                    }}
                }
                text(class = "gds-about__name", a11y:role = Role::Heading) {{application.name}}
                row(class = "gds-about__meta") {
                    CopyableMeta(
                        value = application.version,
                        copied = copied_version,
                        label_key = "gds-about-version",
                        label_fallback = "Version",
                    ) {}
                    {has_build_tag.then(|| view! {
                        CopyableMeta(
                            value = application.build_tag,
                            copied = copied_build,
                            label_key = "gds-about-build-tag",
                            label_fallback = "Build tag",
                        )
                    }.into_view())}
                }
            }

            column(class = "gds-about__rows") {
                {has_website.then(|| {
                    let website = application.website;
                    view! {
                        Button(
                            variant = ButtonVariant::Ghost,
                            class = "gds-about__row",
                            on:click = move |_| open_external_url(website),
                        ) {
                            text {{move || localized("gds-about-website", "Website")}}
                            Icon(name = IconName::ChevronRight)
                        }
                    }.into_view()
                })}
                {has_feedback.then(|| {
                    let feedback = application.feedback;
                    view! {
                        Button(
                            variant = ButtonVariant::Ghost,
                            class = "gds-about__row",
                            on:click = move |_| open_external_url(feedback),
                        ) {
                            text {{move || localized("gds-about-feedback", "Feedback")}}
                            Icon(name = IconName::ChevronRight)
                        }
                    }.into_view()
                })}
                {has_credits.then(|| {
                    view! {
                        Button(
                            variant = ButtonVariant::Ghost,
                            class = "gds-about__row",
                            on:click = move |_| page.set(AboutPage::Credits),
                        ) {
                            text {{move || localized("gds-about-credits", "Credits")}}
                            Icon(name = IconName::ChevronRight)
                        }
                    }.into_view()
                })}
                {has_license.then(|| {
                    view! {
                        Button(
                            variant = ButtonVariant::Ghost,
                            class = "gds-about__row",
                            on:click = move |_| page.set(AboutPage::License),
                        ) {
                            text {{move || localized("gds-about-license", "License")}}
                            row {
                                text(class = "gds-about__version") {{application.license}}
                                Icon(name = IconName::ChevronRight)
                            }
                        }
                    }.into_view()
                })}
            }
        }
    }
}

#[component]
fn CreditsView(
    application: &'static Application,
    page: RwSignal<AboutPage, LocalStorage>,
) -> impl IntoView {
    let has_authors = !application.authors.is_empty();
    let has_credits = !application.credits.is_empty();
    view! {
        column(class = "gds-about__credits") {
            SubpageHeader(
                title_key = "gds-about-credits",
                title_fallback = "Credits",
                on_back = UnsyncCallback::new(move |()| page.set(AboutPage::Overview)),
            )
            ScrollArea(class = "gds-about__scroll") {
                column(class = "gds-about__rows") {
                    {has_authors.then(|| view! {
                        column(class = "gds-about__rows") {
                            text(class = "gds-about__section-title") {{move || {
                                localized("gds-about-authors", "Developers")
                            }}}
                            for author in move || application.authors.iter().copied(), key = |author| *author {
                                text(class = "gds-about__prose") {{author}}
                            }
                        }
                    }.into_view())}
                    {has_credits.then(|| view! {
                        column(class = "gds-about__rows") {
                            text(class = "gds-about__section-title") {{move || {
                                localized("gds-about-projects", "Open-source projects used")
                            }}}
                            for credit in move || application.credits.iter(), key = |credit| credit.name {
                                {if is_http_url(credit.website) {
                                    let website = credit.website;
                                    AnyView::new(view! {
                                        Button(
                                            variant = ButtonVariant::Ghost,
                                            class = "gds-about__row",
                                            on:click = move |_| open_external_url(website),
                                        ) {
                                            text {{credit.name}}
                                            text(class = "gds-link") {{website}}
                                        }
                                    })
                                } else {
                                    AnyView::new(view! {
                                        row(class = "gds-about__row") {
                                            text {{credit.name}}
                                        }
                                    })
                                }}
                            }
                        }
                    }.into_view())}
                }
            }
        }
    }
}

#[component]
fn LicenseView(
    application: &'static Application,
    page: RwSignal<AboutPage, LocalStorage>,
) -> impl IntoView {
    let body = if application.license_text.is_empty() {
        application.license
    } else {
        application.license_text
    };
    view! {
        column(class = "gds-about__license") {
            SubpageHeader(
                title_key = "gds-about-license",
                title_fallback = "License",
                on_back = UnsyncCallback::new(move |()| page.set(AboutPage::Overview)),
            )
            ScrollArea(class = "gds-about__scroll") {
                text(class = "gds-about__prose") {{body}}
            }
        }
    }
}

/// Shared application About view displaying version, authors, credits, and license.
#[component]
pub fn About(
    application: &'static Application,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let page = RwSignal::new_local(AboutPage::Overview);

    view! {
        column(
            class = "gds-about",
            class = class,
            attr:data-binary = application.binary,
            on:key_down = move |ev: &mut EventCx<'_, events::KeyDown>| {
                if page.get_untracked() != AboutPage::Overview
                    && ((ev.key == Key::Named(NamedKey::Escape) && ev.modifiers == Modifiers::NONE)
                        || (ev.modifiers.alt() && ev.key == Key::Named(NamedKey::ArrowLeft)))
                {
                    page.set(AboutPage::Overview);
                    ev.stop_propagation();
                }
            },
            {..attrs}
        ) {
            {move || match page.get() {
                AboutPage::Overview => AnyView::new(view! {
                    OverviewView(application = application, page = page)
                }),
                AboutPage::Credits => AnyView::new(view! {
                    CreditsView(application = application, page = page)
                }),
                AboutPage::License => AnyView::new(view! {
                    LicenseView(application = application, page = page)
                }),
            }}
        }
    }
}
